- [ ] extract the shaders wesl files and logic to a new crate.
- [ ] extract the water implementation into multiple smaller utility wesl files.
- [ ] allow for composition with modifications for dynamic shader.

---

### 1. Crate & Directory Structure (`crates/bevy_shader_kit`)

```text
crates/bevy_shader_kit/
  Cargo.toml
  src/
    lib.rs                  # ShaderKitPlugin: shader registration, MaterialPlugins, systems, public exports
    compose.rs              # ShaderHook, HookMode, ShaderModification, ShaderBlueprint, compose_wesl, DynamicShader
    water.rs                # WaterParams, WaterMaterialExt, WaterMaterial, WaterSurface, WaterHeightSource, sync_water_fields
    sdf/
      mod.rs                # SdfVolumeUniform, DistanceFieldSource, PrimaryField, shared SDF types
      baker.rs              # FieldBounds, sdf_from_triangles, opaque extraction, bake_distance_fields
      view.rs               # DistanceFieldView, DistanceFieldViewExt, sync_distance_field_view, animate_distance_field
    shaders/
      math/
        aa.wesl             # phase_derivative_aa(phase)
        falloff.wesl        # linear_falloff, quadratic_falloff, view_distance_fade
      noise/
        hash.wesl           # hash21(p)
        value2d.wesl        # value_noise_2d(p)
        fbm2d.wesl          # scrolling_fbm_2d(p, time)
      optics/
        refraction.wesl     # snell_refract(v, n, eta)
        fresnel.wesl        # fresnel_schlick(f0, ndv)
        scattering.wesl     # schlick_phase, beer_lambert_in_scatter
      shading/
        hemisphere.wesl     # hemisphere_factor, hemisphere_mix
      sdf/
        texture3d.wesl      # df_texel, df_sample, df_gradient
        volume.wesl         # SdfVolume, volume_uvw, volume_inside, volume_box_distance, volume_sample*, volume_gradient, line_box_intersect
        field_array.wesl    # FieldContact, field_array_min_distance, field_array_contact, field_array_raymarch_depth
        contact.wesl        # ContactSurface { point, outward, emerged }, project_onto_contact
        view_bindings.wesl  # DistanceFieldView uniform + bindings
        view_mode.wesl      # entry point: raymarched clay/glow debug view
      water/
        types.wesl          # WaterParams, WaterContext, @binding(100..104)
        ripples.wesl        # radial_contact_distance, ripple_wave_signal, perturb_normal_horizontal
        foam.wesl           # shore_edge_mask, proximity_edge_mask, foam_noise_mask, apply_foam_roughness, blend_foam_color
        optics.wesl         # water_volume_in_scatter
        water.wesl          # entry point: default composed water fragment shader
```

All files in `assets/shaders/*.wesl` move into `crates/bevy_shader_kit/src/shaders/` and `assets/shaders/` is deleted.

---

### 2. Cargo & Plugin Registration

- **`crates/bevy_shader_kit/Cargo.toml`**:
  - `name = "bevy_shader_kit"`, `edition = "2024"`.
  - Dependencies: `bevy` (`default-features = false` with `bevy_asset`, `bevy_image`, `bevy_light`, `bevy_mesh`, `bevy_pbr`, `bevy_render`, `bevy_shader`, `bevy_transform`) and `mesh_to_sdf = "0.4"`.
  - Zero dependencies on `world_generation` or `bevy_script_menus` (`tests/crate_isolation.rs` passes).
- **`Cargo.toml` (root)**:
  - Add `"crates/bevy_shader_kit"` to `[workspace] members` and `bevy_shader_kit = { path = "crates/bevy_shader_kit" }` to `[dependencies]`.
  - Remove `mesh_to_sdf = "0.4"` from root `[dependencies]`.
- **`ShaderKitPlugin::build` (`src/lib.rs`)**:
  1. Register library modules via `load_shader_library!(app, "shaders/...")` in dependency order (before entry points and `MaterialPlugin`s), exposing them as `bevy_shader_kit::shaders::<domain>::<module>`:
     - `shaders/math/{aa,falloff}.wesl`
     - `shaders/noise/{hash,value2d,fbm2d}.wesl`
     - `shaders/optics/{refraction,fresnel,scattering}.wesl`
     - `shaders/shading/hemisphere.wesl`
     - `shaders/sdf/{texture3d,volume,field_array,contact,view_bindings}.wesl`
     - `shaders/water/{types,ripples,foam,optics}.wesl`
  2. Register entry points via `embedded_asset!`:
     - `shaders/sdf/view_mode.wesl` -> `"embedded://bevy_shader_kit/shaders/sdf/view_mode.wesl"`
     - `shaders/water/water.wesl` -> `"embedded://bevy_shader_kit/shaders/water/water.wesl"`
  3. Add `MaterialPlugin::<DistanceFieldViewMaterial>::default()` and `MaterialPlugin::<WaterMaterial>::default()`.
  4. Initialize `DistanceFieldView`, `DynamicShaderRegistry`, `DistanceFieldUnitCube` and add `Update` systems (`bake_distance_fields`, `animate_distance_field`, `sync_distance_field_view`, `sync_water_fields`, `rebuild_dynamic_shaders`).

---

### 3. SDF & Water Extraction (`sdf/` & `water.rs`)

- **SDF (`src/sdf/`)**:
  - Extract `FieldBounds`, `sdf_from_triangles`, opaque mesh extraction, `bake_distance_fields`, `animate_distance_field`, `sync_distance_field_view`, `DistanceFieldSource`, `PrimaryField`, `DistanceFieldVolume`, `DistanceFieldView`, `DistanceFieldViewHidden`, `DimmedFog`, `InvisibleMaterial`, `DistanceFieldParams`, `DistanceFieldViewExt`, `DistanceFieldViewMaterial`.
  - Cache the unit `Cuboid` mesh in `DistanceFieldUnitCube`.
  - Replace `With<SkyBox>` with `DistanceFieldDimmedColor(pub Color)` so `bevy_shader_kit` has no dependency on `terrain::atmosphere::SkyBox`.
  - Provide `DistanceFieldView::toggle(&mut self) -> bool`; keep the command and `debug_info!` bindings in `src/terrain/mod.rs`.
- **Water (`src/water.rs`)**:
  - Extract `SdfVolumeUniform` / `WaterFieldParams`, `WaterParams`, `WaterMaterialExt`, `WaterMaterial`, `WaterSurface`, and `sync_water_fields`.
  - Replace `With<Sea>` in `sync_water_fields` with a crate-owned `WaterHeightSource` marker component; keep `Sea`, `setup_water_surface` (using `Terrain::size()`), and `set_water_level` in `src/terrain/water.rs`.

---

### 4. Dynamic Shader Composition with Modifications (`src/compose.rs`)

Supports both compile-time pipeline specialization and runtime WESL modification composition targeting individual sub-parts:

- **Pipeline Specialization (`WaterMaterialKey` + `MaterialExtension::specialize`)**:
  - Flags: `ripples`, `foam`, `optics` -> `ShaderDefVal::Bool("WATER_RIPPLES" / "WATER_FOAM" / "WATER_OPTICS", ...)`.
  - Constants: `depth_steps: u32` -> `ShaderDefVal::UInt("WATER_DEPTH_STEPS", ...)`.
  - Shader override: `override_shader: Option<Handle<Shader>>` stored strongly on `WaterMaterialExt`, hashed in `WaterMaterialKey`, and assigned to `descriptor.fragment.shader` in `specialize`.
- **Sub-Stage Hook Composition (`ShaderBlueprint`, `ShaderModification`, `compose_wesl`, `DynamicShader`)**:
  - **`ShaderHook`**: `ContactSample`, `ContactProjection`, `RippleDistance`, `RippleWave`, `RippleNormal`, `FoamEdge`, `FoamPattern`, `FoamRoughness`, `PreLighting`, `PostLighting`, `VolumeOptics`, `FoamShade`, `PostProcess`, plus `CustomStage(&'static str)`.
  - **`HookMode`**: `Append` (ordered by `(priority, id)`) vs `Replace` (highest-priority replacement overrides only that sub-stage).
  - **`ShaderModification`**: `{ id, priority, enabled, imports, shader_defs, declarations, hooks }`.
  - **`water_blueprint()`**: default blueprint built on `bevy_shader_kit::shaders::water::*` and `WaterContext` so modifiers can mix/replace any sub-part (e.g., swapping `FoamPattern` noise or adding `FoamEdge` wake trails without touching `FoamRoughness` or `FoamShade`).
  - **`compose_wesl(blueprint, mods)`**: pure function that deduplicates & hoists all `import` lines above declarations, resolves each hook slot, and merges `ShaderDefVal` overrides.
  - **`DynamicShader` / `DynamicShaderRegistry`**: tracks dirty blueprints/modifications and writes `Shader::from_wesl(output.source, virtual_path)` into `Assets<Shader>` in `rebuild_dynamic_shaders` for live pipeline recompilation.

---

### 5. Root Crate Updates & Verification

- **Root edits**:
  - Register `ShaderKitPlugin` in `src/main.rs`.
  - Delete `assets/shaders/*.wesl` and `src/terrain/distance_field.rs`.
  - Update `src/terrain/mod.rs`, `src/terrain/atmosphere.rs` (`DistanceFieldDimmedColor`), `src/terrain/water.rs` (`WaterHeightSource`), and `src/integration/bridges.rs`.
- **Verification**:
  - Migrate existing SDF tests into `crates/bevy_shader_kit`.
  - Add pure unit tests for `compose_wesl` (top-of-file import hoisting, `(priority, id)` ordering, sub-stage `Replace` isolation, disabled modifiers, `ShaderDefVal` overrides) and `WaterMaterialKey` specialization.
  - Run `cargo test --test crate_isolation`, `cargo test --workspace`, and `cargo run` to confirm visual parity and live `DynamicShader` compilation.

Ready to start implementing whenever you give the go-ahead.