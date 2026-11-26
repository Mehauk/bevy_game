Here is the full plan to eliminate both causes of the shoreline and terrain-intersection gaps while keeping `module_isolation`, `crate_isolation`, and existing CPU/GPU parity intact.

---

### Part 1 — Fix Coastal `spill` Discard Mismatch (`crates/bevy_shader_kit`)

**Why it breaks today:**
In `crates/bevy_shader_kit/src/shaders/water/water.wesl`, fragments are discarded when:
```wgsl
if (water.water_height + max_crest_height() < spill_height_at(ctx.p.xz)) {
    discard;
}
```
`terrain_height_at` (and CPU `HeightGrid::sample`) bilinearly interpolates the 4 corners around `coast_weight_of(world_xz)` (`clamp(uv * res - 0.5, ...)`), whereas `spill_height_at` in `coast.wesl` snaps via nearest-texel `coast_texel_of` (`floor(uv * res)`). Along any sloping beach (`~5.86 m` texels at `world = 3000.0`, `grid_resolution = 512`), underwater pixels up to half a texel (`~2.93 m`) seaward of a dry texel center read that dry texel's `spill` and get `discard`ed.

**Changes (`crates/bevy_shader_kit/src/shaders/water/coast.wesl`):**
1. Update `spill_height_at(world_xz: vec2<f32>) -> f32` to load the 4 corner texels of the exact `coast_weight_of(world_xz)` bilinear footprint (`corner`, `corner + (1, 0)`, `corner + (0, 1)`, `corner + (1, 1)`) and return `min(min(west, east), min(north, south))`.
   - Along any coastal slope connected to the sea, at least one corner in the cell is seaward (`spill <= water_height`), so underwater pixels are never discarded.
   - Inside an enclosed dry inland basin, all 4 surrounding texels sit at or above the basin's spill ridge (`spill > water_height + max_crest_height()`), so dry inland basins still discard cleanly.
2. Remove the now-unused `coast_texel_of` helper.
3. No changes needed to `water.wesl` or `compose.rs` (`water_blueprint_reproduces_the_shipped_shader` stays untouched).

---

### Part 2 — Distance-Attenuated Swells in the Water Vertex Shader (`crates/bevy_shader_kit`)

**Why it's needed:**
- Right now `SWELLS` includes a `6.5 m` wavelength component (`src/terrain/water.rs`), which aliases on any grid spacing `>= 3.25 m` and causes coarse outer triangles to sag below distant beaches during wave troughs.
- When we replace the flat `Plane3d` with a camera-centered clipmap (Part 3), fading wave displacement to `0` before the coarse rings guarantees:
  1. Zero wave aliasing on coarse rings.
  2. Zero trough-chord dips under distant shorelines (`coastal_surface_y` returns exact `water.water_height` when `swell == 0` because `trough_floor <= 0`).
  3. Zero vertex swimming on outer rings even when the clipmap translates in steps finer than the outer ring spacing.

**Changes:**
1. **`crates/bevy_shader_kit/src/shaders/water/water_vertex.wesl`:**
   - Import `bevy_pbr::render::mesh_view_bindings::view`.
   - Compute `swell_atten = 1.0 - smoothstep(32.0, 60.0, length(swell_rest - view.world_position.xz))` (full waves within `32 m` of the camera, smoothly reaching `0` before the `4.0 m` ring at `64 m`).
   - Pass `swell_atten` into coastal/offshore vertex height & normal calculation:
     - `WATER_COAST`: scale `swell` inside `coastal_surface_y(swell_rest, water.time, swell_atten)` and `coastal_normal(swell_rest, water.time, swell_atten)` (or factor into `shore_context_at`).
     - `!WATER_COAST`: scale `swell_offset(...) * swell_atten` and blend `swell_normal(...)` toward `vec3(0.0, 1.0, 0.0)` by `swell_atten`.
2. **`crates/bevy_shader_kit/src/shaders/water/coast.wesl`:**
   - Add `swell_atten: f32` parameter to `coastal_surface_y` and `coastal_normal` (attenuating `swell` before `max(swell, trough_floor)`).
   - Note: `water.wesl` only calls `coastal_wash_mask` and `shore_context_at`, not `coastal_surface_y` or `coastal_normal`, so fragment code is unaffected.

---

### Part 3 — Camera-Centered Water Clipmap Mesh (`src/terrain/water.rs`)

**Why it replaces `Plane3d`:**
- `Plane3d` with `SEA_SUBDIVISIONS = 256` over `3000 x 3000` produces `~11.7 m` quads (`131,072` triangles), which span across multiple `5.86 m` bilinear `HeightGrid` saddle cells and `0.94 m` LOD0 terrain triangles, dipping underneath convex shoreline slopes.
- A single static concentric clipmap mesh centered on `TerrainViewer` gives `0.5 m` quads near the player/shoreline (finer than LOD0 terrain) in **1 draw call** with **~57k triangles** (less than half the current triangle count).

**Geometry & Topology (`src/terrain/water.rs`):**
- Parameters:
  - `CLIPMAP_BASE_STEP: f32 = 0.5` (`0.5 m` inner quads)
  - `CLIPMAP_RING_CELLS: usize = 64` (even; each ring spans `64 x 64` cells at its own step, with inner `32 x 32` hole covered by the finer ring inside it)
  - `CLIPMAP_RINGS: usize = 9` (steps `0.5, 1, 2, 4, 8, 16, 32, 64, 128 m`; outer half-extent = `4096 m`, covering the full `3000 x 3000` world from any corner)
  - `CLIPMAP_SNAP: f32 = 2.0` (integer multiple of rings `0..=2` where `step <= 2.0 m` and `swell_atten > 0`, so every wave-displaced ring stays world-grid-locked with zero swimming while outer rings are flat)
- **Watertight 2:1 ring stitching in `build_water_clipmap_mesh()`:**
  - **Ring 0:** Solid `64 x 64` grid (`step = 0.5 m`, `half = 16 m`), stitched on all 4 outer borders (`N | S | E | W`) so its `64` outer edges share exact vertices with Ring 1's `32`-cell inner hole (`step = 1.0 m`).
  - **Rings `1..CLIPMAP_RINGS - 1`:** Hollow ring covering `[0..64] x [0..64]` minus `[16..48] x [16..48]`. Inner border vertices `[16..48]` remain unskipped (matching the previous ring's even outer vertices 1:1), while the outer border (`0` and `64`) is 2:1 stitched on all 4 borders (`N | S | E | W`) except the outermost ring (`CLIPMAP_RINGS - 1`).
  - Keep vertex normals `Vec3::Y` and `Uv` optional (matching `Plane3d` attributes `POSITION`, `NORMAL`, `UV_0` for `StandardMaterial` compatibility).
- **Viewer tracking (`src/terrain/water.rs` & `src/terrain/mod.rs`):**
  - Update `drive_sea_level` (or a dedicated `sync_water_clipmap` system chained with it) to read `viewer: Res<TerrainViewer>` (already updated from `MainCamera` in `src/integration/terrain_viewer.rs`, respecting `tests/module_isolation.rs`):
    ```rust
    let snapped = (Vec2::new(viewer.position.x, viewer.position.z) / CLIPMAP_SNAP).round() * CLIPMAP_SNAP;
    transform.translation = Vec3::new(snapped.x, sea_state.sea_level, snapped.y);
    ```
  - Note on world borders: outside `[-world/2, +world/2]`, `coast.wesl` already clamps `coast_map` to the submerged border texel (`-30.0` for `Crater`, `-4.0` for `Noise`), so the outer clipmap rings extend seamlessly as open ocean to the horizon/fog rather than showing a hard square cutoff.

---

### Part 4 — Verification Plan

1. **Unit tests in `src/terrain/water.rs`:**
   - `build_water_clipmap_mesh` covers `(2 * outer_half)^2` exact area with positive triangle winding (zero flipped, degenerate, or overlapping triangles, and zero gaps across ring seams).
   - Outer ring boundaries never reference odd vertices on stitched borders, and inner ring holes match the adjacent finer ring's outer border vertices 1:1.
   - `snap_clipmap_center` snaps to exact multiples of `CLIPMAP_SNAP` within `±CLIPMAP_SNAP * 0.5` of the viewer.
2. **Suite & isolation checks:**
   - `cargo test --workspace`
   - `cargo test --test module_isolation`
   - `cargo test --test crate_isolation`
   - Zero code comments in all edited files (`CLAUDE.md`).
3. **Visual check (`cargo run`):**
   - Inspect crater rim shorelines, low/high tide transitions, `set_terrain_generator noise`, and aerial `angle_camera` + `screenshot` views to confirm no shoreline cutouts, no chord gaps, and no vertex swimming while moving.

---

Ready to implement whenever you'd like me to proceed.