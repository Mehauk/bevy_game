**requires water_shader**
**requires river_shader**

# Ticket: Extract `shaderutils.wesl`, Optimize Terrain Texture Taps, and Add WESL Static Analysis

## Objective
1. Extract shared math, noise, and BRDF helpers into `assets/shaders/shaderutils.wesl` and refactor `sky_material.wesl`, `terrain_material.wesl`, and `water_material.wesl` to import from it.
2. Reduce worst-case per-pixel texture samples in `terrain_material.wesl`.
3. Add an AST-based static analysis integration test (`tests/shader_lint.rs`) that parses all `.wesl` files and enforces syntax, modularity, derivative safety, noise-octave, and texture-tap budgets on `cargo test`.

## Files in Scope
- `assets/shaders/shaderutils.wesl` (new)
- `assets/shaders/sky_material.wesl`
- `assets/shaders/terrain_material.wesl`
- `assets/shaders/water_material.wesl`
- `Cargo.toml` (`[dev-dependencies]` only)
- `tests/shader_lint.rs` (new)

## Constraints (Do Not Violate)
- **Do NOT modify Rust `Material` structs, bindings, or uniforms** unless explicitly required; preserve all `@group(constants::MATERIAL_BIND_GROUP) @binding(...)` indices and uniform struct layouts (`SkySurface`, `TerrainSurface`, `WaterSurface`) across all three shaders.
- **Keep `shaderutils.wesl` strictly stateless:** pure `fn` and `const` declarations only. No `@group`, `@binding`, `var<uniform>`, or texture/sampler declarations.
- **Use Bevy 0.20 WESL syntax:** sibling imports must use `import super::shaderutils::{...};`. Never use `#` for comments (use `//`).
- **Preserve all debug modes** (`terrain.debug_mode == 1u`, `2u`, `3u`) and `@if(VERTEX_COLORS)` conditional compilation blocks.
- **Check `Cargo.lock` first** before adding `wgsl-parse` to `[dev-dependencies]` in `Cargo.toml` so the version matches what `bevy` 0.20 already resolves (enable the `wesl` feature).

## Implementation Plan

### Part 1: Create `assets/shaders/shaderutils.wesl`
Move/consolidate shared primitives into `shaderutils.wesl`:
- `PI`
- `hash21`, `hash22`, `hash33`
- `noise_and_grad(p: vec2<f32>) -> vec3<f32>` (quintic value noise returning `(val, dx, dy)`)
- `smooth_noise2(p: vec2<f32>) -> f32`
- `fbm2(p: vec2<f32>, octaves: i32) -> f32`
- `schlick_fresnel(ndv: f32, f0: f32) -> f32`
- `ggx_specular(ndh: f32, ndl: f32, ndv: f32, roughness: f32) -> f32`

Update `sky_material.wesl`, `terrain_material.wesl`, and `water_material.wesl` to import and use these helpers instead of local duplicates:
- `sky_material.wesl`: use `fbm2(..., 4)` for clouds (do not exceed 4 octaves).
- `terrain_material.wesl`: use `fbm2(..., 2)` for macro tint/rock variation (do not exceed 2 octaves).
- `water_material.wesl`: use `noise_and_grad` for `wave_sample` and `fbm2(..., 2)` for foam/shimmer/ice cracks.

### Part 2: Optimize `terrain_material.wesl` Texture Tap Budget
Reduce worst-case texture samples per pixel while preserving visual quality:
1. Raise the splat skip threshold from `w <= 0.005` to `w <= 0.02`.
2. In the splat loop (`0u..4u`), only use 3-tap stochastic sampling (`*_st`) for the dominant/visible near weights (`w > 0.15`); use single-tap sampling (`+ st_near.o0`) for minor layers (`w <= 0.15`).
3. At far range (`far_w >= 0.99`), use **single-tap** far sampling (`uv_far + st_far.o0`) for albedo (no 3-tap stochastic blending at far distance—macro noise already hides tiling), and skip far normal/ORM texture lookups by falling back to flat tangent-space normal `(0, 0, 1)` and `(1.0, terrain.roughness, 0.0)` blended cleanly across `far_w`.
4. Ensure all `tile_mip(...)` calls remain in uniform control flow outside conditional branches.

### Part 3: Add Static Analysis (`tests/shader_lint.rs`)
Add an integration test suite using `wgsl_parse` (with `wesl` feature) that scans all `.wesl` files in `assets/shaders/` and asserts the following rules with clear diagnostic error messages (`file`, `function`, and reason):

1. **`wesl_all_shaders_parse`**:
   - Every `assets/shaders/*.wesl` parses cleanly via `wgsl_parse::parse_str` (catches syntax errors, invalid `#` comments, malformed `@if` directives).
2. **`shaderutils_is_stateless`**:
   - `shaderutils.wesl` contains zero `@group`, `@binding`, or module-level `var` declarations.
3. **`no_duplicate_shared_primitives`**:
   - Functions listed in `shaderutils.wesl` (`hash21`, `hash22`, `hash33`, `noise_and_grad`, `smooth_noise2`, `fbm2`, `schlick_fresnel`, `ggx_specular`) are not defined in any other `assets/shaders/*.wesl` file.
4. **`noise_and_loop_budget`**:
   - `fbm2` is never called inside any `for`/`while`/`loop` statement in any shader.
   - Any literal `octaves` argument to `fbm2` is `<= 2` in `terrain_material.wesl` and `water_material.wesl`, and `<= 4` in `sky_material.wesl`.
   - `dpdx` / `dpdy` (and helpers calling them like `tile_mip`) are never called inside conditional `if` or loop blocks.
5. **`texture_tap_budget`**:
   - Compute static worst-case `textureSample*` / `textureLoad` cost of `fragment()` by traversing the AST call graph:
     - Sum sequential statements.
     - Take `max(branch_costs)` across `if / else if / else` chains.
     - Multiply body cost by static trip count for bounded `for` loops (e.g., `0u..4u` -> `× 4`).
   - Enforce worst-case budgets:
     - `sky_material.wesl`: `== 0` texture taps
     - `water_material.wesl`: `== 0` texture taps
     - `terrain_material.wesl`: `<= 32` worst-case texture taps (excluding early-return `debug_mode` branches)

## Verification & Acceptance Criteria
- [ ] `cargo test --test shader_lint` compiles and passes all 5 static analysis checks.
- [ ] Intentionally violating a rule locally (e.g., temporarily adding `fbm2(..., 5)` in `terrain_material.wesl` or a `@binding` in `shaderutils.wesl`) causes `cargo test --test shader_lint` to fail with a clear message (revert the temporary test edit afterward).
- [ ] `cargo check` succeeds with no warnings introduced in non-test code.
- [ ] Running the app briefly (`cargo run`) produces **zero** `ERROR bevy_render::render_resource::pipeline_cache: failed to process shader` log lines.