Here is a complete plan for `water-coast-shore-waves` to replace the hard boolean drain with a continuous drain and bring shoreline waves, swash, surf foam, and receding wet sand to life.

---

### Why Shores Currently Look Static & "Boolean"

Four parts of the current coastal pipeline fight against lifelike shorelines:

1. **Hard boolean spill cutoff (`water.wesl` & `coast.wesl`):**
   - `spill_height_at` takes `min` across the 4 corners of a `~5.86 m` `HeightGrid` cell, and `water.wesl` runs `if (water.water_height + max_crest_height() < spill_height_at(ctx.p.xz)) { discard; }`.
   - When tide drops or a coastal sill drains, water pops out of existence in binary cell-sized chunks rather than thinning out and draining smoothly. `SeaState::flooded_surface_height_at` (`src/terrain/water.rs`) mirrors that same `Option<f32>` boolean cutoff on the CPU.
2. **Waves freeze at `rest_depth == 0` (`swell_types.wesl` & `coast.wesl`):**
   - `shore_vert_damp(rest_depth)` is `smoothstep(0.0, 4.0, rest_depth)`, which zeroes out 100% of vertical wave motion at the waterline.
   - Because `swell` becomes `0.0` at `rest_depth <= 0.0`, the shoreline never advances up the beach or drains back down, and `coastal_wash_mask` in `water.wesl` freezes at `0.5` along the edge.
3. **Hard specular seam at zero depth (`water.wesl` & `src/terrain/water.rs`):**
   - `Sea` uses `AlphaMode::Opaque` with `specular_transmission: 1.0` and `reflectance: 0.35`. Even as `ctx.depth` reaches `0.0`, Bevy's PBR pass still draws full Fresnel sky reflections right until `coastal_surface_y` drops below `terrain_y` (`dry_side_tuck`), creating a crisp razor edge instead of a tapering wet film.
4. **Static global wetness (`wet_sand.wesl`):**
   - `Wetness` only checks `world_position.y` against one global scalar `wetness.height` (`sea_level + 0.15`), so individual waves rushing up and draining down the beach don't leave glistening dark wet sand behind.

---

### Phase 1 — Smooth Water Drain Instead of Boolean Discard

**Goal:** Eliminate binary popping on draining basins, shallow flats, and shorelines on both GPU and CPU.

1. **Bilinear spill field + continuous `drain_factor` (`coast.wesl` & `water.wesl`):**
   - Replace `min(min(west, east), min(north, south))` in `spill_height_at` (or add a companion `spill_smooth_at`) with bilinear interpolation using `coast_weight_of(world_xz)` so spill elevation varies continuously across each cell instead of snapping on cell boundaries.
   - Replace the hard threshold at `water_height + max_crest_height()` with a continuous drain margin `COAST_DRAIN_BAND` (e.g., `0.15–0.25 m`):
     - Compute `flood_head = (water.water_height + max_crest_height()) - spill_smooth_at(ctx.p.xz)`.
     - Keep `discard` only when `flood_head < -COAST_DRAIN_BAND` (or `ctx.depth <= -0.02`) as an early-out optimization for completely dry inland basins.
     - Compute `drain_mask = smoothstep(-COAST_DRAIN_BAND, 0.0, flood_head)` so draining lagoons/puddles smoothly lose thickness rather than popping off at one threshold.
2. **Smooth vertex lowering during drain (`coast.wesl`):**
   - In `coastal_surface_y`, smoothly blend the surface down toward `terrain_y - COAST_DRY_TUCK` as `flood_head` approaches `0` and as `ctx.depth` drains below `0.04 m`, eliminating sudden drops across triangles.
3. **Thin-film optical fade at zero depth (`water.wesl`):**
   - Compute `film = smoothstep(0.0, COAST_THIN_FILM_DEPTH, ctx.depth) * drain_mask` (e.g., `COAST_THIN_FILM_DEPTH = 0.08 m`).
   - Multiply `pbr_input.material.reflectance`, `ctx.foam`, and `in_scatter` by `film` (and blend `perceptual_roughness` slightly higher in the last couple centimeters of film) so the water sheet dissolves seamlessly into glossy wet terrain as it drains to `0`.
4. **CPU parity (`src/terrain/grid.rs` & `src/terrain/water.rs`):**
   - Update `HeightGrid::sample_spill` and `SeaState::flooded_surface_height_at` to share the same bilinear spill convention and drain margin, returning a drained height or `None` only outside `COAST_DRAIN_BAND` so `SubmersionState` (`src/integration/submersion.rs`) and `WaterWakeTracker` (`src/integration/ripples.rs`) taper smoothly to dry land.

---

### Phase 2 — Living Shore Swash: Runup & Asymmetric Backwash

**Goal:** Make waves physically wash *up* gently sloped beaches and *drain* back down, while keeping steep cliffs clean.

1. **Slope-gated swash runup (`coast.wesl` & `swell_types.wesl`):**
   - Currently `shore_vert_damp(rest_depth)` hits `0.0` at `rest_depth == 0.0`. Extend the lower bound into negative `rest_depth` (dry side) up to `-swash_reach` (`swash_reach = SWASH_BASE + SWASH_ENERGY_GAIN * wave_energy`, e.g. `0.2–0.7 m` vertical reach).
   - Gate `swash_reach` by beach slope using `length(terrain_gradient_at(world_xz))`:
     - Gentle slopes (`slope ~ 0.05..0.35`) get full runup.
     - Very flat plains (`slope < 0.02`, like the `Crater` center) and steep cliffs (`slope > 0.7`) smoothly damp runup (`smoothstep(0.01, 0.06, slope) * (1.0 - smoothstep(0.45, 0.9, slope))`) so waves don't flood flat plains or clip sideways out of cliffs.
2. **Asymmetric bore vs. backwash wave shape (`swell_types.wesl`):**
   - Real shore swash rushes up quickly and drains back slowly. Shape the nearshore swash wave using a skewed phase (e.g. combining the fundamental sine with a second harmonic `sin(phase) + 0.25 * sin(2 * phase)` or a asymmetric rise/drain curve) and bias the crest shoreward along `-shore_dir`.
   - Remove the rigid `trough_floor = -(rest_depth - 0.03)` lock inside the active swash zone so troughs pull back cleanly under `terrain_y` (`dry_side_tuck`), while preserving the `0.03 m` clearance only on flat zero-slope shallow ground (`slope < 0.02`) to prevent the Z-fighting seen on flat plains.
3. **Sync CPU shore surface (`crates/bevy_shader_kit/src/water.rs` & `src/terrain/water.rs`):**
   - Mirror the slope-gated swash term in `SwellSet::height_at_shored` / `SeaState::shore_surface_height_at` so a character standing at the water's edge actually gets ankles washed by incoming waves and drained during backwash.

---

### Phase 3 — Surf Zone Dynamics, Lacy Backwash Foam & Turbidity

**Goal:** Give breaking waves, receding backwash streaks, and shallow surf distinct visual energy.

1. **Un-frozen swash phase for foam (`coast.wesl` & `water.wesl`):**
   - Fix `wash_line` in `water.wesl` by driving `coastal_wash_mask` with the **pre-damped** swash wave phase/crest velocity rather than post-damped `swell_height_shored` (which currently stalls at `0.5`).
   - Distinguish **incoming bore** (bright leading foam lip where `d(swell)/dt > 0`) from **draining backwash** (thin, retreating lacy streaks).
2. **Multi-band coastal foam (`foam.wesl` & `water.wesl`):**
   - **Breaker band:** Peak foam at breaking depth (`rest_depth ~ 0.5..2.2 m`, scaling deeper when `wave_energy` is high).
   - **Swash leading edge:** Thin crisp rim (`0.0..0.18 m` live `ctx.depth`) riding the advancing/retreating water edge.
   - **Directional backwash pattern:** Stretch `scrolling_fbm_2d` UV coordinates along `shore_dir` in the swash zone (advected slowly seaward during drain) so foam forms natural streaky lacy web patterns instead of isotropic blobs.
3. **Surf-zone suspended sediment / turbidity (`water.wesl`):**
   - In `WATER_OPTICS`, subtly shift `scattering` and `absorption` toward warmer sandy-teal in turbulent shallow water (`0..2.5 m` scaled by `wave_energy`), contrasting shallow surf against deeper offshore ocean color.

---

### Phase 4 — Wave-Synced Wet Sand & Ebb-Tide Drying

**Goal:** Make sand glisten behind every retreating wave and gradually dry out.

1. **Dynamic swash wetness (`crates/bevy_shader_kit/src/wet.rs`, `wet_sand.wesl`, `src/terrain/water.rs`):**
   - Extend `Wetness` uniform (staying 16-byte aligned) with `swash_height: f32` (instantaneous upper runup envelope, e.g. `sea_level + swash_reach`), `time: f32`, and `wave_energy: f32` (or keep `wetness.height` as the slow-drying ebb-tide ceiling `wetness_height` and add a subtle wave-front wetness pulse in `wet_sand.wesl`).
   - Inside `wet_sand.wesl`, combine:
     - **Tide lag band:** slow-drying dark sand up to `wetness.height` (`WETNESS_DECAY` already tracks ebb tide in `drive_sea_level`).
     - **Active swash band:** extra mirror-gloss sheen (`wet_roughness ~ 0.08–0.12`) modulated along the beach contour with scrolling coastal noise (`scrolling_fbm_2d`) synced to `wave_energy`, so sand freshly uncovered by a draining wave visibly glistens and fades back to matte sand (`0.92`) before the next set arrives.

---

### Phase 5 — Controls, Isolation & Verification

1. **Toggles & Parameters:**
   - Keep `set_wave_energy`, `toggle_water_coast`, `toggle_water_swells`, `toggle_water_tides`, `toggle_water_foam`, and `toggle_water_wetness` (`src/terrain/mod.rs`) controlling the new behavior without extra boilerplate.
   - Ensure `crates/bevy_shader_kit` stays free of root-crate imports (`tests/crate_isolation.rs`) and `src/terrain/` stays free of peer feature imports (`tests/module_isolation.rs`), with zero code comments (`CLAUDE.md`).
2. **Automated Tests:**
   - `crates/bevy_shader_kit/src/water.rs`: verify `Wetness` / `WaterParams` layout sizes match WESL; verify slope-gated swash runs up gentle beaches, stays flat on cliffs/flat plains, and keeps `height_at_shored` continuous across `rest_depth == 0`.
   - `src/terrain/grid.rs` & `src/terrain/water.rs`: verify bilinear `sample_spill` matches `coast.wesl`, enclosed basins drain smoothly across `COAST_DRAIN_BAND` without snapping, and `SubmersionState` transitions cleanly from wading -> draining -> dry.
3. **In-Game Visual Verification:**
   - Test on both `Crater` and `set_terrain_generator noise` across `set_wave_energy 0.1` (calm lapping), `0.5` (normal surf), and `1.0` (storm swash), plus `set_water_level` tide drops, using `angle_camera` + `screenshot` to inspect close-up shore swash, draining lagoons, and wet sand sheen.