- [ ] handle parameters for dynamic shaders
- [ ] dynamically discover new shaders in assets

---

### 1. Compile-Time Auto-Discovery

Automatically register every `.wesl` under `crates/bevy_shader_kit/src/shaders/` without manually maintaining `load_shader_library!` / `embedded_asset!` lists in `lib.rs`:

- **New: `crates/bevy_shader_kit/build.rs`**
  - Walks `src/shaders/**/*.wesl` and emits `cargo:rerun-if-changed=src/shaders`.
  - Classifies each file:
    - Contains `@fragment`, `@vertex`, or `@compute` -> emits `embedded_asset!(app, "shaders/...")` (`"embedded://bevy_shader_kit/shaders/..."`).
    - Otherwise -> parses any `import bevy_shader_kit::shaders::...` lines, topologically sorts libraries so dependencies register first, and emits `load_shader_library!(app, "shaders/...")`.
  - Writes `$OUT_DIR/generated_shaders.rs` defining `register_builtin_shaders(app: &mut App)`.
- **Edit: `crates/bevy_shader_kit/src/lib.rs`**
  - Replace the manual `load_shader_library!` / `embedded_asset!` block with `include!(concat!(env!("OUT_DIR"), "/generated_shaders.rs"));` and call `register_builtin_shaders(app)` before any `MaterialPlugin`.

---

### 2. Runtime-Tweakable Dynamic Scalars, Vecs, & Colors

Let `ShaderModification`s introduce new floats, vectors, and colors that can be changed live at runtime with **zero shader recompilation**:

- **New: `crates/bevy_shader_kit/src/params.rs`**
  - `ParamValue` (`F32(f32)`, `Vec2(Vec2)`, `Vec3(Vec3)`, `Vec4(Vec4)`, `Color(LinearRgba)`) and `ParamType`.
  - `DynamicParamBuffer` (`#[repr(C)] #[derive(Clone, Copy, ShaderType, Reflect, Default)]`) holding `pub slots: [Vec4; 16]` (64 scalar lanes / 16 `vec4` slots).
  - `DynamicParamSchema`:
    - Packs declared parameters into `(slot, lane, len)`: `F32` packs into `.x/.y/.z/.w`; `Vec2` packs into `.xy` or `.zw` without crossing a `Vec4` boundary; `Vec3` / `Vec4` / `Color` allocate a new `Vec4` slot.
    - Stores live values and provides `set_f32`, `set_vec2`, `set_vec3`, `set_vec4`, `set_color`, and `write_to_buffer(&mut DynamicParamBuffer)`.
    - Emits typed WESL accessor functions:
      ```wesl
      fn param_wake_strength() -> f32 { return dynamic_params.slots[0].x; }
      fn param_foam_tint() -> vec3<f32> { return dynamic_params.slots[1].xyz; }
      ```
- **Edit: `crates/bevy_shader_kit/src/shaders/water/types.wesl`**
  - Add `DynamicParamBuffer` struct (`slots: array<vec4<f32>, 16>`) and `@group(constants::MATERIAL_BIND_GROUP) @binding(105) var<uniform> dynamic_params: DynamicParamBuffer;`.
- **Edit: `crates/bevy_shader_kit/src/water.rs`**
  - Add `#[uniform(105)] pub dynamic_params: DynamicParamBuffer` to `WaterMaterialExt` (initialized to default zeros).
- **Edit: `crates/bevy_shader_kit/src/compose.rs`**
  - Add `pub params: Vec<(String, ParamValue)>` to `ShaderModification`.
  - Update `compose_wesl` to pack enabled modifiers' `params` into `DynamicParamSchema`, emit `param_<name>()` WESL functions above declarations, and return `(schema, initial_buffer)` alongside the composed WESL and `shader_defs`.
  - Update `DynamicShader` / `DynamicShaderRegistry` to hold `(DynamicParamSchema, DynamicParamBuffer, params_dirty: bool)`:
    - Mutating a param updates `params_dirty` and writes directly to `WaterMaterialExt.dynamic_params` in an `Update` system — without flagging the shader for WESL recompilation.

---

### 3. Runtime Shader Discovery

Discover and hot-reload `.wesl` files placed in `assets/shaders/` and `mods/shaders/` at runtime (mirroring `MenuConfig` in `bevy_script_menus`):

- **New: `crates/bevy_shader_kit/src/catalog.rs`**
  - `ShaderAssetRoot { folder: String, prefix: String }` and `ShaderKitConfig { roots: Vec<ShaderAssetRoot> }` (defaulting to `mods` -> `"mods://"` and `assets` -> `""`).
  - `ShaderKind { Library, EntryPoint }` and `DiscoveredShader { name, asset_path, handle: Handle<Shader>, kind, hooks: Vec<String> }`.
  - `ShaderCatalog` resource:
    - Scans `<root>/shaders/**/*.wesl` at `Startup` and on `ShaderCatalog::rescan(&AssetServer)`.
    - Distinguishes `EntryPoint` (`@fragment` / `@vertex` / `@compute`) vs `Library`, and extracts `fn hook_<stage_name>(` names for drop-in hook libraries.
    - Retains every `Handle<Shader>` so Bevy's `AssetServer` + `file_watcher` hot-reload edits live.
    - Provides helpers to apply a discovered `EntryPoint` to `WaterMaterialExt.override_shader`, or attach a discovered `Library` as a `ShaderModification` (importing via `package::...` and invoking its `hook_<stage>` function).
- **Edit: `crates/bevy_shader_kit/src/lib.rs`**
  - Init `ShaderKitConfig` and `ShaderCatalog`, add startup discovery system, and re-export `ShaderKitConfig`, `ShaderAssetRoot`, `ShaderCatalog`, `DiscoveredShader`, `ShaderKind`.

---

### 4. Root Crate Wiring & Console Commands

Keep command integration in the root crate so `crates/bevy_shader_kit` stays free of workspace dependencies:

- **Edit: `src/main.rs`** (or wherever `ShaderKitPlugin` is configured):
  - Optionally configure `ShaderKitConfig` roots (`mods` -> `"mods://"`, `assets` -> `""`, matching `MenuConfig`).
- **Edit: `src/terrain/mod.rs`** (or a dedicated wiring module in `src/`):
  - Register console commands using `command!`:
    - `shader>list` — lists runtime-discovered shaders (`name`, `kind`, `hooks`).
    - `shader>rescan` — rescans `assets/shaders/` and `mods/shaders/` for newly added `.wesl` files.
    - `shader>use <name|default>` — switches `WaterMaterialExt.override_shader` to a discovered `EntryPoint` shader (or resets to default).
    - `shader>param <name> <f32...>` — updates a runtime `DynamicParamSchema` value on the active `DynamicShader` / `WaterMaterialExt` with no shader recompile.

---

### 5. Verification

- **New unit tests in `crates/bevy_shader_kit`**:
  - `params.rs`: scalar/vec packing order, `Vec2` alignment without crossing `Vec4` boundaries, overflow error past 16 slots, `write_to_buffer`, and `param_<name>()` WESL emission.
  - `compose.rs`: modifiers with `params` emit valid accessors and initial `DynamicParamBuffer` values; disabled modifiers emit nothing.
  - `catalog.rs`: temp-dir discovery of nested `.wesl` files, `Library` vs `EntryPoint` detection, `fn hook_*` extraction, and root prefix formatting (`""` vs `"mods://"`).
- **Checks**:
  - `cargo test --test crate_isolation`
  - `cargo test --workspace`
  - `cargo run`: drop a test `.wesl` into `assets/shaders/`, run `shader>rescan` / `shader>use`, edit a `.wesl` on disk to confirm `file_watcher` hot-reload, and tweak a runtime modifier scalar via `shader>param` to verify live uniform updates without shader recompilation.