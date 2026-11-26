# refactor.md

## Original instruction (verbatim)

> complete terrain.md

Ticket: `tickets/incomplete/terrain.md` — three stages: (1) pluggable `Generator`, (2) grid-authoritative
heightfield + erosion passes, (3) console commands & toggles for terrain and water simulations.

## Baseline findings (2026-11-25)

- `cargo test`: 165 pass, 3 fail — all three from the `TerrainConfig::default().world = 1000.0` vs
  tests asserting world 3000 (`chunks_tile_the_world_exactly` 50×50 chunks,
  `lod_selection_follows_distance_with_hysteresis` ring = 60, `neighbouring_chunks_share_their_seam`
  float-exact seam which only holds when `world / chunk_count` is exact: 3000/50 = 60 exactly,
  1000/17 = 58.82353… is not). This is the reconciliation the ticket asks for: default becomes
  `3000.0` (2500 chunks).
- `Terrain` is `Copy`; only two `*terrain` copy sites (`chunks.rs`), so `Copy → Clone + Arc<HeightGrid>`
  is contained.
- `coast.wesl` bilinear convention (`coast_weight_of` + `terrain_height_at`): `uv = (p - bounds_min) *
  inv_size`, `pos = clamp(uv * res - 0.5, 0, res - 1)`, floor/fraction bilinear with clamped corner
  loads. The CPU `HeightGrid::sample` must mirror this exactly.
- Exact-equality tests that constrain grid sampling: `the_plain_is_flat_and_the_rim_sinks`
  (sample(0,0) == 0.0 exactly — true because all four bilinear corners sit inside the flat inset;
  sample(0, edge) == -30.0 exactly — out-of-bounds clamps to the last texel row whose crater value
  is exactly -CRATER_DEPTH), `the_collider_fills_its_chunk_and_its_depth` (aabb.mins.y == -30.0
  exactly — rim cells clamp t to 1.0), seam tests (same function, same coords).
- `WaterMaterialKey` already hashes the five `WaterMaterialExt` bools → toggling them swaps pipeline
  variants with no shader plumbing.
- `sync_body_sdf` overwrites `extension.body_foam = true` every frame; `drive_sea_level` overwrites
  `sea_level`/`level_rate` every frame — toggles must gate these sources, not fight them.
- `sync_underwater_post_process` reads `sea_state.swells` directly → swell toggle must gate via a
  single source of truth (`SeaState::effective_swells()`), not just the material.
- Command names derive from `module_path!()`: terrain commands land as `terrain>toggle_erosion` etc.;
  `toggle_currents`/`toggle_splashes` must be registered from `src/integration/*` (isolation rule).

## Plan

### Stage 1 — `Generator`
- `terrain.rs`: `#[derive(Clone, Copy, Debug, PartialEq)] pub enum Generator { Crater, Noise { seed, octaves, scale } }`
  with `sample(&self, world: Vec2, x: f32, z: f32) -> f32`.
  - `Crater`: existing smoothstep math moved verbatim (bit-identical parity test).
  - `Noise`: pure-arithmetic seeded 2D gradient noise (splitmix-style integer hash, 8 lattice
    gradients, quintic fade) + FBM (gain 0.5, lacunarity 2, normalized by summed amplitude).
    No `noise::Perlin` construction per call. Degenerate safety: `octaves == 0` or `scale <= 0`
    (or NaN) → 0.0; octaves capped at 16 so `freq = scale * 2^o` can never overflow to inf/NaN.
  - Edge falloff: within `0.08 * min(world)` of any border (and outside the world) blend toward
    `NOISE_EDGE_DEPTH = -4.0` (below default sea level) so the ocean always rings the map and `spill`
    can flood interior valleys — avoids the enclosed-basin risk the ticket names.
- `TerrainConfig`: add `generator: Generator` (default `Crater`); default `world` → `3000.0`.
- `sample_ground_height` routes to `self.0.generator.sample(self.0.world, x, z)`.
- Tests: crater parity, noise determinism, degenerate params, stitch with Noise on N/S/E/W,
  bake with Noise (finite, spill >= ground).

### Stage 2 — `HeightGrid` + erosion
- New `src/terrain/grid.rs`: `HeightGrid { resolution: UVec2, bounds_min, texel, inv_size, heights: Vec<f32> }`,
  seeded at texel centers from `Generator`; `sample(x, z)` = exact `coast.wesl` bilinear mirror;
  accessors `heights()`, `resolution()`, `texel()`, `bounds_min()`, `inv_size()`.
- New `src/terrain/erosion.rs`: `ErosionConfig { enabled, seed, thermal_iterations, hydraulic_drops, talus_angle }`
  (Copy, default disabled) on `TerrainConfig` (`grid_resolution: u32` default 512 also moves onto config).
  Thermal pass (steepest-neighbour talus relaxation) and hydraulic droplet pass (deterministic
  splitmix64 per drop, bilinear splat erode/deposit) run on the flat `Vec<f32>` before the `Arc` exists.
- `Terrain` → `Clone` struct `{ config, revision: u64, grid: Arc<HeightGrid> }` with `Terrain::new(config)`;
  `sample_ground_height` → `grid.sample`; `ground_normal` step = `NORMAL_SAMPLE_STEP.max(texel_min * 0.5)`.
- `chunks.rs`: `let sampler = terrain.clone();` (2 sites).
- `heightmap.rs`: extract `pub fn compute_spill(ground: &[f32], res: UVec2) -> Vec<f32>`; `bake_height_map`
  reads `terrain.grid().heights()`; `CoastField` built from grid metadata; `HEIGHT_MAP_RESOLUTION` const deleted.
- Tests: bilinear == generator at texel centers, Rust mirror of `coast.wesl` parity (interior/edge/OOB),
  erosion determinism (bit-identical reruns), thermal reduces max slope past talus + finite, watertight
  LOD seams on eroded Noise grid, existing async + FixedUpdate suites.

### Stage 3 — revision, commands, toggles
- `Terrain::regenerate(&mut self)` (rebuild grid, `revision += 1`), `set_config`.
- `chunks.rs`: `revision` on `TerrainChunk` + `ChunkMeshTask`, `ColliderRevision(u64)` on collider chunks;
  `update_chunk_lods` rebuilds on `chunk.revision != terrain.revision()`; `sync_chunk_colliders` rebuilds
  in-radius colliders on `ColliderRevision.0 != terrain.revision()`.
- `heightmap.rs`: `TerrainHeightMap.revision` + `sync_height_map` recomputing spill and overwriting
  `image.data` in place (same `Handle<Image>`).
- `terrain/mod.rs`: `sync_spawn_ground_height` (SpawnGroundHeight follows revision); commands
  `toggle_erosion`, `set_terrain_generator <crater|noise> [seed] [octaves] [scale]`, `regenerate_terrain [seed]`.
- `water.rs`: `SeaState` gains `tides_enabled`, `swells_enabled`, `wetness_enabled` + `effective_swells()`;
  `drive_sea_level` holds `sea_level = base_level`, `level_rate = 0` when tides off; `sync_water_params`
  pushes `effective_swells()`; `sync_wetness` writes `f32::MIN` when off; commands `toggle_water_coast`,
  `toggle_water_ripples`, `toggle_water_foam`, `toggle_water_optics`, `toggle_water_body_foam`,
  `toggle_water_tides`, `toggle_water_swells`, `toggle_water_wetness`.
- `integration/post_process.rs`: `settings.swells = sea_state.effective_swells()`.
- `integration/currents.rs`: `CurrentsToggle` + `toggle_currents` (zeroes `character.water.current` when off).
- `integration/ripples.rs`: `SplashesToggle` + `toggle_splashes` (skips `emit_character_splashes`; manual
  `splash` command unaffected).
- `integration/body_foam.rs`: set `body_foam = true` only when `body_sdf` handle changes (first bake),
  so the console toggle sticks.
- `integration/debug_views.rs`: `DebugCurtains.terrain_revision` forces re-drape on regeneration.
- Tests: regeneration round-trip (revision, in-place image, colliders, chunk convergence), toggle
  stickiness across `Update` + `FixedUpdate` (manual clock), downstream sync, `module_isolation`,
  `crate_isolation`, zero comments.

## Decisions considered and rejected

- **No edge falloff on Noise (accept enclosed basins)**: rejected — with arbitrary noise the edge row
  can ridge above sea level and starve every interior valley of `spill`; a sub-sea-level rim keeps the
  ocean connected, matching the crater's behaviour (rim at -30).
- **`noise::Perlin` prebuilt in a `OnceLock`/resource**: rejected — ticket demands pure arithmetic;
  a hand-rolled hash gradient noise is ~30 lines, allocation-free, and trivially deterministic.
- **Keep `Terrain` `Copy` with `Arc` interior mutability for regeneration**: rejected — `Copy` silently
  clones the Arc handle, so `regenerate` through a copy would be a no-op; `Clone` makes the aliasing
  explicit and the two call sites are the only churn.
- **Erosion as async/bevy tasks**: rejected — passes run once per regeneration on the flat grid before
  the `Arc` exists; zero locking, and determinism is testable by re-running the pure function.
- **`toggle_currents`/`toggle_splashes` inside `terrain`**: rejected — violates `tests/module_isolation.rs`;
  they live in their integration modules (command prefix becomes `integration>currents>…` etc.).

## Behaviour changes (review separately from pure refactors)

1. `TerrainConfig::default().world`: 1000.0 → 3000.0 (fixes the 3 failing baseline tests; 2500 chunks).
2. `sample_ground_height` becomes bilinear grid sampling (was analytic per-vertex). Crater values at
   texel centers are bit-identical; between texels the mesh now interpolates the grid instead of
   evaluating the analytic function (sub-cm differences on the crater rim curve).
3. `ground_normal` finite-difference step grows from 0.5 to ~2.93 m at default grid resolution
   (normals slightly smoother).
4. New console commands: `terrain>toggle_erosion`, `terrain>set_terrain_generator`,
   `terrain>regenerate_terrain`, `terrain>toggle_water_{coast,ripples,foam,optics,body_foam,tides,swells,wetness}`,
   `integration>currents>toggle_currents`, `integration>ripples>toggle_splashes`.
5. `sync_body_sdf` no longer forces `body_foam = true` every frame (only on first bake / SDF swap).
6. `sync_underwater_post_process` reads `SeaState::effective_swells()` (identical while swells enabled).

## Implementation log

All three stages implemented; `cargo test`: **195 passed, 0 failed, zero warnings** (baseline was
165/3). Ticket moved to `tickets/complete/0011-terrain.md`.

### Stage 1 — `Generator`
- `terrain.rs`: `Generator { Crater, Noise { seed, octaves, scale } }` with `sample(world, x, z)`.
  `Crater` moved verbatim (parity test proves bit-identity). `Noise`: `hash2` (splitmix-style),
  8 lattice gradients, quintic `fade`, `gradient_noise` (×1.4142135 normalize), `fbm` (gain 0.5,
  lacunarity 2, amplitude-normalized). Safety: `octaves == 0` or `scale <= 0`/NaN → 0.0; octaves
  clamped to 16 so `scale * 2^o` cannot overflow. Edge falloff blends to `NOISE_EDGE_DEPTH = -4.0`
  within `0.08 * min(world)` of a border so the ocean always rings the map (spill can flood valleys).
- `TerrainConfig`: `generator` (default `Crater`), `grid_resolution: 512`, `erosion`; default
  `world` 1000 → 3000 (fixes the 3 baseline failures).
- Tests: crater parity, noise determinism/degenerate params, edge sinking, noise stitch on N/S/E/W
  across lod 0..3 (`assert_stitches_on_every_border`), noise bake finite with `spill >= ground`.

### Stage 2 — `HeightGrid` + erosion
- `grid.rs`: `HeightGrid { resolution, bounds_min, texel, inv_size, heights }` seeded at texel
  centers; `sample` = exact `coast.wesl` bilinear mirror — WGSL `mix` form `a*(1-t)+b*t` (not
  `f32::lerp`), `+1` corner indices clamped, `wy` weights the iz/iz+1 rows. Parity test uses
  asymmetric heights (`ix*3 + iz*7 + 1`) so an axis swap cannot pass; texel-center exactness test
  uses world 512 (dyadic texel → bit-exact round-trip; 1/300 is not).
- `erosion.rs`: `ErosionConfig` (default disabled, deterministic `seed`); thermal = steepest-lower-
  neighbour talus relaxation (conserves the height sum); hydraulic = splitmix64-seeded droplets
  with bilinear splat erode/deposit. **Bug found and fixed during implementation**: the droplet
  start position multiplied the random unit offset by `size` twice, so every drop started outside
  the bounds, saw zero gradient, and evaporated — both seeds left the grid untouched and the
  `assert_ne!` determinism test failed. Fixed by dropping the extra `size *` factor.
- `Terrain`: `Copy → Clone { config, revision: u64, grid: Arc<HeightGrid> }`; `sample_ground_height`
  → `grid.sample`; `ground_normal` step `NORMAL_SAMPLE_STEP.max(texel_min * 0.5)`;
  `terrain_gradient` step scales with texel too (audit amendment).
- `heightmap.rs`: `compute_spill(ground, resolution)` extracted (generalized to non-square);
  `bake_height_map` reads `terrain.grid()`; `HEIGHT_MAP_RESOLUTION` deleted.
- `chunks.rs`: `*terrain` → `terrain.clone()` (plus a per-task `sampler.clone()` for the async move).

### Stage 3 — revision, commands, toggles
- `Terrain::regenerate()` (rebuild grid, `revision += 1`) + narrow `set_generator` /
  `set_erosion_enabled` setters (no public `config_mut` — see audit fixes).
- `chunks.rs`: `revision` on `TerrainChunk` + `ChunkMeshTask`, `ColliderRevision(u64)` inserted at
  spawn and on every collider build. `update_chunk_lods` rebuilds when `chunk.revision != revision`
  and its pending-dedup guard requires `task.revision == revision` (a stale task can never satisfy
  the guard, and a newer regeneration always re-spawns). `attach_finished_chunk_meshes` stamps
  `chunk.revision = pending.revision` — the task's revision, never the terrain's current.
  `sync_chunk_colliders` rebuilds in-radius colliders when `ColliderRevision` is missing or stale.
- `heightmap.rs`: `TerrainHeightMap.revision`; `sync_height_map` (`Option<ResMut>` guard) recomputes
  spill and overwrites `image.data` in place — same `Handle<Image>`, no rebake (test asserts
  `images.len() == 1`) — and refreshes `CoastField` from the grid.
- `mod.rs`: `sync_spawn_ground_height` (`Local<u64>` revision watch); commands `toggle_erosion`,
  `set_terrain_generator <crater|noise> [seed] [octaves] [scale]`, `regenerate_terrain [seed]`
  (reseeds only a `Noise` generator; crater ignores it), all bare paths → `terrain>…`.
- `water.rs`: `SeaState` gains `tides_enabled`/`swells_enabled`/`wetness_enabled` +
  `effective_swells()`; `surface_height_at`/`shore_surface_height_at` read it (audit amendment);
  `drive_sea_level` holds `sea_level = base_level`, `level_rate = 0` when tides off;
  `sync_water_params` pushes `effective_swells()` to the material; `sync_wetness` writes
  `f32::MIN` when off. Eight `toggle_water_*` commands (shared `flip_extension_flag` helper).
- `integration/`: `CurrentsToggle` + `toggle_currents` (zeroes `character.water.current`);
  `SplashesToggle` + `toggle_splashes` (`run_if` on `emit_character_splashes` only — manual
  `splash` command untouched); `body_foam.rs` sets `body_foam = true` only when the SDF handle
  changes (test proves a console flip survives the next sync); `post_process.rs` reads
  `effective_swells()`; `debug_views.rs` `DebugCurtains.terrain_revision` forces curtain re-drape.
- New Stage 3 tests: regeneration round-trip (revision, chunk convergence, collider re-stamp),
  in-place heightmap rewrite, command behavior (swap/reject/flip/reseed), tides/swells/wetness
  gating, water toggle stickiness across `sync_water_params`, currents-off, splashes-off,
  body-foam stickiness, spawn-height sync, curtain re-drape.

### Deviations from the ticket/plan
- `pub use terrain::Generator` (ticket Stage 1 "re-export `Generator`") removed again: nothing
  outside `terrain` uses it in a non-test build, and the binary crate warns on the unused re-export.
  The type stays reachable as `crate::terrain::terrain::Generator` (all consumers already use that).
- `Terrain::set_config` skipped — commands mutate via `set_generator` / `set_erosion_enabled` +
  `regenerate()`; `world` and `grid_resolution` are immutable after `Terrain::new` (audit fix).
- `regenerate_terrain [seed]` reseeds the `Noise` generator only; `ErosionConfig.seed` is a separate
  knob (toggled via config, not the console).
- `sync_height_map` also refreshes `CoastField` (ticket only said `image.data`) so CPU/GPU metadata
  cannot drift if bounds ever change.
- `HeightGrid::new` gated `#[cfg(test)]` (only test fixtures use it).

## Plan audit

Adversarial audit agent ran `cargo test` (baseline confirmed) and verified the plan against the code.
Verdict: no blockers; implementable with these amendments folded in:

- **Major (swells)**: `SeaState::surface_height_at` and `shore_surface_height_at` must read
  `effective_swells()`, not `self.swells` — callers (`sync_submersion`, `emit_character_splashes`,
  `lens_surface_height`, `submersion_debug_info`) otherwise keep swell motion while "off".
- **Major (stale tasks)**: `attach_finished_chunk_meshes` stamps `chunk.revision = pending.revision`
  (the task's revision, never `terrain.revision()`); the pending dedup guard in `update_chunk_lods`
  must also require `task.revision == terrain.revision()`; `ColliderRevision` inserted at spawn and
  on every rebuild.
- **Minor**: tuple-struct test constructions become `Terrain::new(...)` (`terrain.rs` collider test,
  `chunks.rs` `lod_app`, `currents.rs` `current_at`); `heightmap.rs` `spill_of` test reads grid
  resolution (const deleted).
- **Minor (CPU/GPU mirror)**: use WGSL `mix` form `a*(1-t)+b*t` (not `f32::lerp`), clamp the `+1`
  corner indices like `coast_clamp`, and weight `wy` between iz/iz+1 rows (shader's `south` corner is
  `corner+(1,1)`); the parity test must use asymmetric heights so an axis swap cannot pass.
- **Minor**: `sync_height_map` takes `Option<Res<TerrainHeightMap>>` (resource only exists after
  `setup_water_surface`; several test apps never create it).
- **Minor**: register water toggle commands in `terrain/mod.rs` with bare imported paths so names are
  `terrain>toggle_water_*` (a `water::`-qualified path would yield `terrain>water>…`).
- **Minor**: scale `terrain_gradient`'s step with texel like `ground_normal` (fixed 1.0 < 5.86 m texel
  would make shore gradients blocky).
- **Noted**: `sync_body_sdf` re-forces `body_foam = true` when the SDF handle swaps on re-bake —
  accepted (a re-bake is a legitimate re-enable). `MAX_SURFACE_DEBUG_CELLS = 64` < 2500 chunks means
  the surface overlay warns/disables at default world — pre-existing at 1000 too, not a regression.

## Implementation audit

Adversarial review agent re-ran the suite (195/0 confirmed) and traced the revision protocol,
collider sync, heightmap rewrite, command-name derivation, swell gating, and toggle stickiness.
Verdict: no critical or persistent-state-corruption bugs; the stale-task race cannot persist (a
pending task with a stale revision always fails the dedup guard and is replaced). Findings and
dispositions:

1. **Fixed** — `setup_water_surface` seeded `WaterMaterialExt.swells` from the raw `sea_state.swells`
   instead of `effective_swells()` (latent CPU/GPU parity break if water is ever set up while swells
   are off).
2. **Fixed at the root** — `sync_height_map`'s in-place overwrite assumes `world`/`grid_resolution`
   never change after bake, but the public `config_mut()` made that reachable. `config_mut()` is gone;
   `Terrain` now exposes only `set_generator` / `set_erosion_enabled` (grid metadata is immutable for
   a `Terrain`'s lifetime), so the in-place path can never see a resolution change. The material's
   `coast` CoastField likewise cannot go stale now, since bounds/texel are fixed at construction.
3. **Fixed** — `regenerate_terrain <seed>` on a Crater world silently ignored the seed; the output now
   says "(seed ignored: the crater generator has none)" (test added).
4. **Fixed** — collider detach removed only `Collider`, leaving a stale `ColliderRevision` that broke
   the "every stamp has a collider" invariant; detach now removes both.
5. **Accepted** — while splashes are off, `WaterWakeTracker` state freezes, so re-enabling can emit one
   delayed (not spurious — the straddle transition really did happen while off) impact/exit splash.
   The ticket asked only to skip `emit_character_splashes`.
6. **Accepted** — `body_foam` re-enables when the body SDF handle swaps (re-bake = legitimate
   re-enable; already sanctioned in the plan audit).
7. **By design** — a `regenerate()` landing between `update_chunk_lods` and `attach_finished_chunk_meshes`
   lets one old-revision mesh show for a frame; the stamp-the-task's-revision rule the ticket mandates
   guarantees it self-heals the next frame.

After fixes: `cargo test` → **196 passed, 0 failed, zero warnings**; `module_isolation` and
`crate_isolation` pass; zero comments in all edited files.
