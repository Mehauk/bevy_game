## Stage 1 — Pluggable Generation (`Generator`)

Same foundation: keep `Terrain` stateless and `Copy` while moving the analytic crater behind an enum.

### Migration Steps
1. **Add `Generator` in `src/terrain/terrain.rs`**:
   - Define `#[derive(Clone, Copy, Debug, PartialEq)] pub enum Generator { Crater, Noise { seed: u32, octaves: u32, scale: f32 } }`.
   - Implement `Generator::sample(&self, world: Vec2, x: f32, z: f32) -> f32`:
     - `Crater`: existing `CRATER_DEPTH` / `CRATER_RIM_X` / `CRATER_RIM_Z` / `CRATER_INSET` smoothstep math.
     - `Noise`: deterministic, stateless 2D FBM using pure coordinate-hashed gradient noise (do **not** construct `noise::Perlin::new(seed)` inside `sample_ground_height`, as it rebuilds a 512-byte permutation table on every vertex and normal call).
2. **Thread through `TerrainConfig` & `Terrain`**:
   - Add `pub generator: Generator` to `TerrainConfig` (default `Generator::Crater` initially).
   - Route `Terrain::sample_ground_height(&self, x, z)` to `self.0.generator.sample(self.0.world, x, z)`.
   - Reconcile `TerrainConfig::default().world` (`3000.0`, 2500 chunks) vs. existing tests in `terrain.rs` and `chunks.rs` (which assert `300.0`, 25 chunks) before expanding test coverage.

### Files Touched
- `src/terrain/terrain.rs`
- `src/terrain/mod.rs` (re-export `Generator`)

### Risks
- **Per-call table construction**: Constructing `noise::Perlin` inside `sample_ground_height` will stall chunk building; keep noise evaluation pure arithmetic or prebuilt once outside the per-vertex loop.
- **Boundary ridges vs. ocean spill**: Arbitrary noise can place ridges along world edges above `sea_level`, blocking `heightmap.rs` edge-seeded `spill` flooding from entering interior valleys unless you intend enclosed basins or add an edge falloff.
- **Interior LOD seams**: `Crater` is flat in the interior, so curved `Noise` slopes will exercise interior `StitchFlags` seams for the first time.

### What to Test Before Moving to Stage 2
- `Noise` determinism across `(seed, octaves, scale, x, z)` and safe finite outputs for `octaves == 0` or `scale <= 0.0`.
- `Generator::Crater` bit-identical parity with the original function.
- `a_stitched_fine_edge_meets_the_coarse_neighbour_exactly` passing with `Generator::Noise` on all 4 borders (`N, S, E, W`).
- `bake_height_map` with `Generator::Noise` producing finite values with `spill >= ground`.
- `FixedUpdate` tests in `src/integration/currents.rs` and `src/integration/submersion.rs` (with `TimeUpdateStrategy::ManualDuration`).

---

## Stage 2 — Grid-Authoritative Heightfield & Simulation Passes (Erosion)

Invert `heightmap.rs` so a stored `HeightGrid` becomes the single source of truth, seeded by `Generator` and optionally shaped by iterative terrain passes (erosion, smoothing, etc.) before meshes, colliders, and `spill` read from it.

### Migration Steps
1. **Create `HeightGrid` (`src/terrain/grid.rs` or in `terrain.rs`)**:
   - Store `resolution: UVec2`, `bounds_min: Vec2`, `texel: Vec2`, `inv_size: Vec2`, and `heights: Vec<f32>`.
   - Move `HEIGHT_MAP_RESOLUTION` into `TerrainConfig::grid_resolution: u32` (default `512`).
   - Seed texel centers (`bounds_min + (ix + 0.5, iz + 0.5) * texel`) from `Generator`.
2. **Add toggleable generation-time passes (`TerrainSimConfig` / `TerrainPass`)**:
   - Define a config struct on `TerrainConfig` (e.g., `pub erosion: ErosionConfig { pub enabled: bool, pub thermal_iterations: u32, pub hydraulic_drops: u32, pub talus_angle: f32, ... }` — disabled or tuned by default, deterministic from a `seed`).
   - Run enabled passes directly over `HeightGrid.heights` right after seeding from `Generator`. Because passes run on the flat `Vec<f32>` grid before creating the `Arc<HeightGrid>`, there is zero locking and zero async complexity.
3. **Preserve the `sample_ground_height(x, z)` seam with CPU/GPU parity**:
   - Change `Terrain` from `Copy` to `Clone` wrapping `config: TerrainConfig`, `revision: u64`, and `grid: Arc<HeightGrid>`.
   - Implement `HeightGrid::sample(x, z)` using the exact texel-center bilinear convention of `coast_weight_of` + `terrain_height_at` in `crates/bevy_shader_kit/src/shaders/water/coast.wesl`.
   - Scale `Terrain::ground_normal` finite-difference step with `texel` (`NORMAL_SAMPLE_STEP.max(texel_min * 0.5)`) so normals stay smooth across grid cells.
   - Update `let sampler = *terrain;` to `let sampler = terrain.clone();` in `src/terrain/chunks.rs` (two sites: `spawn_terrain_chunks` and `update_chunk_lods`).
4. **Invert `src/terrain/heightmap.rs`**:
   - Extract `pub fn compute_spill(ground: &[f32], res: UVec2) -> Vec<f32>`.
   - Update `bake_height_map` to read `terrain.grid().heights()` directly instead of calling `sample_ground_height`, and build `CoastField` from `grid` metadata.

### Files Touched
- `src/terrain/terrain.rs` (and optional `src/terrain/grid.rs` / `src/terrain/erosion.rs`)
- `src/terrain/heightmap.rs`
- `src/terrain/chunks.rs` (`terrain.clone()`)
- `src/terrain/mod.rs`

### Risks
- **CPU/GPU shoreline desync**: Any difference between `HeightGrid::sample` and `coast.wesl` (`coast_weight_of`) shifts water shorelines by `0.5 * texel`. Keep the exact same clamped `uv * res - 0.5` math.
- **Grid resolution vs. erosion detail**: At `world = 3000.0` and `grid_resolution = 512`, 1 texel is `5.86m`, so fine erosion gullies won't resolve unless `grid_resolution` is higher (e.g. `1024`) or `world` is smaller; however, `compute_spill` Dijkstra cost grows as $O(N \log N)$ with grid size. Keep `grid_resolution` configurable and profile `Startup` with `ProfilerPlugin`.
- **Non-deterministic erosion**: If hydraulic droplet simulation uses thread-local/non-seeded RNG, terrain will differ across runs or regenerations. Always seed erosion passes deterministically from `TerrainConfig`.

### What to Test Before Moving to Stage 3
- **Bilinear & `coast.wesl` parity**: `HeightGrid::sample` matches generator values at texel centers and matches a Rust mirror of `coast.wesl` across interior, edge, and out-of-bounds points.
- **Erosion invariants**: With erosion enabled, deterministic re-runs produce bit-identical `HeightGrid`s; thermal erosion reduces slopes steeper than `talus_angle` while conserving finite heights (`!is_nan`, within expected bounds).
- **Watertight LOD seams**: `build_chunk_mesh` across adjacent LOD levels (`0..3`) stays watertight on an eroded `Noise` grid.
- **Async chunk & `FixedUpdate` suites**: `chunks.rs` async mesh/collider tests and `currents.rs` / `submersion.rs` manual-clock tests pass.

---

## Stage 3 — Console Commands & Toggles for Terrain and Water Simulations

With `HeightGrid` in place, Stage 3 adds two clean sets of console controls without needing runtime incremental sculpting:
1. **Terrain simulation toggles & on-demand regeneration** (`erosion`, `generator`, `seed`).
2. **Water simulation toggles** (shader features, tides, swells, currents, splashes, wetness, underwater).

### Migration Steps

#### A. Terrain Simulation Toggles & On-Demand Regeneration
1. **Track `revision: u64` on `Terrain`, `TerrainChunk`, `ChunkMeshTask`, and `ColliderRevision`**:
   - Add `pub fn regenerate(&mut self)` on `Terrain`: builds a fresh `Arc<HeightGrid>` from `self.config` (running enabled passes like erosion), swaps `self.grid`, and increments `self.revision`.
   - In `src/terrain/chunks.rs`:
     - Store `revision: u64` on `TerrainChunk` and `ChunkMeshTask`, plus `ColliderRevision(u64)` on collider chunks.
     - `update_chunk_lods` rebuilds chunks whenever `chunk.revision != terrain.revision()` (alongside `lod` / `flags`), letting the existing `AsyncComputeTaskPool` pipeline swap meshes asynchronously without freezing the frame.
     - `sync_chunk_colliders` rebuilds active colliders within `collider_radius` when `ColliderRevision.0 != terrain.revision()`.
2. **Re-upload `TerrainHeightMap` in-place**:
   - In `src/terrain/heightmap.rs`, add a `sync_height_map` system checking `heightmap.revision != terrain.revision()`: recompute full `spill` from `terrain.grid().heights()` and overwrite `images.get_mut(&heightmap.image).data` in-place (keeping the same `Handle<Image>` bound to `WaterMaterialExt.coast_map`).
3. **Sync downstream terrain caches**:
   - Update `SpawnGroundHeight` when `terrain.revision()` changes so `respawn_character` stays above mutated ground.
   - In `src/integration/debug_views.rs`, track `terrain_revision: u64` in `DebugCurtains` so active Level-2 curtains re-drape over regenerated terrain.
4. **Register terrain console commands (`src/terrain/mod.rs`)**:
   - `toggle_erosion` (flips `config.erosion.enabled` and calls `terrain.regenerate()`).
   - `set_terrain_generator <crater|noise> [seed] [octaves] [scale]` (updates `config.generator` and calls `terrain.regenerate()`).
   - `regenerate_terrain [seed]`.

#### B. Water Simulation Effect Toggles
1. **Add simulation flags to `SeaState` (`src/terrain/water.rs`)**:
   - Add explicit toggles such as `pub tides_enabled: bool`, `pub swells_enabled: bool`, `pub wetness_enabled: bool` (plus keeping `wave_energy`).
   - Gate `drive_sea_level` tide oscillation on `tides_enabled` (holding `sea_level = base_level` and `level_rate = 0.0` when off), gate `SwellSet` evaluation on `swells_enabled` (pushing `SwellSet::default()` to `WaterMaterialExt.swells` and skipping CPU swell offset when off), and gate `sync_wetness` on `wetness_enabled` (setting `wetness.height = f32::MIN` when off).
2. **Expose shader-level water toggles via `WaterMaterialExt` (`src/terrain/water.rs`)**:
   - Register commands (`command!` in `src/terrain/mod.rs`) mutating `WaterMaterialExt` on `WaterSurface`:
     - `toggle_water_coast` (`ext.coastal`, toggles `WATER_COAST` vertex/fragment shoaling & spill discard)
     - `toggle_water_ripples` (`ext.ripples`, toggles `WATER_RIPPLES`)
     - `toggle_water_foam` (`ext.foam`, toggles `WATER_FOAM`)
     - `toggle_water_optics` (`ext.optics`, toggles `WATER_OPTICS`)
     - `toggle_water_body_foam` (`ext.body_foam`, toggles `WATER_BODY_FOAM`)
     - `toggle_water_tides`, `toggle_water_swells`, `toggle_water_wetness`
   - Note: `WaterMaterialKey` already hashes all 5 `WaterMaterialExt` bools, so toggling them recompiles/switches pipeline variants automatically with zero extra shader plumbing!
3. **Integration-level water toggles (`src/integration/`)**:
   - Keep cross-feature simulation toggles inside `src/integration/` to respect `tests/module_isolation.rs`:
     - `src/integration/currents.rs`: add `CurrentsToggle` + `toggle_currents` command (`character.water.current = Vec3::ZERO` when disabled).
     - `src/integration/ripples.rs`: add `SplashesToggle` + `toggle_splashes` command (skips `emit_character_splashes` when disabled, while keeping manual `splash` command working).
     - (`src/integration/post_process.rs` already has `toggle_underwater`.)
   - Note on `body_foam.rs`: if `toggle_water_body_foam` is exposed on `WaterMaterialExt`, avoid having `sync_body_sdf` unconditionally overwrite `extension.body_foam = true` every frame—either only initialize it once when the body SDF first finishes baking, or respect an explicit `BodyFoamToggle`.

### Files Touched
- `src/terrain/terrain.rs` (and `grid.rs` / `erosion.rs`): `revision`, `regenerate()`, `ErosionConfig`
- `src/terrain/chunks.rs`: `revision` on `TerrainChunk`, `ChunkMeshTask`, `ColliderRevision`
- `src/terrain/heightmap.rs`: `revision` tracking + in-place `sync_height_map`
- `src/terrain/water.rs`: `SeaState` simulation flags + water toggle commands
- `src/terrain/mod.rs`: command registrations, `SpawnGroundHeight` sync, `sync_height_map` schedule
- `src/integration/currents.rs`: `CurrentsToggle` + `toggle_currents`
- `src/integration/ripples.rs`: `SplashesToggle` + `toggle_splashes`
- `src/integration/body_foam.rs`: guard `body_foam` overwrite so console toggle sticks
- `src/integration/debug_views.rs`: re-drape `DebugCurtains` on `terrain.revision()` changes

### Risks
- **System overwrite fighting console toggles**: `body_foam::sync_body_sdf` currently sets `extension.body_foam = true` every frame, and `drive_sea_level` overwrites `sea_level` every frame. Any command toggle must control the source-of-truth flag rather than being clobbered on the next `Update`.
- **Stale `ChunkMeshTask` after rapid regenerations**: Running `toggle_erosion` twice quickly could let an older `ChunkMeshTask` finish after the newer `regenerate()`. Tagging `ChunkMeshTask` with `revision` and having `update_chunk_lods` check `chunk.revision != terrain.revision()` guarantees the latest revision always wins.
- **Swell parity between GPU `UnderwaterPostProcess` and `SeaState`**: If `swells_enabled = false` sets `SwellSet::default()` on `SeaState.swells`, `sync_underwater_post_process` (`src/integration/post_process.rs`) will automatically stop wave meniscus motion on the lens too—make sure `SeaState::swells` is the single source of truth rather than mutating only `WaterMaterialExt.swells`.
- **Module isolation regressions**: Placing `toggle_currents` or `toggle_splashes` inside `src/terrain/` would violate `tests/module_isolation.rs`; keep those commands in their `src/integration/*` modules (or inside `src/ripples/mod.rs` for ripple emission).

### What to Test Before Considering Stage 3 Complete
- **Regeneration round-trip**: Trigger `regenerate` / `toggle_erosion` in a test app; verify `terrain.revision()` increments, `TerrainHeightMap` updates `Image.data` in-place with the same `Handle<Image>`, active `Collider`s update, and all `TerrainChunk`s converge to the new `revision` with watertight LOD seams.
- **Water toggle stickiness**: Toggle `body_foam`, `swells`, `tides`, `currents`, `splashes`, and `wetness` off and step `Update` + `FixedUpdate` multiple frames; verify `sync_body_sdf`, `drive_sea_level`, `sync_water_params`, `sync_tidal_currents` (with `TimeUpdateStrategy::ManualDuration`), and `emit_character_splashes` do not re-enable or overwrite the disabled state.
- **Downstream sync**: Verify `SpawnGroundHeight` and `DebugCurtains` update after `terrain.regenerate()`.
- **Isolation & code hygiene**: `cargo test --workspace`, `cargo test --test module_isolation`, `cargo test --test crate_isolation`, and zero code comments in all edited files.