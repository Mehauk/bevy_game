### Root Causes Visible in the Screenshot

There are two separate bugs interacting in the image (plus a minor shoreline Z-fighting edge case):

1. **Wave troughs carving dry islands out of shallow water (Primary Bug)**
   - In `src/terrain/water.rs` (`SeaState::shore_surface_height_at`) and `crates/bevy_shader_kit/src/shaders/water/coast.wesl` (`coastal_surface_y`), shallow water is supposed to damp swells toward `0` and clamp wave troughs at `terrain_y + 0.03` (`TROUGH_CLEARANCE` / `COAST_TROUGH_CLEARANCE`). That is why the HUD reports `Water Depth: 0.03 m` (`Movement State: wading`) at `(0, 0.9, 0)` while `sea_level` is shallowly flooding the flat `Crater` plain (`y = 0.0`).
   - However, in `crates/bevy_shader_kit/src/water.rs`, `WaterMaterialExt::specialize` only appends `water_shader_defs(&key.bind_group_data)` to **`descriptor.fragment`**, never to **`descriptor.vertex`**:
     - Consequently, `WATER_COAST` is **undefined** in `crates/bevy_shader_kit/src/shaders/water/water_vertex.wesl`.
     - The vertex shader always runs the `!WATER_COAST` branch (`swell_offset` with full `0.225 m` combined amplitude and no coastal damping or `COAST_TROUGH_CLEARANCE` floor).
     - Inside the `60 m` `swell_atten` radius, un-clamped wave troughs plunge below `y = 0.0`, getting depth-culled by the flat terrain and exposing wave-shaped dry patches with pale zero-depth intersection rims—while beyond `60 m` (`swell_atten == 0`), the water sits above `y = 0.0` out to the horizon.

2. **Horizontal striped banding inside the distant exposed terrain patches (Secondary Bug)**
   - In `src/terrain/atmosphere.rs`, `setup_atmosphere` spawns the `DirectionalLight` pointing along `(-0.15, -0.05, 0.25)` (~`9.7°` above the horizon) with `CascadeShadowConfigBuilder { first_cascade_far_bound: 25.0, maximum_distance: 200.0, ..default() }` and default shadow biases (`shadow_depth_bias = 0.02`, `shadow_normal_bias = 1.8`).
   - At a `9.7°` grazing angle, each shadow texel spans `~6×` further horizontally across flat `y = 0.0` ground. Cascade 0 (`< 25 m`) clears cleanly (which is why the foreground patch under the capsule has no stripes), but starting in Cascade 1 (`> 25 m`), texel depth error exceeds the default `0.02` bias and produces the parallel lit/shadowed acne stripes visible across every exposed patch in the upper half of the image.

3. **Coplanar Z-fighting / wide-area foam on flat shallow ground (Edge Case)**
   - Even with `WATER_COAST` active in the vertex shader, whenever `sea_level` crosses `0.0` on the flat `Crater` plain (or right at `rest_depth == 0.0` on gentle slopes), `coastal_surface_y` equals `terrain_y` (`0.0 mm` separation), which can Z-fight during the transition.
   - Additionally, because `Crater` is flat at `y = 0.0` across `2800 × 2800 m` and `WaterParams::default().foam_width` is `0.8`, `shore_edge_mask(ctx.depth, 0.8)` covers the entire `60 m` view radius in `scrolling_fbm_2d` foam whenever the plain is shallowly flooded (`0 < sea_level < 0.8`).

---

### Full Fix Plan

#### Step 1 — Forward `WaterMaterialKey` shader defs to the vertex shader (`crates/bevy_shader_kit/src/water.rs`)
1. Extract a small helper (e.g., `apply_water_specialization`) or update `WaterMaterialExt::specialize` directly so `water_shader_defs(&key.bind_group_data)` is extended onto **both** `descriptor.vertex.shader_defs` and `descriptor.fragment.shader_defs`.
2. Keep `override_shader` fragment override behavior unchanged.
3. Verify `water_vertex.wesl` bindings (`water` at `@binding(100)`, `swells` at `@binding(102)`, `coast` at `@binding(105)`, `coast_map` at `@binding(106)`) are all vertex-accessible (they already use `AsBindGroup` default visibility and `textureLoad` rather than `textureSample`).
4. **Unit test:** Add a test in `crates/bevy_shader_kit/src/water.rs` verifying that `WATER_COAST` (and the other `water_shader_defs`) are applied to both vertex and fragment shader defs.

#### Step 2 — Eliminate shoreline Z-fighting at `rest_depth ≈ 0` (`crates/bevy_shader_kit/src/shaders/water/coast.wesl`)
1. In `coastal_surface_y` (`coast.wesl`), smoothly tuck vertices slightly beneath the terrain as `shore.rest_depth` approaches and drops below `0.0` (for example, adding a small negative offset `-0.04 * (1.0 - smoothstep(-0.08, 0.01, shore.rest_depth))` so dry-side vertices sit cleanly under `y = terrain_y` instead of coplanar at `0.0 mm` when `sea_level == terrain_y`).
2. Confirm `coastal_normal` remains smooth across `rest_depth == 0.0` and `SeaState::shore_surface_height_at` (`src/terrain/water.rs`) remains synced for `rest_depth > 0.03` (CPU submersion still reads `flooded_surface_height_at` unchanged).

#### Step 3 — Eliminate Cascade 1+ grazing-angle shadow acne (`src/terrain/atmosphere.rs`)
1. On the `DirectionalLight` in `setup_atmosphere`, increase `shadow_depth_bias` (e.g., `0.10`–`0.15`) and `shadow_normal_bias` (e.g., `2.5`–`3.0`) so flat/gently sloped terrain beyond `25 m` does not self-shadow at the `9.7°` sun elevation, while checking in-game that the foreground capsule shadow stays anchored to its feet (no peter-panning).
2. Optionally tune `CascadeShadowConfigBuilder` (e.g., slightly larger `first_cascade_far_bound` or `overlap_proportion` / tighter `maximum_distance`) if Cascade 2/3 texels near the `200 m` edge still show faint banding during low tide or with `set_terrain_generator noise`.

#### Step 4 — (Optional Polish) Keep shallow flat-plain foam concentrated near actual shorelines/waves (`crates/bevy_shader_kit/src/shaders/water/water.wesl`)
- If you do not want the entire `2800 × 2800 m` `Crater` plain covered in mottled white foam whenever `0 < sea_level < 0.8`:
  - In `water.wesl` under `@if(WATER_COAST)`, weight `shore_edge_mask` on coastal surfaces by shoreline proximity / slope or wave energy (or reduce `WaterParams::default().foam_width` from `0.8` to `~0.25–0.35`) so foam hugs actual depth transitions and wave crests rather than uniform ankle-deep flats.

---

### Verification Plan

1. **Automated checks:**
   - `cargo test --workspace`
   - `cargo test --test crate_isolation`
   - `cargo test --test module_isolation`
2. **In-game visual checks (`cargo run`):**
   - **Shallow flood (`Water Depth: 0.03 m`):** Confirm water smoothly covers the plain around the character with zero wave-trough holes, zero pale intersection rims, and matching CPU `Water Depth` vs rendered water level.
   - **Low tide (`set_water_level -0.2` or waiting for tide ebb) & `set_terrain_generator noise`:** Confirm exposed dry ground beyond `25 m` has zero horizontal shadow-acne stripes, the capsule shadow stays grounded, and the shoreline intersects cleanly with no Z-fighting as `sea_level` crosses `0.0`.
   - **Toggle parity (`toggle_water_coast`):** Confirm toggling coastal mode off and back on visibly switches both vertex wave behavior and fragment coastal shading.