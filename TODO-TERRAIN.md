# Terrain Generation TODO

Implementation and research roadmap for `docs/terrain.md`, grounded in the current `src/terrain/` (`TerrainMap`, `TerrainPlugin`), `Cargo.toml` (`bevy 0.20.1`, `avian3d 0.8`, `meshless_voronoi 0.7.2`, `noise 0.9.0`), and `assets/textures/` setup.

---

## 1. Research & Technical Spikes
- [ ] Research `meshless_voronoi` 0.7.2 periodic mode (`periodic: true`), `VoronoiCell` neighbor/face geometry APIs, and edge cases on small or heavily jittered grids.
- [ ] Research seamless 2D toroidal noise sampling with the `noise` crate (4D torus projection `(cos u, sin u, cos v, sin v)` vs domain-tiled noise) for artifact-free `FiniteTorus` worlds.
- [ ] Research Voronoi boundary interpolation math that supports per-edge `cell_blend_width` without discontinuities at 3-cell triple junctions (e.g., perpendicular bisector distance vs natural-neighbor / smooth-minimum weights).
- [ ] Research Bevy 0.20 custom material / `MaterialExtension<StandardMaterial>` designs for multi-biome terrain splatting using `Texture2DArray` or texture atlases (including albedo, normal, roughness, AO, and `.exr` textures already used in `assets/textures/mud/`).
- [ ] Research triplanar vs UV-projected cliff texturing in WGSL to prevent texture stretching on steep/sheer slopes and section-boundary cliffs.
- [ ] Research water rendering and physics integration in Bevy 0.20 + `avian3d` 0.8 (water surface shader with depth absorption/foam/wave normals, underwater fog transition, and buoyancy/swimming sensor volumes).
- [ ] Research terrain chunking, LOD, and incremental `Collider::trimesh_from_mesh` / heightfield collider rebuilds so grids larger than `200x200` subdivisions rebuild without frame hitches.
- [ ] Research hydraulic erosion and hydrology solvers suitable for heightfields (particle-based droplet erosion vs shallow-water / D8 / D-infinity flow accumulation) and how flow paths wrap across torus borders.
- [ ] Research depression-filling and basin-extraction algorithms (e.g., priority-flood) to distinguish closed endorheic basins (saline/hypersaline) from draining freshwater lakes and river outlets.
- [ ] Research aeolian (wind) erosion and directional dune-formation algorithms driven by per-cell wind vectors.
- [ ] Research parameter-to-region gating architectures (rule matrix vs constraint-filtered scoring) to guarantee every valid parameter roll resolves cleanly across all 36 regions.
- [ ] Research dynamic atmospheric blending in Bevy 0.20 (`DistanceFog`, `DirectionalLight`, ambient light, and procedural sky shader vs `SkyBox` tinting) across Voronoi cell transitions.
- [ ] Research runtime seasonal transitions to separate static structural terrain data from dynamic seasonal passes (snow/ice masks, sea level, coastal erosion, and conditional collider rebuilds).
- [ ] Research the cleanest image encoding path for headless CPU `cargo test` previews (`image` crate in `[dev-dependencies]` vs dependency-free PNG/PPM writer) and pick a gitignored output directory (e.g., `docs/.compiled/terrain-previews/`).
- [ ] Research Bevy 0.20 headless 3D offscreen rendering (`ScheduleRunnerPlugin` without `WinitPlugin`, `RenderTarget::Image`, `Screenshot::image`, and `save_to_disk`) for capturing real 3D terrain/fog/water screenshots in an automated test.

## 2. Terrain Pipeline Architecture & Tooling
- [ ] Refactor `src/terrain/map.rs` into an explicit staged generation pipeline (`Parameters -> BaseFeatures -> NoiseFeatures -> EvolvingFeatures -> SeasonalFeatures`) with clear intermediate data structs.
- [ ] Introduce a `WorldSeed` / sub-seed derivation scheme via `splitmix64` so each generation stage is deterministic and independent.
- [ ] Define a `TerrainSource` abstraction (`Generated` vs future `StaticAsset`) so static world asset loading can plug in cleanly later.
- [ ] Refactor single-mesh generation in `src/terrain/mod.rs` to support chunked terrain meshes and chunked `avian3d` colliders.
- [ ] Move heavy terrain generation and mesh/collider construction onto `AsyncComputeTaskPool` (or staged chunk tasks) to prevent main-thread spikes on regeneration.
- [ ] Add terrain debug visualization modes toggled via console commands (Voronoi cell borders/IDs, region map, slope angle, precipitation, flow accumulation, seasonal flood zone).
- [ ] Add/expand terrain console commands (`set_world_topology`, `set_world_seed`, `regenerate_terrain`, `inspect_point <x> <z>`, `set_season <season>`) in `src/terrain/mod.rs`.

## 3. World Generation Types (`Finite`, `FiniteTorus`, `Infinite` Prep)
- [ ] Define `WorldTopology` enum (`Finite`, `FiniteTorus`, and an `Infinite` stub/interface) on `TerrainMap`.
- [ ] Implement canonical coordinate wrapping and shortest-distance helpers (`wrap_xz`, `delta_xz`, `distance_xz`) used uniformly across Voronoi, noise, erosion, and meshing.
- [ ] Enable `periodic = true` in `VoronoiIntegrator::build` when `WorldTopology::FiniteTorus` is active and verify cell wrap-around behavior.
- [ ] Update `cell_at`, `cell_base_height_at`, and neighbor queries to compute distances via `distance_xz` for seamless torus wrapping.
- [ ] Implement 4D-torus periodic noise sampling for `FiniteTorus` so noise features tile across opposite map boundaries.
- [ ] Stitch border vertex heights and compute wrap-aware vertex normals across opposite edges of `FiniteTorus` meshes to eliminate lighting and collider seams.
- [ ] Implement clean `Finite` world edge boundary behavior (oceanic perimeter falloff or clamped erosion/flow boundaries).
- [ ] Design chunk-keyed coordinate indexing (`IVec2` chunk coords + deterministic per-chunk Voronoi jitter) so future `Infinite` streaming can generate cells on demand.

## 4. Core — Parameters & Region Gating
- [ ] Define climate and environmental parameter enums in a `terrain::params` module: `Temperature` (7 levels), `Humidity` (5), `Wind` (4), `SunExposure` (5), and `Precipitation` (5).
- [ ] Define landform and geological parameter types: `BaseHeightBand` (8 levels), `SlopeMagnitude` (5) + `BaseGradient { slope: SlopeMagnitude, aspect: Vec2 }`, `Hydrology` (5), `Substrate` (5), `Geothermal` (3), and `Salinity` (4).
- [ ] Define the complete `Region` enum covering all 36 regions across the 5 elevation tiers (Submerged: 8, Coastal & Sea Level: 6, Lowland: 11, Highland: 6, Alpine & Peak: 5) plus sub-variant metadata where specified.
- [ ] Create `CellParameters` holding all 11 parameters plus the resolved `Region`, and store it on `TerrainCell`.
- [ ] Encode the parameter-to-region gating matrix covering hard eligibility constraints (elevation band, temperature, humidity, precipitation, hydrology, substrate, geothermal, salinity, slope, wind, sun) for all 36 regions.
- [ ] Implement deterministic `resolve_region(params: &CellParameters) -> Region` with tie-breaking and safe fallback rules so no parameter combination ever fails to resolve.
- [ ] Implement physical parameter-coupling rules during cell rolling (e.g., elevation cooling `Temperature` at Highland/Alpine/Peak; `Salinity` constrained to Marine/Brackish in submerged/coastal bands; geothermal heat overrides).
- [ ] Add macro-climate fields (latitude temperature curve, prevailing wind vector, rain-shadow factor, geothermal hotspot masks) to guide parameter generation coherently.
- [ ] Expand `cell_info` console command output to print a cell's full parameter set, rolled `Region`, and adjacent cell IDs.
- [ ] Add unit tests verifying that all 36 `Region` variants are reachable and that `resolve_region` never breaks any gating rule.

## 5. Base Features — Sea Level, Macro Landforms, Voronoi & Gradient
- [ ] Define `SeaLevel` (default `0.0`, modulated by seasonal offsets) and treat all terrain below current sea level as submerged.
- [ ] Spawn and sync a sea-level water surface mesh (`WaterPlane`) with translucent PBR or custom water rendering.
- [ ] Add underwater detection and hooks (depth below sea level drives underwater fog/tint and exposes water state for character/camera integration).
- [ ] Implement macro-landform generators (`WorldShape`: Continent, Archipelago, Island Chain, Inland Sea) using low-frequency fields to assign `BaseHeightBand` and macro elevation before cell perturbation.
- [ ] Extract exact Voronoi cell adjacency (neighbor IDs and shared-edge bisectors) from `meshless_voronoi` cells instead of scanning all cells per lookup.
- [ ] Implement neighbor-constrained parameter rolling (e.g., BFS/relaxation from seed cells bounding parameter step differences between neighbors) so adjacent sections transition naturally without jarring jumps.
- [ ] Replace global `cell_blend_width` with a per-boundary `BoundaryBlend` table (`(cell_a, cell_b) -> blend_width`) rolled deterministically per shared Voronoi edge.
- [ ] Implement multi-cell spatial blending weights `cell_weights_at(x, z)` that honor each boundary's `cell_blend_width` and blend smoothly around triple-junctions.
- [ ] Apply `BaseGradient` plane tilting (`height += dot(delta, aspect) * slope_tan(slope)`) per Voronoi cell, interpolated via `cell_weights_at`.
- [ ] Combine macro `BaseHeightBand` elevation, cell base height, and `BaseGradient` tilt cleanly inside `cell_base_height_at`.
- [ ] Write unit tests for Voronoi adjacency extraction, neighbor parameter smoothness bounds, per-boundary `cell_blend_width` symmetry, and `BaseGradient` plane math.

## 6. Base Features — Region Materials, Cliff Texturing & Atmosphere
- [ ] Audit existing PBR assets in `assets/textures/{ground,mud,rock,rocks}` (including `.exr` normal/roughness handling) and define texture slots for all substrate/region materials.
- [ ] Define `RegionMaterialProfile` mapping each of the 36 `Region`s (plus substrate modifiers) to a base color/texture and cliff color/texture, roughness, and reflectance.
- [ ] Build a custom Bevy `TerrainMaterial` / `MaterialExtension` supporting multi-layer PBR texture blending, triplanar cliff projection, and vertex/lookup-texture cell data.
- [ ] Pack per-vertex or data-texture cell weights, dominant cell ID, higher-neighbor cliff ID, and steepness factors for shader consumption.
- [ ] Implement low-angle base color/texture blending across section boundaries using each boundary's `cell_blend_width`.
- [ ] Compute terrain steepness from surface normals/gradients and blend from base material to cliff material at steep angles.
- [ ] Implement cross-boundary cliff preference so steep transitions across cell boundaries select the higher section's cliff color/texture.
- [ ] Define `RegionAtmosphereProfile` (fog color, visibility range, directional light tint/exponent, sky tint) per region/climate.
- [ ] Implement `sync_terrain_atmosphere` to blend `TerrainFog`, `DirectionalLight`, and `SkyBox` settings smoothly between neighboring sections using the camera/player's Voronoi blend weights.
- [ ] Upgrade `SkyBox` from a flat unlit color to a gradient/atmospheric sky material synchronized with the blended section atmosphere.

## 7. Noise Features
- [ ] Define a `NoiseAlgorithm` enum and `NoiseSampler` abstraction (`sample(x, z, topology, wind_dir) -> f32`) supporting frequency, amplitude, octaves, and domain warping.
- [ ] Port the existing `Fbm<Perlin>` + swell implementation into the `Hilly` noise algorithm.
- [ ] Implement `RidgedMultifractal` noise for alpine ridges, volcanic peaks, and mountain ranges.
- [ ] Implement anisotropic `Dune` noise aligned to local `Wind` direction and magnitude for sand dunes, cold deserts, and snowfields.
- [ ] Implement `Terraced` / stepped plateau noise for plateaus, mesas, badlands, and karst terrain.
- [ ] Implement `Billow` / mound noise for moorlands, wetlands, grasslands, and seabed kelp/coral formations.
- [ ] Implement `Cellular` / Voronoi-crack noise for volcanic lava fields, salt pans, dried mudflats, and scree/talus slopes.
- [ ] Implement parameter-based noise selection and cross-boundary noise blending so terrain transitions smoothly between different noise types across cell boundaries.
- [ ] Update the noise types list in `docs/terrain.md` under `Noise Features` (`<!-- AI markdown exception -->`) as each noise algorithm is added.

## 8. Evolving Features (Precipitation, Erosion, Rivers, Lakes, Wind, Craters)
- [ ] Generate a 2D `PrecipitationMap` grid by interpolating cell `Precipitation` values and applying orographic lift / rain-shadow modulation from terrain slope and `Wind` direction.
- [ ] Implement priority-flood pit-filling and depression detection on the heightfield (respecting `WorldTopology` wrapping) to identify natural basins and lakes.
- [ ] Implement D8 / D-infinity flow-direction and flow-accumulation solver driven by `PrecipitationMap` to trace rivers from headwaters to lakes or sea level.
- [ ] Implement hydraulic erosion (stream-power / fluvial carving and thermal talus relaxation) along high-accumulation paths to carve river channels, gorges, and floodplains.
- [ ] Detect steep drops along accumulated river paths to mark waterfall locations, carve plunge pools, and emit waterfall render/FX descriptors.
- [ ] Generate river ribbon meshes and inland lake water-plane meshes at resolved basin elevations (preserving `Salinity`: freshwater vs closed hypersaline basins).
- [ ] Update cell/vertex `Hydrology` and wetness/substrate masks post-erosion (marking fluvial channels, poorly drained floodplains, and flooded basins).
- [ ] Implement aeolian (wind) erosion and deposition pass driven by interpolated cell `Wind` vectors (smoothing windward slopes, depositing leeward sediment/dunes).
- [ ] Implement crater impact generation (deterministic placement, parabolic bowl, raised ejecta rim, radial perturbation) gated by substrate/region.
- [ ] Add console commands to inspect precipitation, flow accumulation, river paths, and lake basins at any `(x, z)` coordinate.
- [ ] Write unit tests verifying flow-accumulation conservation on sloped planes, torus-wrapping flow continuity, and basin water-level monotonicity.

## 9. Seasonal Features
- [ ] Define `Season` enum (`Spring`, `Summer`, `Fall`, `Winter`) and a `SeasonClock` resource tracking current season, normalized cycle progress, and cycle speed.
- [ ] Define per-season parameter deltas (`SeasonDelta`: temperature, humidity, precipitation, wind, sun exposure) per region/climate tier.
- [ ] Define per-season `sea_level` values and smooth interpolation across the seasonal cycle.
- [ ] Identify all terrain vertices inside the seasonal flood zone (`min_seasonal_sea_level <= height <= max_seasonal_sea_level`).
- [ ] Implement coastal erosion on seasonal flood-zone vertices: interpolate heights into a smooth beach/tidal-flat gradient toward the water and soften steep cliffs in the flood band.
- [ ] Override biome material/texture within the seasonal flood zone (e.g., wet sand, mudflat, salt pan, or frozen shore) with smooth blending at the upper boundary.
- [ ] Implement dynamic runtime seasonal visual updates (snowline/ice coverage, foliage/ground tint shifts, sea-level mesh height, and atmospheric fog/light deltas).
- [ ] Handle seasonal phase transitions affecting physics/gameplay (e.g., freezing water surfaces into walkable sea ice / frozen lakes in winter; refreshing `avian3d` colliders when solid/liquid transitions occur).
- [ ] Write unit tests verifying seasonal parameter delta clamping, seasonal flood-zone vertex classification, coastal gradient monotonicity, and material overrides.

## 10. Validation, Performance & Verification
- [ ] Benchmark full terrain generation, erosion, and mesh/collider rebuild stages using `ProfilerPlugin` and establish frame/load time budgets.
- [ ] Add golden-seed deterministic regression tests for both `Finite` and `FiniteTorus` topologies across all pipeline stages.
- [ ] Verify character spawning (`SpawnGroundHeight`), max-slope movement (`Character::max_slope_angle_rad`), and camera collision (`CameraRig`) work cleanly across cliffs, water planes, and torus seams.
- [ ] Verify all existing and newly added terrain unit/integration tests pass with `cargo test` and `cargo check --all-targets`.
- [ ] Verify all terrain code follows `CLAUDE.md` rules (no code comments, decoupled from other feature plugins except through `utils` and `integration`, and all terrain commands remain under `terrain>*`).
- [ ] Add a CPU-only `cargo test` visual-export suite that renders top-down PNG previews for:
  - [ ] Shaded relief heightmap (hillshade + elevation hypsometric tint).
  - [ ] Voronoi cell layout, centers, `BaseGradient` tilt arrows, and per-boundary `cell_blend_width` borders.
  - [ ] Biome/`Region` color map and steep-slope cliff mask.
  - [ ] `PrecipitationMap`, flow accumulation, river networks, waterfalls, and lake basins.
  - [ ] Seasonal comparison strip (`Spring`, `Summer`, `Fall`, `Winter`) highlighting sea-level shifts and the seasonal flood zone.
  - [ ] A `2x2` tiled `FiniteTorus` render to visually verify seamless boundary wrapping across all layers.
- [ ] Add an offscreen 3D headless render test (using `RenderTarget::Image` + `Screenshot::image` + `save_to_disk`, marked `#[ignore]` for GPU-optional CI) that captures multi-angle 3D snapshots (overview, boundary cliff close-up, river/coastline, and seasonal winter/fog view) for AI visual inspection.
- [ ] Add a `terrain>export_preview [layer]` console command (optional helper) so the same diagnostic renderer can dump maps on demand from a live session.