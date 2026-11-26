- [ ] add post processing shaders to the shaders crate
- [ ] add underwater post process shader that handles partial submission
- [ ] use this post process in game

Based on your codebase (Bevy `0.20.1`, `crates/bevy_shader_kit`, `tickets/incomplete/post-processing.md`, and the `CLAUDE.md` module-isolation rules), here is the best plan.

---

### Core Architectural Decision: Custom `Core3d` System over `FullscreenMaterial`

Bevy 0.20 introduced `FullscreenMaterialPlugin` (`bevy::core_pipeline::fullscreen_material`), which is great for simple 2D/color-only effects, **but its bind group layout is hardcoded to `(screen_texture, sampler, uniform)`**—it does **not** bind `ViewUniform` or the depth buffer.

For **partial-submersion underwater post-processing**, you need both:
1. **Camera near-plane world-space reconstruction** (`view.world_from_clip`, `view.world_position`) to test where the waterline cuts across the camera lens per pixel.
2. **Scene depth** (`ViewPrepassTextures::depth_only_view()`, which `MainCamera` in `src/camera.rs` already enables via `DepthPrepass`) to compute the water ray-travel distance before hitting terrain/objects.

In Bevy 0.20, `ViewNode` / `Node3d` was replaced by plain ECS systems in the `Core3d` schedule using `ViewQuery` and `RenderContext`. Putting a small generic `PostProcessEffect` framework in `bevy_shader_kit` built on `Core3d` gives you zero-boilerplate reuse for underwater and any future post-process effects.

---

### Phase 1 — Reusable Post-Processing Framework in `crates/bevy_shader_kit`

Keep `crates/bevy_shader_kit` completely isolated from the root crate (`tests/crate_isolation.rs`).

1. **Manifest (`crates/bevy_shader_kit/Cargo.toml`)**
   - Add `"bevy_core_pipeline"` (and `"bevy_camera"` if needed for `Camera`/`ExtractComponent` types) to the `bevy` dependency features.

2. **Generic Pass & Pipeline (`crates/bevy_shader_kit/src/post_process.rs`)**
   - Define a trait + plugin pair (`PostProcessEffect` + `PostProcessPlugin<T>`), where `T: Component + ExtractComponent + Clone + Copy + ShaderType + WriteInto + Default`:
     - `fn fragment_shader() -> ShaderRef`
     - `fn stage() -> Core3dSystems` (default `Core3dSystems::EarlyPostProcess`, so HDR linear lighting/absorption/scattering run **before** Bloom, Tonemapping, and UI)
     - Optional `fn is_enabled(&self) -> bool` (skip the pass with zero GPU cost when disabled or completely above water)
     - Optional `fn override_shader(&self) -> Option<Handle<Shader>>` (matching `WaterMaterialExt` so `compose.rs` dynamic shaders can plug in later)
   - Register inside `PostProcessPlugin<T>`:
     - `ExtractComponentPlugin::<T>` and `UniformComponentPlugin::<T>`
     - `RenderStartup` system initializing the bind group layout and `FullscreenShader::to_vertex_state()`
     - `SpecializedRenderPipelines` keyed on `(target_format: TextureFormat, msaa_samples: u32, has_depth: bool, override_shader: Option<Uuid>)` — **specializing on `Msaa` is critical** because Bevy defaults to `Msaa::Sample4`, making `ViewPrepassTextures::depth` multisampled
     - A `Core3d` render system using:
       ```rust
       ViewQuery<(
           &ExtractedView,
           &ViewTarget,
           &ViewUniformOffset,
           &DynamicUniformIndex<T>,
           &Msaa,
           Option<&ViewPrepassTextures>,
       )>
       ```
       calling `view_target.post_process_write()`, binding group `0`:
       - `@binding(0)`: `screen_texture` (`texture_2d<f32>`)
       - `@binding(1)`: `screen_sampler` (`sampler`)
       - `@binding(2)`: `depth_texture` (`texture_depth_2d` or `texture_depth_multisampled_2d` via `@if(MULTISAMPLED)`)
       - `@binding(3)`: `view` (`View` uniform from `ViewUniforms`)
       - `@binding(4)`: `settings` (`T` uniform from `ComponentUniforms<T>`)

3. **Shared WESL Post-Process Libraries (`crates/bevy_shader_kit/src/shaders/post_process/`)**
   - Register via `load_shader_library!` in `ShaderKitPlugin::build`:
     - `bindings.wesl`: `@group(0)` bindings + `@if(MULTISAMPLED)` `sample_scene_depth(frag_coord)` helper + reverse-Z helpers:
       - `near_plane_world(uv, view)` (in Bevy's reverse-Z, NDC `z = 1.0` is the near plane)
       - `view_ray_dir(uv, view)`
       - `depth_distance(uv, depth, view)` (handling `depth == 0.0` skybox fallback)

---

### Phase 2 — Underwater Shader with Partial Submersion

1. **Component & Uniform (`crates/bevy_shader_kit/src/underwater.rs`)**
   - `#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Reflect)]` `UnderwaterPostProcess`:
     - `water_height: f32`, `time: f32`, `max_depth: f32`, `phase_g: f32`
     - `scattering: Vec3`, `absorption: Vec3`, `sun_direction: Vec3`, `sun_illuminance: Vec3`, `ambient_illuminance: Vec3` (matching `WaterParams` defaults so above-water `WaterMaterial` and underwater volume optics match seamlessly)
     - `meniscus_thickness: f32`, `meniscus_color: Vec3`, `distortion_strength: f32`, `enabled: u32`
   - Register `PostProcessPlugin::<UnderwaterPostProcess>::default()` inside `ShaderKitPlugin`.

2. **Fragment Shader (`crates/bevy_shader_kit/src/shaders/post_process/underwater.wesl`)**
   - Registered via `embedded_asset!`, reusing your existing WESL modules:
     - `package::shaders::optics::scattering::{beer_lambert_in_scatter, schlick_phase}`
     - `package::shaders::optics::fresnel::fresnel_schlick`
     - `package::shaders::noise::fbm2d::scrolling_fbm_2d`
   - **Partial submersion algorithm per pixel**:
     1. Compute `p_near = near_plane_world(uv, view)` and `rd = normalize(p_near - view.world_position)`.
     2. Compute local surface height at the lens: `surface_y = settings.water_height + meniscus_wave(p_near.xz, settings.time)` (small animated wave so the waterline across the camera gently ripples rather than looking razor-flat).
     3. Signed submersion at the lens: `submersion = surface_y - p_near.y`.
     4. **Above waterline (`submersion < -half_band`)**: return `screen_texture` unchanged (prevents double-darkening pixels above water when looking down at `Sea`).
     5. **Meniscus band (`abs(submersion) <= half_band`)**: smoothstep blend + subtle refracted highlight/dark rim right along the water–air boundary on the lens.
     6. **Below waterline (`submersion > half_band`)**:
        - Reconstruct `t_scene` from `depth_texture` (clamped to `max_depth` if `depth == 0.0`).
        - If looking upward (`rd.y > 0.0`), intersect ray `(view.world_position, rd)` with `y = settings.water_height` to get `t_plane`. Clamp underwater travel distance to `d_water = min(t_scene, t_plane)` so rays exiting into the sky only attenuate over the underwater segment, and modulate with `fresnel_schlick(0.02, abs(rd.y))` (Snell's window / total internal reflection past the critical angle).
        - Sample `screen_texture` with subtle animated UV distortion (`scrolling_fbm_2d`, faded out near the meniscus so the waterline stays clean).
        - Apply Beer-Lambert transmittance `exp(-(scattering + absorption) * d_water)` to the scene color and add `beer_lambert_in_scatter(...)`.

---

### Phase 3 — Game Integration (`src/integration/`)

Following `CLAUDE.md` (feature modules never import each other; wiring lives in `src/integration/`):

1. **New `src/integration/post_process.rs` (wired in `src/integration/mod.rs`)**
   - Observer `On<Add<MainCamera>>`: inserts `UnderwaterPostProcess::default()` onto `MainCamera`.
   - `sync_underwater_post_process` (`Update` system):
     - Reads `Time`, `WaterHeightSource` (`Transform.translation.y`), and `WaterSurface` (`Assets<WaterMaterial>`) to keep `water_height`, `time`, `scattering`, `absorption`, and sun terms synchronized with `Sea` (including runtime `set_water_level` changes).
     - Reads `DistanceFieldView` (`active`): sets `enabled = 0` while inspecting the SDF clay view so the underwater tint doesn't obscure distance-field debugging.
   - Optional `command!` (`toggle_underwater`) and `debug_info!` (`Submersion: above / partial / submerged`) for quick testing.

2. **Watch out for two existing interactions**
   - **`Sea` underside (`src/terrain/water.rs`)**: `Sea` uses `cull_mode: None` with `specular_transmission: 1.0`. Once you submerge, look upward through `Sea` to verify `water.wesl` doesn't double-apply `water_volume_in_scatter` on back-faces (`@if` / `front` check in `water.wesl`: when `!front`, `Sea` is seen from below, so letting the post-process handle volume scatter below water avoids double-fogging).
   - **Offscreen screenshots (`src/screenshot.rs`)**: `CaptureMode::CameraView` spawns a temporary `ScreenshotCaptureCamera` copying `DistanceFog`, `Exposure`, `Tonemapping`, and `RenderLayers`, but currently does **not** copy `DepthPrepass` or post-process components. Either pass those through or have an integration observer/hook mirror `DepthPrepass` + `UnderwaterPostProcess` onto `ScreenshotCaptureCamera` so `screenshot` captures underwater views accurately.

---

### Phase 4 — Verification

1. **Unit tests (`crates/bevy_shader_kit`)**:
   - `ShaderType` alignment/size checks for `UnderwaterPostProcess` matching the WGSL/WESL struct.
   - Pipeline specialization key tests (`Msaa` sample count, `TextureFormat`, `override_shader`).
2. **Checks**:
   - `cargo test --test crate_isolation`
   - `cargo test --workspace`
3. **Visual test in-game**:
   - Run `cargo run`, open the `~` command palette, use `set_water_level` and `angle_camera` to place the camera half-submerged, and verify the meniscus split, underwater depth fade, looking up through Snell's window, and `screenshot` output.