## Cross-Stage Invariants

Before starting Stage 1, note four properties of the current codebase that govern all three stages:

1. **Single height seam**: `Terrain::sample_ground_height(&self, x: f32, z: f32) -> f32` is already the sole entry point for `build_chunk_mesh`, `build_chunk_collider`, `ground_normal`, `bake_height_map`, `terrain_gradient`, `SeaState::shore_surface_height_at`, `SpawnGroundHeight`, `integration/currents.rs`, `integration/submersion.rs`, and `integration/debug_views.rs`. Preserving this signature keeps every consumer untouched across stages.
2. **Thread-safe chunk sampling**: `src/terrain/chunks.rs` copies `Terrain` (`let sampler = *terrain;`) into `AsyncComputeTaskPool` tasks (`build_chunk_mesh`). Once `Terrain` owns heap data in Stage 2+, it must shift from `Copy` to cheap `Clone` over immutable snapshots (`Arc<HeightGrid>`) so background tasks never race or block on locks.
3. **Existing `world` size vs. unit tests**: `TerrainConfig::default().world` is `Vec2::splat(3000.0)` (`50×50 = 2500` chunks at `chunk_size = 60.0`), while existing tests in `src/terrain/terrain.rs` and `src/terrain/chunks.rs` still assert `5×5 = 25` chunks (`300.0`). Reconcile that baseline (either update those assertions or pass an explicit `300.0` config in test fixtures) before adding stage tests.
4. **Module isolation & test clocks**: `src/terrain/` must never import peer features (`tests/module_isolation.rs`), all new code must contain zero comments, and `FixedUpdate` integration tests (`currents.rs`, `submersion.rs`) must keep using `TimeUpdateStrategy::ManualDuration`.

---

## Stage 1 — Pluggable Generation (`Generator`)

### Migration Steps
1. **Define `Generator` in `src/terrain/terrain.rs`**:
   - Add `#[derive(Clone, Copy, Debug, PartialEq)] pub enum Generator { Crater, Noise { seed: u32, octaves: u32, scale: f32 } }`.
   - Implement `Generator::sample(&self, world: Vec2, x: f32, z: f32) -> f32`:
     - `Crater` runs the existing `CRATER_DEPTH` / `CRATER_RIM_X` / `CRATER_RIM_Z` / `CRATER_INSET` smoothstep formula verbatim.
     - `Noise { seed, octaves, scale }` evaluates deterministic, stateless 2D FBM (pure coordinate-hashed gradient noise with quintic fade and normalized octave sum). Avoid calling `noise::Perlin::new(seed)` inside `sample_ground_height`, as it rebuilds a 512-entry permutation table on every vertex/normal call.
2. **Wire into `TerrainConfig` and `Terrain`**:
   - Add `pub generator: Generator` to `TerrainConfig` (default `Generator::Crater` initially so startup parity is preserved, or `Noise` once validated).
   - Delegate `Terrain::sample_ground_height(&self, x: f32, z: f32) -> f32` to `self.0.generator.sample(self.0.world, x, z)`.
   - Keep `Terrain` `Copy` so `chunks.rs` needs no changes in Stage 1.
3. **Optionally expose generator switching for Stage 1 verification**:
   - Re-export `Generator` from `src/terrain/mod.rs` so scenarios/tests can construct `Terrain` with either variant cleanly.

### Files Touched
- `src/terrain/terrain.rs`: add `Generator`, `TerrainConfig::generator`, route `sample_ground_height`.
- `src/terrain/mod.rs`: export `Generator`.

### Risks
- **Stateless noise cost vs. table allocation**: Constructing `noise::Perlin::new(seed)` inside `sample_ground_height` will allocate/initialize tables millions of times across chunk meshes and `ground_normal` finite differences, causing severe startup/LOD stalls. Use pure arithmetic coordinate-hash gradient noise so `Generator` remains `Copy`, stateless, and allocation-free.
- **Boundary ridges blocking ocean spill**: Unlike `Crater` (whose perimeter is `-30.0` everywhere), raw FBM `Noise` can place ridges higher than `sea_level` along the 4 world edges. In `heightmap.rs`, priority-flood `spill` seeds from the 4 edges, so edge ridges prevent open-ocean flooding of interior valleys. Decide whether `Noise` maps include an optional edge basin skirt or intentionally form enclosed inland basins.
- **Interior LOD seams exposed**: `Crater` has a flat interior (`y = 0.0`), so interior LOD stitching was previously hidden. `Noise` puts slopes and curvature across every chunk boundary, exposing any normal or index-stitch bug immediately.
- **Spawn elevation at `(0, 0)`**: `TerrainPlugin::build` computes `SpawnGroundHeight` from `sample_ground_height(0.0, 0.0)`; if `Noise` puts `(0, 0)` deep underwater (`< sea_level`), the character spawns submerged unless amplitude/offset places `(0, 0)` above `sea_level`.

### What to Test Before Moving to Stage 2
- **Determinism & edge cases**: Identical `(seed, octaves, scale, x, z)` produces bit-identical `f32`; distinct seeds produce distinct heights; `octaves == 0`, `scale <= 0.0`, and large coordinates remain finite.
- **`Crater` parity**: `Generator::Crater` matches the previous analytic output across plain, rim slopes, and corners.
- **Watertight LOD stitching over `Noise`**: Parameterize `a_stitched_fine_edge_meets_the_coarse_neighbour_exactly` in `src/terrain/terrain.rs` over `Generator::Noise` (and test North/South/East/West stitches, not just East) so curved interior seams meet coarse chords with zero crack.
- **Spill bake**: Run `bake_height_map` with `Generator::Noise` and verify all `ground` and `spill` texels are finite and `spill >= ground`.
- **Integration `FixedUpdate` parity**: Run `cargo test` for `integration::currents` and `integration::submersion` (manual clocks) and `cargo test --test module_isolation`.

---

## Stage 2 — Grid-Authoritative Heightfield

### Migration Steps
1. **Introduce `HeightGrid` (`src/terrain/grid.rs` or inside `src/terrain/terrain.rs`)**:
   - Store `resolution: UVec2`, `bounds_min: Vec2`, `texel: Vec2`, `inv_size: Vec2`, `heights: Vec<f32>`.
   - Add `pub grid_resolution: u32` (default `512`, replacing the standalone `HEIGHT_MAP_RESOLUTION` constant in `heightmap.rs`) to `TerrainConfig`.
   - Seed `HeightGrid::from_config(config: &TerrainConfig)` once by evaluating `config.generator.sample(config.world, world_x, world_z)` at texel centers `bounds_min + (ix + 0.5, iz + 0.5) * texel` — matching the exact layout currently baked in `heightmap.rs`.
2. **Match `coast.wesl` bilinear sampling on CPU**:
   - Implement `HeightGrid::sample(&self, x: f32, z: f32) -> f32` using the exact convention in `crates/bevy_shader_kit/src/shaders/water/coast.wesl` (`coast_weight_of` + clamped 4-corner bilinear interpolation over texel centers):
     - `u = ((x - bounds_min.x) * inv_size.x * res.x - 0.5).clamp(0.0, res.x - 1.0)`
     - `v = ((z - bounds_min.y) * inv_size.y * res.y - 0.5).clamp(0.0, res.y - 1.0)`
     - Bilinearly mix `(ix0, iz0)`, `(ix1, iz0)`, `(ix0, iz1)`, `(ix1, iz1)` where `ix1 = (ix0 + 1).min(res_x - 1)`.
   - Scale `Terrain::ground_normal` finite-difference step safely (`step = NORMAL_SAMPLE_STEP.max(texel_min)` or `texel * 0.5`) so normals don't capture bilinear cell creases when `texel > 0.5m`.
3. **Make `Terrain` own `Arc<HeightGrid>`**:
   - Change `Terrain` from `Copy` to `Clone`: `pub struct Terrain { config: TerrainConfig, grid: Arc<HeightGrid> }`.
   - Route `Terrain::sample_ground_height(&self, x, z)` through `self.grid.sample(x, z)`.
   - Expose `pub fn grid(&self) -> &HeightGrid` and `pub fn config(&self) -> TerrainConfig`.
   - In `src/terrain/chunks.rs`, change the two `let sampler = *terrain;` sites (`spawn_terrain_chunks`, `update_chunk_lods`) to `let sampler = terrain.clone();` (`O(1)` atomic refcount bump).
4. **Invert `src/terrain/heightmap.rs`**:
   - Extract `pub fn compute_spill(ground: &[f32], res: UVec2) -> Vec<f32>` as a pure function operating on `&[f32]`.
   - Change `bake_height_map(terrain: &Terrain, images: &mut Assets<Image>) -> TerrainHeightMap` so it reads `terrain.grid().heights()` instead of calling `terrain.sample_ground_height(x, z)`.
   - Populate `CoastField` from `grid.bounds_min`, `grid.texel`, `grid.inv_size`, and `grid.resolution` instead of hardcoding `HEIGHT_MAP_RESOLUTION`.

### Files Touched
- `src/terrain/terrain.rs` (and optional `src/terrain/grid.rs` submodule): `HeightGrid`, `TerrainConfig::grid_resolution`, `Terrain` holding `Arc<HeightGrid>`, `Clone` instead of `Copy`, bilinear `sample_ground_height`.
- `src/terrain/heightmap.rs`: extract `compute_spill`, read `terrain.grid().heights()`, build `CoastField` from grid metadata.
- `src/terrain/chunks.rs`: `let sampler = terrain.clone();` in `spawn_terrain_chunks` and `update_chunk_lods`.
- `src/terrain/mod.rs`: declare `mod grid;` if separated.

### Risks
- **CPU/GPU half-texel shift**: If `HeightGrid::sample` uses texel corners (`0..world`) while `coast.wesl` uses texel centers (`0.5..res - 0.5`), the water shoreline (`water.wesl` / `water_vertex.wesl`) will misalign from terrain meshes and colliders by `0.5 * texel` (`~2.9m` at `3000m / 512`). Mirroring `coast_weight_of` exactly eliminates this.
- **Resolution mismatch (`world = 3000.0` vs. `512²`)**: At `3000m`, `512²` gives `5.86m` per texel while `lod 0` mesh quads are `0.94m` (`60 / 64`). High-frequency generator noise is low-pass filtered onto the `5.86m` grid. Keeping `grid_resolution` configurable (`512`, `1024`, etc.) lets you trade memory (`1 MB` vs `4 MB`) and startup spill cost for resolution without hardcoding.
- **Bilinear normal kinks**: Sampling `NORMAL_SAMPLE_STEP = 0.5` across a `5.86m` bilinear cell exposes subtle normal discontinuities along texel-center grid lines unless step scales with `texel` or uses `CoastField`-style smoothing.
- **Startup stall on oversized grids**: `BinaryHeap` Dijkstra over `2048²` (`4.2M` cells) takes significantly longer than `512²` (`262k` cells) during `Startup` (`setup_water_surface`). Keep default `512` (or `1024`) and monitor with `ProfilerPlugin`.

### What to Test Before Moving to Stage 3
- **Texel-center and bilinear accuracy**: At exact texel centers, `HeightGrid::sample` equals the generator value; at midpoints between 4 texels, it equals their mean; out-of-bounds `(x, z)` clamps cleanly without panic or NaN.
- **CPU/GPU `coast.wesl` parity test**: Run a unit test comparing `HeightGrid::sample` against a direct Rust translation of `coast.wesl`'s `coast_weight_of` + `terrain_height_at` at interior, seam, and out-of-bounds coordinates.
- **Watertight LOD seams from grid**: Verify `build_chunk_mesh` across adjacent `lod 0` / `lod 1` / `lod 2` / `lod 3` chunks produces identical border heights and normals when reading from `HeightGrid`.
- **`compute_spill` unit tests**: Verify flat plain spills `<= 0.0`, closed crater holds rim saddle level, and breached rim drains to boundary height.
- **Async pipeline & integration tests**: Confirm `chunks.rs` tests (`chunk_meshes_arrive_from_the_task_pool`, `colliders_follow_the_viewer`) and `FixedUpdate` tests (`currents.rs`, `submersion.rs`) pass.

---

## Stage 3 — Simulation Plumbing (Runtime Terrain Mutation)

### Migration Steps
1. **Revisioned copy-on-write edits on `Terrain`**:
   - Add `revision: u64` to `Terrain`, plus per-chunk target revisions (`chunk_revisions: Vec<u64>` indexed by `iz * count.x + ix`) and a dirty-spill tracker (`pending_spill_dirty: Option<DirtySpillBounds>` or raised/lowered texel lists).
   - Provide an edit API on `Terrain` (e.g. `apply_edit(world_bounds, |grid_slice| ...)` or `modify_region(world_rect, fn)`):
     - Uses `Arc::make_mut(&mut self.grid)` so in-flight `AsyncComputeTaskPool` tasks continue reading the prior immutable `Arc<HeightGrid>` snapshot with zero locks or data races.
     - Records per-texel deltas (whether any texel rose above `old_spill`, or lowered below `old_spill`).
     - Increments `self.revision`, expands the edited world rectangle by `padding = grid.texel + Vec2::splat(normal_step)` (covering bilinear interpolation + `ground_normal` stencil across chunk borders), and bumps `chunk_revisions` for every chunk intersecting that padded rectangle.
2. **Incremental mesh & collider rebuild in `src/terrain/chunks.rs`**:
   - Add `pub revision: u64` to `TerrainChunk` and `ChunkMeshTask`, and add `#[derive(Component)] struct ColliderRevision(u64)` to collider-bearing chunks.
   - In `update_chunk_lods`: skip rebuilding only when `chunk.lod == lod && chunk.flags == flags && chunk.revision == target_revision` (and `pending.revision == target_revision`). Dirty chunks enqueue a `ChunkMeshTask` carrying `sampler = terrain.clone()` and `revision: target_revision`.
   - In `attach_finished_chunk_meshes`: set `chunk.revision = pending.revision`. If `terrain.chunk_revision(chunk.coord) != chunk.revision`, the next `update_chunk_lods` tick automatically schedules the newer revision.
   - In `sync_chunk_colliders`: alongside attach/detach by `collider_radius`, rebuild `build_chunk_collider(&terrain, chunk.coord)` when an active collider's `ColliderRevision.0 != terrain.chunk_revision(chunk.coord)` (optionally budgeting max collider rebuilds per frame if continuous brushes edit many chunks at once).
3. **Incremental `spill` recompute & in-place GPU upload in `src/terrain/heightmap.rs`**:
   - Store `pub spill: Vec<f32>` (and `last_revision: u64`) inside `TerrainHeightMap`.
   - Implement `update_spill_incremental(ground: &[f32], spill: &mut [f32], res: UVec2, raised: &[u32], lowered: &[u32]) -> bool`:
     - **Short-circuit**: if `raised` has no texels with `ground[i] > spill[i]` and `lowered` has no texels with `ground[i] < spill[i]` (e.g., raising/lowering underwater mud below the spill plane), return `false` (`O(edited_texels)`).
     - **Raises first**: starting from lifted texels where `ground[i] > spill[i]`, flood outward through neighbors that are valid interior basins (`spill[n] < INFINITY` and `spill[n] >= old_threshold`) to invalidate `spill[n] = INFINITY` (except world-border texels, which reset to `ground[border]`), then run a multi-source priority-flood seeded from the valid perimeter neighbors of the invalidated set (`spill != INFINITY`) to refill exact `spill`.
     - **Lowers second**: push lowered texels where `ground[i] < spill[i]` (along with their updated `max(ground[i], min_neighbor_spill)`) into a min-heap and propagate strictly while `candidate < spill[neighbor]`, stopping naturally at the drained basin's boundary.
   - Add an `Update` system `sync_height_map(mut terrain: ResMut<Terrain>, mut heightmap: ResMut<TerrainHeightMap>, mut images: ResMut<Assets<Image>>)` running before `water::sync_water_params`:
     - Drains pending dirty texels from `Terrain`, updates `TerrainHeightMap::spill` incrementally, and overwrites the existing `Image` bytes in `images.get_mut(&heightmap.image)` in-place (preserving the `Handle<Image>` bound to `WaterMaterialExt.coast_map`).
4. **Sync downstream terrain-derived caches (`src/integration/`)**:
   - In `src/integration/debug_views.rs`: store `terrain_revision: u64` in `DebugCurtains` so `sync_debug_curtains` regenerates `curtain_mesh(&terrain, &segments)` when `terrain.revision() != state.terrain_revision`, even if `segments` didn't change.
   - Keep `integration/currents.rs` and `integration/submersion.rs` reading `Res<Terrain>` dynamically (already live), and ensure `setup_water_surface` / `sync_height_map` ordering keeps `TerrainHeightMap` initialized cleanly in `Startup`.

### Files Touched
- `src/terrain/terrain.rs` (and `grid.rs`): `revision`, `chunk_revisions`, padded dirty-bounds-to-chunk mapping, `Arc::make_mut` edit API, raised/lowered dirty texel tracking.
- `src/terrain/chunks.rs`: `revision` on `TerrainChunk` / `ChunkMeshTask`, `ColliderRevision`, dirty checks in `update_chunk_lods`, `attach_finished_chunk_meshes`, and `sync_chunk_colliders`.
- `src/terrain/heightmap.rs`: `spill` cache in `TerrainHeightMap`, `update_spill_incremental`, in-place `Assets<Image>` update (`sync_height_map`).
- `src/terrain/water.rs` & `src/terrain/mod.rs`: share `TerrainHeightMap` initialization/update cleanly in `Startup` and `Update` (plus optional simulation/edit command in `terrain` for live testing).
- `src/integration/debug_views.rs`: invalidate `DebugCurtains` on `terrain.revision()` changes.

### Risks
- **Missing neighbor rebuild on border edits**: Editing a texel within `1 texel + normal_step` of a chunk edge alters `sample_ground_height` and `ground_normal` on the adjacent chunk. Failing to pad the dirty-rect-to-chunk intersection breaks LOD seam watertightness.
- **1-frame async seam lag during edits**: When two neighboring chunks rebuild concurrently on `AsyncComputeTaskPool`, one can attach 1 frame before the other, producing a transient 1-frame seam until both land. Self-heals as soon as the second task completes; if unacceptable for large discrete steps, attach completed tasks from the same revision together.
- **Raise-then-lower ordering hazard in incremental spill**: Applying lowerings before raisings in the same batch can cause newly invalidated cells to miss downward drains. Always run raise-invalidation + perimeter flood before downward heap propagation, and coalesce multiple edits per frame before running `sync_height_map`.
- **Large dam-breach / continuous brush frame spikes**: Rebuilding dozens of Avian `Collider::heightfield`s or invalidating a continent-sized spill basin in one `Update` tick can spike frame time (`ProfilerPlugin`). Budget collider rebuilds per frame, keep `collider_radius` local, and coalesce edits.
- **Stale curtain debug views**: `DebugCurtains` currently skips mesh rebuild when `segments` is unchanged; without checking `terrain.revision()`, elevated Level-2 curtains float or sink relative to mutated terrain.

### What to Test Before Considering Stage 3 Complete
- **Targeted dirty chunking**: Edit a single interior chunk well away from borders (`> texel + normal_step` inside); verify only that chunk's `target_revision` increments, only 1 `ChunkMeshTask` spawns, and the other 24/2499 chunks do zero rebuild work.
- **Border-padded seam test**: Edit directly across a chunk border separating an `lod = 0` chunk and a stitched `lod = 1` chunk; settle tasks and assert both chunks incremented revision and the stitched fine edge matches the coarse chord vertex-for-vertex.
- **Collider synchronization**:
  - Edit terrain under `TerrainViewer`; assert the active chunk's `Collider` updates to the new heightfield.
  - Edit terrain far outside `collider_radius`; assert no `Collider` spawns early, then move `TerrainViewer` over it and assert the newly attached `Collider` uses the mutated height.
- **Incremental spill equivalence (property/fuzz test)**: Apply sequences of random raises, lowers, dam closures, and dam breaches on a test grid (e.g. `64×64`), asserting `update_spill_incremental` matches a fresh full `compute_spill` element-for-element after every edit and updates `Image.data` in-place without changing `Handle<Image>`.
- **Full isolation & simulation checks**:
  - `cargo test --test module_isolation` and `cargo test --test crate_isolation`.
  - `FixedUpdate` tests (`currents.rs`, `submersion.rs` with `TimeUpdateStrategy::ManualDuration`) observe mutated terrain heights and gradients.
  - `debug_views.rs` unit/app test confirms `DebugCurtains` mesh Y positions update after a `Terrain` edit.