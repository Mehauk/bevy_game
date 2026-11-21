Your suspicion is completely accurate. Looking closely at `src/terrain/`, `TODO-TERRAIN.md` was checked off by implementing minimal CPU stand-ins and unit-tested wrappers while bypassing several core requirements—and introducing a handful of real bugs that tests hid via tautological assertions or small `compact()` configs.

---
- [ ] do this section too
### What to Fix First to Make These Previews Meaningful
0. switch to weighted voronoi for base features
1. **Fix `macro_landform` elevation offset** so `Continent` / `Archipelago` actually produce ~30–60% land above `sea_level = 0.0` (instead of an underwater world).
2. **Skip submerged cells (`height <= sea_level`)** in `find_lakes`, `river_cells`, `river_paths`, and `find_waterfalls`, and render rivers as connected lines / lakes as filled polygons in `preview.rs` instead of `3×3` dots.
3. **Bilinearly interpolate `slope_at` and use bicubic/Catmull-Rom interpolation** (or higher-res preview sampling) so `slope`, `cliff`, and `height` stop showing `32×32` grid creases and blocks.
4. **Fix `cell_border_colour`** (e.g. `max_weight < 0.96` instead of `.all(|w| w > 0.02)`) and project `cell_weights_at` onto perpendicular bisectors so boundaries are clean strips without triple-junction loop holes.
5. **Normalize `orographic` scale in `generate_precipitation`** (it should scale gently with gradient, not multiply by `45.0`) and **add hypsometric + bathymetric sea coloring** to `height` and `season`.
6. **Only generate `*_tiled` previews for `FiniteTorus`**, and test `macro_landform` / `voronoi` / `noise` on `FiniteTorus` with **exact integer periods** so the seam actually disappears.


- [ ] my character doesnt fall anymore even when the terrain beneath me is lower or if i walk off the edge of the map?
- [ ] Add a complete prievew of what the terrain looks like from the top
- [x] The atmosphere shifts are too sudden and too strong, revert to the old one with minor adjusments based on humidity
---

### Key Gaps & Bugs Found in the Current `src/terrain/`

1. **64×64 CPU Splat Bake Destroys PBR Detail (`splat.rs` vs `material.rs`)**
   - Instead of a GPU `MaterialExtension` sampling a `Texture2DArray`, `bake_chunk` downsamples `256×256` tiles repeating every `24m` into a **`64×64` image per `200m` chunk** (~`3.1m` per texel). Normal maps, pebble/grain detail, and micro-roughness are blurred away.
   - `TerrainTextures` (`1024×1024` via `AssetServer`) and `SurfaceTiles` (`256×256` synchronously via `image::open` on `Startup`) load all 27 textures **twice**; `SplatHandles::cell_data` is baked and uploaded to GPU memory and never read by any shader.

2. **Broken Torus Periodicity (`macro_landform.rs`, `preview.rs`, `mod.rs`)**
   - `macro_landform::value_noise` computes `period = (size / cell).round()` for `cell = size * 0.34`, `0.11`, `0.16`, `0.22`—none of which divide `size` evenly, creating **a cliff discontinuity across the torus seam**.
   - `torus_elevation_is_periodic` and `torus_tiling_repeats_without_a_seam` masked this by calling `wrap_xz` / `wrap_to_world` *before* sampling, comparing `f(wrap(-half))` to itself (`f(-half) == f(-half)`).
   - At runtime, `FiniteTorus` only spawns one `[-size/2, size/2]` region of chunks; walking to the edge drops off into void instead of wrapping/tiling visuals and entities.

3. **Bugs in Hydrology, Lakes, and Cell Mapping (`hydrology.rs`, `map.rs`, `water.rs`)**
   - `mark_hydrology` checks `lake_cells.contains(&index)` where `lake_cells` holds **heightfield vertex indices** (`0..dim²`) and `index` is a **Voronoi cell ID** (`0..cell_count`), corrupting cell hydrology/regions.
   - `find_lakes` sets `outlet` only when `filled <= sea_level + FILL_EPSILON`, so **every inland lake above sea level** has `outlet = None` and is misclassified as `Salinity::Hypersaline`.
   - `carve_rivers` is a single-pass subtract (`<= 6m`) + `smooth_once` rather than iterative hydraulic erosion; `river_paths` stops 1 cell short of confluences; `waterfalls` never carve plunge pools or spawn 3D cascades.
   - `lake_planes` spawns a flat **square quad** sized to the lake's max radius (poking out of hillsides/valleys); `river_ribbon_mesh` uses constant width per path instead of scaling with `accumulation`.

4. **Dead / Unwired Systems & Inverted Physics/Climate (`season.rs`, `params.rs`, `mod.rs`)**
   - `seasonal_params`, `SeasonDelta`, `ground_tint`, `prevailing_wind_for_season`, `SeasonClock::blend_to_next`, and `SeaLevelSeason::continuous_level` are only called in tests. Season changes do not shift cell climate, snow accumulation on ground textures, or atmosphere.
   - `rebuild_water_surfaces` passes hardcoded `Temperature::Cold` into `season::is_frozen`, freezing **every ocean and tropical lake on the planet** solid in Winter.
   - `roll_cell_parameters` adds `rain_shaded` to `sun_index` (making rain shadows *cloudier* instead of sunnier); `relax_parameters` averages `Substrate` ordinals (turning `Sand + Volcanic` into `LimestoneKarst`) and never enforces `MAX_NEIGHBOR_STEP` (an unused `pub const`).
   - `sync_terrain_atmosphere` rebuilds and uploads a brand-new `Sphere` `Mesh` on the CPU **every single frame**.
   - `ScalarField::neighbours8` on `Finite` wraps/clamps boundary neighbors back onto themselves (including self-loops).
   - `VoronoiLayout::cell_at` and `cell_weights_at` linearly scan all cells ($O(N)$ per sample) and are called tens of thousands of times on the main thread; `rebuild_terrain` builds meshes, colliders, and splats synchronously.

---

# Detailed Terrain Rework Checklist

## 1. Immediate Bug Fixes & Mathematical Correctness
- [x] **Fix toroidal `macro_landform` noise periodicity**
  - Force macro FBM octave wavelengths to exact integer divisors of `size` (`period = N`, `cell = size / N` per octave) or project macro noise onto the 4D torus `(cos u, sin u, cos v, sin v)` so `f(half - eps) == f(-half + eps)` continuously.
- [x] **Fix vertex-vs-cell index mismatch in `map::mark_hydrology`**
  - Map `lake.cells` (grid vertex indices) through `layout.cell_at(heights.world_of(x, z))` or sample `hydrology.filled - heights > epsilon` at cell centers / vertex fractions instead of comparing vertex indices to Voronoi cell IDs.
- [x] **Fix `ScalarField` finite boundary clamping & self-neighbors**
  - Fix `ScalarField::neighbours8` to skip out-of-bounds neighbors on `Finite` topology instead of clamping `(0, 0)` onto itself multiple times.
  - Fix `ScalarField::spacing` / `world_of` for `Finite` (`size / (dim - 1)` across `[-half, +half]`) vs `FiniteTorus` (`size / dim` across `[-half, +half)`), eliminating the flat clamped strip along `+x`/`+z` edges.
- [x] **Fix `roll_cell_parameters` climate signs and categorical relaxation**
  - Fix `sun_index`: rain-shadowed cells should shift toward *higher* sun exposure (clearer leeward skies), not lower.
  - Remove ordinal averaging of non-ordinal parameters (`Substrate`, `Geothermal`, `Salinity`) in `relax_parameters`; instead propagate/harmonize categorical geology via neighbor majority vote or geological rules, and preserve physical coupling invariants *after* relaxation.
  - Enforce `MAX_NEIGHBOR_STEP` deterministically via iterative constraint propagation / queue-based clamping rather than random `34%` skip passes.
- [x] **Fix `find_lakes` outlet & salinity classification**
  - Compute true depression spill-over saddles (the lowest boundary cell connecting each depression to a lower basin or sea level) and classify `Salinity` from drainage connectivity + evaporation/precipitation balance (`aridity` vs catchment inflow), not `filled <= sea_level`.
- [x] **Fix `river_paths` confluence gaps**
  - Include the first already-claimed downstream confluence vertex at the end of each tributary `Vec<usize>` so tributary ribbons connect seamlessly into main-stem rivers.
- [x] **Fix non-tautological seam & tiling tests**
  - Test torus continuity at `x = +half - eps` vs `x = -half + eps` **without** pre-wrapping coordinates in the test harness, for `macro_elevation_at`, `NoiseEngine`, `VoronoiLayout::cell_weights_at`, `ScalarField::sample_world`, and `preview` pixel deltas (using `abs_diff`, not unsigned subtraction).

---

## 2. Architecture, Modularity & Runtime Performance
- [x] **Decompose `src/terrain/mod.rs` (~920 lines) into focused submodules**
  - `terrain/plugin.rs`: `TerrainPlugin` registration and schedule sets.
  - `terrain/chunks.rs`: chunk lifecycle, LOD selection, async mesh/collider task pool.
  - `terrain/sky.rs`: procedural sky shader, directional sun/moon lighting, atmosphere & underwater systems.
  - `terrain/commands.rs`: console commands (keep `command!` call sites or pass explicit `"terrain>..."` names so `terrain>*` namespace never breaks).
- [x] **Unify texture loading (eliminate double-loading 27 textures)**
  - Remove duplicate `SurfaceTiles` (`image::open` blocking main-thread `Startup` I/O) once GPU texture arrays are wired; validate `Assets<Image>` asynchronously via `AssetServer` load states and build a single `Handle<Image>` `Texture2DArray` (with mipmaps, anisotropy, and `Repeat` sampler) when ready.
- [x] **Eliminate per-frame CPU `SkyBox` mesh regeneration**
  - Replace `sync_terrain_atmosphere`'s per-frame `Sphere` mesh reconstruction with a lightweight custom `Material` uniform update (`zenith`, `horizon`, `sun_dir`, `sun_color`, `fog_density`).
- [x] **Move chunk mesh, collider, and water meshing onto `AsyncComputeTaskPool`**
  - Currently only `TerrainMap::new` is async while `rebuild_terrain` runs `bake_splats`, `build_chunk_mesh`, and `Collider::trimesh_from_mesh` synchronously on the main thread. Dispatch per-chunk `Task<(Mesh, Option<Collider>)>` with a per-frame completion budget (e.g., max 2–4 chunk uploads/frame) to prevent frame spikes.
- [x] **Replace `Mutex::unwrap()` and tighten error handling**
  - Use Bevy `Task<TerrainMap>` + `futures_lite::future::block_on(future::poll_once(&mut task))` instead of `Arc<Mutex<Option<TerrainMap>>>`.
- [x] **Use `Heightfield` collider or matched-resolution trimesh colliders**
  - Evaluate `Collider::heightfield` (supported by `avian3d` via parry3d) per chunk, or align `CHUNK_MESH_STEPS` with `ScalarField` sample spacing so physics colliders don't miss 2m terrain features or float relative to visual LOD meshes.
- [x] **Benchmark realistic default & large worlds in `bench.rs`**
  - Add benchmarks for default `400×400` (`subdivisions: 200`) and high-res `1024×1024` (`subdivisions: 512`) worlds alongside `compact()`, tracking memory footprint and per-stage timings.

---

## 3. Voronoi Layout, Spatial Indexing & Boundary Blending
- [x] **Accelerate `cell_at` and `cell_weights_at` from $O(N)$ to $O(1)$–$O(\log N)$**
  - Build a uniform spatial acceleration grid (or Delaunay/Voronoi walk / small k-d tree) over Voronoi cell centers during `VoronoiLayout::generate`, caching candidate cells per grid bucket (wrap-aware on `FiniteTorus`).
  - Precompute/cache cell IDs & top-K blend weights for the heightfield grid once per layout instead of re-querying all cells across `base`, `noise`, `precipitation`, and `splat` stages.
- [x] **Upgrade Voronoi generator distribution with Lloyd relaxation**
  - Add configurable Lloyd relaxation passes (`1..3` iterations) on the Voronoi generators so cells avoid sliver triangles and degenerate near-zero edges while retaining organic irregularity.
- [x] **Store full Voronoi polygon geometry on `VoronoiLayout`**
  - Extract actual ordered cell polygon vertices and shared edge segment endpoints `(p0, p1)` from `meshless_voronoi` (properly wrapped into canonical space for periodic faces) so debug rendering, boundary checks, and cliff placement use real edges instead of midpoint stubs.
- [x] **Fix boundary blending math at triple junctions & variable widths**
  - Compute signed perpendicular distance to each candidate pair's bisector: $s_{ij}(p) = \frac{\Delta(p, m_{ij}) \cdot \hat{n}_{ij}}{\text{blend\_width}(i, j)}$ rather than raw Euclidean distance differences $d_j - d_i$ (which curve hyperbolically and vary with cell spacing).
  - Verify $C^1$ continuity and partition-of-unity across 3- and 4-cell junctions where adjacent edges have different `blend_width` values (`4m` vs `16m`).

---

## 4. Macro Landforms, Climate Fields & Region Gating
- [x] **Replace algebraic `WorldShape` falloffs with geologically grounded macro generation**
  - Model tectonic plates / crustal age / uplift belts so mountain ranges form coherent chains, trenches border subduction coasts, volcanic arcs track plate boundaries, and continental shelves grade naturally into abyssal plains.
  - Keep `Continent`, `Archipelago`, `IslandChain`, `InlandSea` as distinct macro tectonic/sea-level presets.
- [x] **Compute `BaseGradient` from actual macro relief, not uniform wind direction**
  - Derive cell `BaseGradient` aspect and slope magnitude from the macro heightfield gradient (perturbed slightly per cell) instead of setting every cell's aspect to `-prevailing_wind`.
- [x] **Simulate true wind-march orographic precipitation & rain shadows**
  - Replace the single-point upwind lookup (`-wind * size * 0.12`) with a multi-step moisture-advection march along `prevailing_wind` (wrap-aware on `FiniteTorus`): air picks up moisture over ocean/coastal cells, condenses precipitation on windward slopes (`dot(grad, wind) > 0`), and creates realistic leeward rain shadows behind ranges.
- [x] **Fix `geothermal_intensity` distribution & hotspot clustering**
  - Remap noise output to `[0, 1]` before thresholding (currently `fbm` centers at `0.0` and rarely passes `0.45`), or place discrete fault/hotspot plumes with radial falloff.
- [x] **Implement and surface `Region` `sub_variants`**
  - Select and store a deterministic `sub_variant` per cell (e.g., *atoll* vs *fringing reef*, *salt flat* vs *sand dunes*, *polar ice cap* vs *valley glacier*) gated by fine parameters (`Salinity`, `Substrate`, `SlopeMagnitude`, `Latitude`), and feed it into noise/material modifiers and `cell_info`.
- [x] **Verify 100% `rule_matches` coverage without constant fallback**
  - Audit `REGION_TABLE` constraints against `roll_cell_parameters` outputs across 10,000+ random seeds; track fallback rate in tests and close unreachable gaps (e.g., `RockyCoast` requiring `Steep/Sheer` slope when coastal cells only roll `Flat..Moderate`).

---

## 5. Multi-Scale Noise Algorithms & Surface Shaping
- [x] **Use true 2D noise on `Finite` worlds (and faster periodic noise on `FiniteTorus`)**
  - Avoid evaluating 4D Perlin at `[x, z, 0.0, 0.0]` on `Finite` topologies; use `NoiseFn<f64, 2>` for `Finite` and reserve 4D torus projection `(cos u, sin u, cos v, sin v)` (or lattice-periodic value/gradient noise) for `FiniteTorus`.
- [x] **Fix `Dune` noise orientation & torus frequency**
  - Rotate sample coordinates smoothly by the true `wind` vector (`cos θ, sin θ`) instead of snapping to cardinal X/Z axes.
  - On `FiniteTorus`, avoid breaking periodicity when rotating anisotropic noise by either using periodic directional wavelets / Gabor noise (summing periodic cosines whose wavevectors lie on integer multiples of `TAU / size`) or blending periodic basis angles.
  - Add asymmetric dune profiles (gentle windward stoss slope, steep leeward slip face using sawtooth/skewed sine shaping).
- [x] **Replace hard-stepped `Terraced` noise with smooth-riser geological strata**
  - Replace `floor(raw * N) / N` (which creates 1-cell vertical cliffs) with smooth plateau-and-scarp shaping (`idx + smoothstep(0, riser_frac, frac)`), plus warped strata thickness and cliff-face micro-noise.
- [x] **Fix `Ridged` normalization and add Swiss/Jordan turbulence**
  - Normalize `RidgedMulti` across octaves dynamically instead of hard-clamping at `[-1, 1]` (which flattens alpine peaks into plateaus).
  - Add slope-weighted octave attenuation (erosion-like derivative dampening / "Swiss turbulence") so valleys stay smooth while peaks remain sharp.
- [x] **Upgrade `Cellular` noise to F2 − F1 + multi-octave cracks**
  - Use `ReturnType::Distance2` / `F2 - F1` ridges for polygonal salt pans, mud cracks, and basalt/lava plates instead of simple `F1` circular cones.
- [x] **Drive noise amplitude, frequency, octaves, and warp from `CellParameters` & `Region`**
  - Scale vertical relief and frequency per cell (e.g., `±1.5m` gentle ripples on `TidalFlat`/`CoastalShelf`, `±35m` sharp peaks on `Alpine`/`Peak`, flat-bottomed basins on `Abyssal`), blended smoothly across cell boundaries via `cell_weights_at`.

---

## 6. Real Hydraulic Erosion, Hydrology & Geomorphology
- [x] **Implement iterative hydraulic erosion**
  - Choose and implement either:
    - **Particle-based droplet erosion** (50k–200k deterministic seeded droplets with inertia, water volume, sediment capacity $C = \max(-\Delta h, \text{minSlope}) \cdot v \cdot w \cdot K$, erosion/deposition brushes, evaporation, wrap-aware on `FiniteTorus`), **or**
    - **Multi-step pipe-model shallow-water / stream-power + alluvial deposition** that recomputes flow across iterations rather than a single static carve pass.
- [x] **Upgrade flow routing from arbitrary priority-flood pop order to D-infinity / steepest-descent with depression hierarchy**
  - Use Barnes priority-flood strictly for depression filling / flat-gradient resolution (using subtle gradient away from higher terrain and toward outlets, avoiding `FILL_EPSILON` staircase artifacts).
  - Compute true steepest-descent or **D-infinity (Tarboton)** flow angles for natural meandering accumulation and non-grid-aligned river paths.
- [x] **Upgrade aeolian (wind) erosion & thermal talus relaxation**
  - Run multi-iteration vectorized/checkerboard wind transport with continuous wind direction (bilinear upwind sampling) and angle-of-repose slumping (`30°–35°` on sand, `40°–50°` on scree/rock depending on `Substrate`!).
- [x] **Carve plunge pools and generate 3D waterfall descriptors**
  - At `Waterfall` drops, erode a downstream plunge pool bowl, widen the channel below the lip, and spawn 3D cascade/mist/foam meshes or particle emitters at runtime.
- [x] **Upgrade crater generation**
  - Add irregular polygon/noise-perturbed crater rims, terraced inner walls on large craters, radial ejecta rays, and apply craters *before* final fluvial/thermal passes only where geologically young/arid/volcanic (preserving eroded crater lakes when wet).

---

## 7. GPU PBR Texture Splatting & Material Pipeline (`MaterialExtension`)
- [x] **Build a true GPU `TerrainMaterial` (`MaterialExtension<StandardMaterial>`)**
  - Pack the 9 (or expanded) `SurfaceLayer` textures into three `Texture2DArray`s (`albedo_array` in sRGB, `normal_array` in linear, `orm_array` in linear) with hardware mipmaps and anisotropic filtering—replacing the blurry `64×64` CPU bake.
- [x] **Pass compact per-vertex / per-chunk splat control data to WGSL**
  - Pack top-4 cell/layer indices + weights (or 16-bit/unorm RGBA splat maps + cell ID map) so the fragment shader samples full-resolution PBR tiles at `world_xz / tile_scale`.
- [x] **Implement triplanar cliff projection & cross-boundary cliff preference in WGSL**
  - Compute world-space normal steepness in fragment shader (`1.0 - world_normal.y`, plus normal-map perturbation) and blend from base layer to the **highest contributing cell's** cliff layer using triplanar XY/ZY projection on steep/sheer walls so cliff textures never stretch.
- [x] **Mitigate texture tiling repetition**
  - Implement stochastic texture bombing / hex-tiling or dual-octave UV scale blending (`near_scale` + `far_scale` + world-space macro tint) so large plains/oceans don't show obvious `24m` grid repetition.
- [x] **Support dynamic seasonal & wetness shading in the terrain shader**
  - Pass seasonal uniforms (`snow_height`, `snow_slope_limit`, `season_tint`, `flood_low`, `flood_high`, `sea_level`) to the fragment shader: blend snow onto flat/gentle high/cold surfaces (shedding off sheer cliffs), darken/gloss wet shore bands near `sea_level`, and keep `DebugMaterialMode` (`Textured`, `FlatColor`, `UvGrid`) + `DebugView` working cleanly inside the shader.

---

## 8. Water Architecture, Meshing, Shaders & Physics Integration

### 8.1 Unified Water Model & Spatial Query (`water.rs`, `map.rs`, `utils/`)
- [x] **Define `WaterKind`, `WaterSample`, and `WaterField` / query API**
  - Add `WaterKind { Sea, Lake, River, Waterfall }` and `WaterSample { surface_y: f32, depth: f32, salinity: Salinity, frozen: bool, flow: Vec2, kind: WaterKind }`.
  - Implement `TerrainMap::water_surface_at(&self, x: f32, z: f32) -> Option<WaterSample>` covering ocean-connected sea, basin lakes, and river channels (respecting `WorldTopology::wrap_xz`).
  - Precompute a fast vertex/grid lookup cache (`water_surface: ScalarField`, `water_kind` / `flow_dir` / `salinity` per grid cell, or spatial segment index for rivers) during `TerrainMap::new` and season updates so runtime queries from camera/character/chunk builders are $O(1)$.

### 8.2 System 1 — Base Water (Ocean-Connected Sea Level)
- [x] **Compute an ocean connectivity mask (`ocean_mask`) instead of flooding all `y < sea_level`**
  - Run a flood-fill/BFS over `heights` vertices `<= sea_levels.max()` seeded from map borders (`Finite`) or `Abyssal` / `DeepOcean` cells (`FiniteTorus`), storing `ocean_mask: Vec<bool>` on `TerrainMap`.
  - Gate base sea water and `is_underwater` checks by `ocean_mask` so isolated inland depressions below `0.0` (e.g., dry rift valleys or closed hypersaline basins) remain dry or become inland `Lake`s rather than spawning phantom seawater.
- [x] **Replace the single 4-vertex `sea_plane` with per-chunk subdivided sea meshes**
  - In `src/terrain/water.rs` and `src/terrain/mod.rs`, replace `water::sea_plane(map.size(), map.sea_level())` with `water::sea_chunk_mesh(map, chunk_key, chunk_size, steps)`.
  - Skip spawning sea meshes for chunks that have no `ocean_mask` vertices below `sea_levels.max() + margin`.
  - Subdivide each sea chunk (e.g., `24×24` or matching `CHUNK_MESH_STEPS`), packing water depth (`sea_level - ground_y`) and local freeze/foam weights into vertex attributes so shorelines, depth absorption, and `FiniteTorus` / `Infinite` chunk tiling work seamlessly.
  - Drive seasonal tide height via `SeaLevelSeason::continuous_level(clock.cycle_progress())`, updating sea chunk `Transform.translation.y` (or shader uniform) without rebuilding meshes every frame.

### 8.3 System 2 — Hydrological Water (Lakes, Rivers & Waterfalls)
- [x] **Filter submerged ocean cells out of hydrology feature extraction (`hydrology.rs`)**
  - Skip `ocean_mask` / `heights <= sea_level` vertices in `find_lakes`, `river_cells`, `river_paths`, and `find_waterfalls` so the seafloor never spawns lakes, underwater river ribbons, or submerged waterfalls.
  - Extend river paths 1 step past the first shoreline/lake-surface intersection (`height <= target_surface`) so rivers terminate cleanly inside lakes or the sea instead of stopping short on the bank.
- [x] **Replace bounding-square `lake_planes` with exact basin-clipped lake meshes (`water.rs`)**
  - Triangulate only the `(gx, gz)` grid quads belonging to each `Lake`'s `cells` plus a 1-ring shoreline margin (clamping perimeter vertices slightly below ground or onto the bank at `lake.surface`), eliminating square water corners poking out of hillsides.
  - On `FiniteTorus`, unwrap each lake's vertex coordinates relative to its anchor cell using `topology.delta_xz` so seam-straddling lakes build a continuous mesh (and render in wrapped `3×3` copies).
  - Derive lake `Salinity` from true spill-saddle drainage + catchment aridity (`Freshwater` if `drains_to_sea`, `Brackish` in coastal mixing zones, `Hypersaline` only for closed arid basins).
- [x] **Upgrade `river_ribbon_mesh` to smoothed, variable-width, bank-embedded ribbons (`water.rs`)**
  - Split `river_paths` at `FiniteTorus` wrap jumps (where `delta_xz` diverges from raw coordinate difference) into continuous local segments, or unwrap segment points continuously across seams.
  - Smooth grid staircases using Chaikin corner-cutting or Catmull-Rom splines while sampling/clamping center elevation against `heights` so ribbons follow valleys cleanly without cutting underground through bends.
  - Compute half-width continuously per station from local `accumulation` (`base_width + scale * accumulation.sqrt()`) rather than a constant `river_width(path.len())` across the whole path.
  - Build a 3- or 4-vertex cross-section (flat/slightly crowned water surface across the channel width, plus outer skirt vertices tucked `~0.15m` below the terrain bank) to eliminate floating edges on sloped terrain and z-fighting (`RIBBON_LIFT` tuning).
  - Encode cumulative downstream flow distance in `UV.y` and normalized across-stream `0..1` in `UV.x` for scrolling flow normals and center-to-bank edge feathering.
- [x] **Upgrade `cascade_mesh` & waterfall plunge pools (`hydrology.rs`, `water.rs`)**
  - Use `topology.delta_xz` for `lip -> plunge` orientation so cascades on `FiniteTorus` seams don't stretch across the map.
  - Shape the waterfall sheet with a slight parabolic arc from `lip` down into the carved `plunge` pool (width matched to incoming river width) plus a circular/radial foam apron at `plunge_height + 0.08`.

### 8.4 Per-Cell Freezing & Physics Colliders (`season.rs`, `water.rs`, `mod.rs`)
- [x] **Replace global `Temperature::Cold` freezing with per-cell temperature + salinity + geothermal rules**
  - Remove the hardcoded `season::is_frozen(clock.season, Temperature::Cold)` in `rebuild_water_surfaces`.
  - Evaluate `is_water_frozen(season, temp, salinity, geothermal)` per cell / per lake:
    - `ActiveVolcanic` / `Geothermal` cells never freeze.
    - `Freshwater` freezes at `Cold` or `Freezing` (and in `Winter` unless `Warm`+); `Brackish` freezes at `Freezing` (or `Cold` in `Winter`); `Marine` requires `Freezing`; `Hypersaline` resists freezing except extreme `Freezing` polar winters.
  - Spawn solid `avian3d` colliders **only** for frozen geometry:
    - For frozen sea chunks: triangulate only the frozen cells inside the chunk (or build a `Collider::trimesh_from_mesh` from the frozen sea submesh) rather than spawning a full-map `Collider::cuboid`.
    - For frozen lakes: build `Collider::trimesh_from_mesh` directly from the exact basin-clipped lake mesh instead of `frozen_collider(radius)` (which places an invisible square wall over dry banks).
    - Keep `WaterSurface` mesh/collider rebuilds scoped to `(generation, season)` or discrete freeze-state changes, using `AsyncComputeTaskPool` alongside terrain chunk jobs.

### 8.5 Dedicated `WaterMaterial` Shader (`material.rs` / `water.rs` / `assets/shaders/`)
- [x] **Implement a custom `WaterMaterial` (`Material` or `MaterialExtension<StandardMaterial>`)**
  - Dual-scrolling normal maps: oriented along `flow` vector for rivers, or world-space wind/wave directions for oceans and lakes.
  - Fresnel sky/horizon reflection (reading blended `TerrainAtmosphere` sky colors) + directional sun specular highlight.
  - Beer-Lambert depth absorption blending shallow tint (`CoralReef` turquoise / clear river) into deep navy/abyssal tint using vertex water depth (`surface_y - ground_y`), plus animated shoreline/bank foam where `depth < 0.8m` and turbulent whitewater foam on cascades (`Waterfall`).
  - Render with `cull_mode: None` (double-sided) and subtle underwater surface transmission so looking upward from beneath the water shows the rippled surface.
  - Transition smoothly to a frosty/cracked ice PBR look (`frozen` mask) rather than an all-or-nothing material swap across whole oceans.

### 8.6 Underwater Camera & Character Swimming Integration (`mod.rs`, `utils/`, `integration/`, `character.rs`)
- [x] **Update `sync_underwater_view` to use unified `water_surface_at`**
  - Query `map.water_surface_at(pos.x, pos.z)` instead of only `map.sea_level()` so diving into deep inland lakes or plunge pools activates underwater fog/tint too.
  - Smoothly blend `TerrainFog` and `ClearColor` over a `0.0..1.5m` submersion transition band instead of hard-snapping at `depth > 0.0`.
- [x] **Bridge water immersion to `character` without breaking module isolation (`CLAUDE.md`)**
  - Keep `character` and `terrain` decoupled: publish a kernel state/bridge in `src/utils/world.rs` (or query resource populated by `src/integration/bridges.rs`) such as a `WaterImmersion { submerged: bool, depth: f32, surface_y: f32, flow: Vec2, frozen: bool }` component updated on `Character` entities in `FixedUpdate` / `PreUpdate` (or `avian3d` `Sensor` volumes spawned per water body).
  - In `src/character.rs`, add swim/buoyancy physics when `immersion.submerged && !immersion.frozen && immersion.depth > CAPSULE_HEIGHT * 0.5`:
    - Neutralize fall gravity and apply spring-damped buoyancy toward the surface (`surface_y`), water drag on `LinearVelocity`, and river current push from `immersion.flow`.
    - Map `Jump` to swim upward / tread water and `Crouch` to dive, while preserving normal ground-walking in shallow water (`depth <= 0.5m`).

---

## 9. Seasons, Sky, Lighting & Atmosphere
- [x] **Wire `SeasonDelta` and `SeasonClock` into live world state**
  - Apply `seasonal_params` to cell climate, snow accumulation masks, and `RegionAtmosphereProfile` blending; interpolate `sea_level` continuously across `clock.cycle_progress()` (`SeaLevelSeason::continuous_level`) instead of snapping once per season.
  - Compute `coastal_erosion` across the annual tidal/flood band during generation (since the band itself is static across the year), rather than re-running erosion and rebuilding every trimesh collider on season tick.
- [x] **Upgrade `SkyBox` to an atmospheric sky shader**
  - Implement Rayleigh + Mie sky scattering shader driven by sun direction, horizon haze, zenith/horizon colors from `RegionAtmosphereProfile`, sun disc, and optional procedural cloud layer—with zero CPU mesh rebuilds.
- [x] **Drive `DirectionalLight` and ambient light dynamically**
  - Move `DirectionalLight` sun angle across elevation/azimuth (or seasonal solar declination), calibrate lux/exposure + `GlobalAmbientLight`, and modulate shadow/light color from the blended `TerrainAtmosphere`.
- [x] **Smooth underwater transitions**
  - Blend fog color, exponential absorption visibility, and underwater caustic/tint smoothly across `depth` (`0..2m` transition band) rather than hard-snapping at `depth > 0.0`.

---

## 10. World Topologies, Chunk Streaming & Verification
- [ ] **Make `WorldTopology::FiniteTorus` truly playable end-to-end**
  - Either render a `3×3` wrapped grid of chunk instances around the camera (offset by `±size`) and wrap player/camera positions across `[-size/2, +size/2)` seamlessly in `integration`, or maintain a toroidal chunk ring around the viewer so there is never a visible edge or fall-off cliff.
- [ ] **Implement `WorldTopology::Infinite` chunk streaming & LOD**
  - Replace the `Infinite` stub with chunk-keyed (`IVec2`) deterministic macro + Voronoi + height + hydrology generation on demand around the camera, paired with distance-based LOD (e.g., `2×`/`4×` step decimation or quadtree/CDLOD with skirt/stitching to hide T-junction cracks).
  - Either implement `TerrainSource::StaticAsset` (loading PNG/R16 heightmaps + optional biome/splat masks) or remove the dead enum variant until ready.
- [ ] **Verify `character` & `camera` across edge cases**
  - Add integration tests (in `integration` or headless test harness) verifying: spawn height stays dry (`>= sea_level` or safely above seabed if coastal), camera `ShapeCast` never clips through chunk seams or water, and walking across chunk/torus boundaries maintains continuous ground contact.
- [ ] **Expand `preview.rs` & offscreen GPU checks**
  - Add a **normal/cliff-stretch** preview and a **dual-scale splat material** preview; ensure the `#[ignore]` offscreen 3D test asserts non-trivial texture detail on *close-up* ground and cliff shots (catching low-res `64×64` bakes) and verifies zero seam discontinuity on `FiniteTorus`.

  Every single visual artifact in these 17 images traces directly to concrete bugs in `src/terrain/`. Three global issues affect almost all of them, followed by specific bugs per layer:

### Global Root Causes Visible Across All Images
1. **Everything is `32×32` blocky (`TerrainConfig::compact()`)**: The preview images are `320×320`, but `compact()` generates a `32×32` heightfield (`~3.75m` per cell). Every `10×10` pixel block is sampling one vertex.
2. **Almost the entire generated world is underwater (see the `season` strip)**: In `Spring`, `Summer`, and `Fall`, **100% of the map is below sea level** (solid blue panels), and in `Winter` only **one ~10-pixel island** pokes above water. Nearly every feature you are seeing in `height`, `slope`, `cliff`, and `hydrology` is happening **on the ocean floor**.
3. **Tiling a `Finite` map (`*_tiled`)**: Every `*_tiled` image (plus `tiling`) has hard rectangular grid seams because the exporter ran on a `WorldTopology::Finite` map and stamped it `2×2` anyway (`if name != "tiling" { write(..., true) }`).

---

### Layer-by-Layer Diagnosis

#### 1. `cells` & `cells_tiled` (Voronoi polygons with white ribbons, loops, and pinches)
- **Swirled loops & pinched holes at corners:** `cell_border_colour` in `src/terrain/mod.rs` draws white wherever:
  ```rust
  weights.len() > 1 && weights.iter().all(|(_, weight)| *weight > 0.02)
  ```
  Near a 3-cell junction, a 3rd cell enters `weights` with a tiny weight (e.g. `0.005`, above `1e-6` but below `0.02`). That suddenly makes `.all(|w| w > 0.02)` **false**, punching solid-colored loop holes right inside the white triple-junctions.
- **Curved, uneven borders:** `VoronoiLayout::cell_weights_at` blends using `distances[j] - distances[i]` (which forms hyperbolas rather than straight perpendicular bisector distances) and falls back to `default_blend_width` for non-adjacent candidates, causing the borders to bow and pinch.
- **Floating cyan/red/yellow ticks:** `preview.rs` draws boundary blend markers perpendicular to the boundary normal (`midpoint ± tangent * blend_width / 2`), so they sit *along* the edge rather than showing edge-crossing width. Meanwhile, every cell's yellow gradient tick points in roughly the same direction because `roll_cell_parameters` sets `aspect = -prevailing_wind` for every cell.

#### 2. `height`, `height_tiled`, and `tiling` (Nearly solid dark green with gridded creases)
- **Flat dark-green colors:** `height_colour` uses `normalized_height(y, extent)` centered at `0.0` (`(y / extent) * 0.5 + 0.5`), and `height_layer` has **no water tinting**. Since the whole map is submerged (`-50m .. 0m`), `y / extent` evaluates negative and clamps almost everything into the dark-green `VALLEY_COLOUR`.
- **Grid-aligned creases:** `hillshade` samples a `32×32` **bilinearly interpolated** `ScalarField` at `0.375m` steps. Bilinear interpolation is only $C^0$ continuous—its slope jumps sharply at every grid line (`3.75m`), creating the visible square tile creases across the surface.

#### 3. `slope` & `cliff` (Big `10×10` pixel blocks)
- **Severe pixelation:** `TerrainMap::slope_at(x, z)` snaps `(x, z)` to the nearest grid vertex via `grid_of` without interpolation:
  ```rust
  let (gx, gz) = self.heights.grid_of(Vec2::new(x, z));
  self.heights.gradient(gx, gz).length()
  ```
  So `slope_at` is piecewise-constant per `32×32` cell, turning both `slope` and `cliff` into `10×10` pixel Minecraft blocks.
- **Mostly red/orange slopes:** Because the submerged terrain drops steeply across the `32×32` grid with `amplitude = 14m` on `3.75m` spacing, underwater gradients routinely hit `1.0–1.6`, saturating `slope` to red and `cliff_share` to bright orange across the ocean floor.

#### 4. `precipitation` (Blocky 3-tone posterized blobs)
- **Clipped to extremes:** In `hydrology::generate_precipitation`, the orographic term multiplies the slope dot product by `relief * 0.5` (`90.0 * 0.5 = 45.0`). Any slight `0.02` slope (`~1°`) produces `±0.9`, instantly clamping `precipitation` to the floor (`0.005`, tan) or ceiling (`1.0+`, cobalt blue).
- **Blocky cell edges:** Because it also uses `field.gradient(x, z)` at `32×32` resolution, the result is a blocky `32×32` binary tan/blue checkerboard.

#### 5. `region` & `region_tiled` (Flat sharp polygons with `2×2` seam)
- Unlike `cells`, `region` samples `region_at` (`cell_at` hard nearest-neighbor) with only `4×4 = 16` cells in `compact()`, and `region_tiled` repeats a `Finite` map `2×2`, producing hard vertical/horizontal seams across tile edges.

#### 6. `season` & `season_tiled` (All-blue ocean + 1 green speck in Winter)
- **All-water world:** `WorldShape::Continent` in `macro_landform.rs` subtracts `0.45` (`base * 0.55 + (1 - dist)^2 * 1.1 - 0.45`), which leaves almost the entire `120×120` compact map below `sea_level` (`-1.6m` to `+1.6m`). Only a single peak rises above `-1.6m` in Winter.
- **Flat water:** `season_water_colour` fills every pixel below `sea_level` with a single unshaded color (no bathymetry/depth gradient).
- **Broken `season_tiled`:** Instead of tiling the `120×120` world consistently, it renders the tiny Winter island 4 times inside the 4th panel while leaving the other 3 seasons blank blue squares.

#### 7. `hydrology` & `hydrology_tiled` (Polka-dot grid of blue, cyan, and yellow squares)
This is the clearest bug in the set:
- **The entire ocean was turned into a "lake":** On a `Finite` world, `priority_flood(&heights, SEA_LEVEL = 0.0)` seeds the map border at `heights.max(0.0)`. Because the map is submerged (`< 0.0`), the boundary is raised to `0.0`, which floods the entire interior ocean floor up to `0.0`. Then `depressions` checks `filled - heights > 0.01`, marking **nearly the entire ocean floor as a `Lake`**!
- **Underwater "rivers" and "waterfalls":** `river_paths`, `carve_rivers`, and `find_waterfalls` never check `height > sea_level`, so with `river_threshold = 10.0` (only 10 out of 1,024 grid cells), rivers and waterfalls (yellow dots) are generated across the flooded ocean floor.
- **Polka-dot pattern:** `paint_cells` in `preview.rs` draws a `3×3` pixel square (`dot(..., radius = 1)`) at each `32×32` grid coordinate (`10px` apart). Instead of connected lines or filled lake regions, every lake, river, and waterfall vertex renders as a disconnected `3×3` dot on a `10px` grid.

---

Issues i found that you might have resolved
- [ ] gaps between terrain and water
- [ ] floating water meshes
- [ ] bad river/waterwall water geometry

---

## 11. Two-System Water Architecture & Integration (Supersedes Section 8)

### 11.1 Unified Water Model & Spatial Query (`water.rs`, `map.rs`, `utils/`)
- [x] **Define `WaterKind`, `WaterSample`, and `TerrainMap::water_surface_at(x, z)`**
  - Add `WaterKind { Sea, Lake, River, Waterfall }` and `WaterSample { surface_y: f32, depth: f32, salinity: Salinity, frozen: bool, flow: Vec2, kind: WaterKind }`.
  - Precompute a fast lookup cache (`water_surface`, `water_kind`, `salinity`, `flow`) during generation and season changes so `water_surface_at(x, z)` covers oceans, inland lakes, and rivers in $O(1)$ (wrap-aware via `WorldTopology::wrap_xz`).

### 11.2 Base Water (Ocean-Connected Sea Level)
- [x] **Compute an `ocean_mask` instead of flooding every point below `sea_level`**
  - Flood-fill `heights` vertices `<= sea_levels.max()` from map borders (`Finite`) or `Abyssal` / `DeepOcean` cells (`FiniteTorus`) to build `ocean_mask: Vec<bool>`.
  - Keep isolated inland depressions below `sea_level` dry (or as closed basins/lakes) instead of letting sea water poke through.
- [x] **Replace the single 4-vertex `sea_plane` with per-chunk subdivided sea meshes (`water.rs`, `mod.rs`)**
  - Build `sea_chunk_mesh(map, chunk_key, chunk_size, steps)` only for chunks containing `ocean_mask` vertices below `sea_levels.max() + margin`.
  - Store water depth (`sea_level - ground_y`) and freeze weights per vertex for shoreline foam, depth absorption, and seamless `FiniteTorus` / `Infinite` chunk tiling.
  - Interpolate sea surface height smoothly across `SeaLevelSeason::continuous_level(clock.cycle_progress())` via `Transform.translation.y` / shader uniform without full mesh rebuilds.

### 11.3 Hydrological Water (Lakes, Rivers & Waterfalls)
- [x] **Exclude `ocean_mask` / submerged ocean cells from `hydrology.rs` extraction**
  - Skip submerged ocean cells in `find_lakes`, `river_cells`, `river_paths`, and `find_waterfalls` so the seafloor never spawns lakes, rivers, or cascades.
  - Extend `river_paths` 1 step past the first shoreline or lake intersection (`height <= target_surface`) so rivers terminate cleanly inside lakes and the sea.
- [x] **Replace square `lake_planes` with exact basin-clipped meshes (`water.rs`)**
  - Triangulate only the grid quads in `lake.cells` plus a 1-ring bank margin tucked into the shoreline at `lake.surface` (unwrapped via `topology.delta_xz` on `FiniteTorus`), eliminating square corners sticking out of hillsides.
  - Derive lake `Salinity` from spill-saddle drainage + catchment aridity (`Freshwater` if draining, `Brackish` in coastal mixing zones, `Hypersaline` in closed arid basins).
- [x] **Upgrade `river_ribbon_mesh` to smoothed, variable-width, bank-embedded ribbons (`water.rs`)**
  - Split/unwrap `river_paths` across `FiniteTorus` wrap seams so rivers never stretch across the map.
  - Smooth paths with Chaikin / Catmull-Rom subdivision, scale half-width continuously from `sqrt(accumulation)`, and use a 3–4 vertex cross-section with outer edges tucked `~0.15m` into the banks to prevent floating edges and z-fighting.
  - Encode downstream distance in `UV.y` and across-stream `0..1` in `UV.x` for scrolling flow normals.
- [x] **Upgrade `cascade_mesh` (`water.rs`)**
  - Orient `lip -> plunge` using `topology.delta_xz`, match width to the incoming river, and render a curved cascade sheet into the plunge pool with a foam apron.

### 11.4 Per-Cell Freezing, `WaterMaterial`, Underwater & Character Physics
- [x] **Fix global Winter ocean freeze (`mod.rs`, `season.rs`, `water.rs`)**
  - Remove hardcoded `season::is_frozen(clock.season, Temperature::Cold)` in `rebuild_water_surfaces` (currently `Cold >= Cold` is always `true`, spawning a `565m × 565m` solid `Collider::cuboid` across the entire map and ~82m past the edges in every season).
  - Evaluate freezing per cell / lake from local seasonal `Temperature`, `Salinity`, and `Geothermal` (volcanic/hydrothermal zones stay liquid; freshwater freezes before marine water), building `Collider::trimesh_from_mesh` **only** over frozen sea sub-meshes and frozen basin-clipped lake meshes.
- [ ] **Create a dedicated `WaterMaterial` shader**
  - Dual-scrolling normal maps (wind-aligned on sea/lakes, flow-aligned on rivers), Fresnel sky reflection, Beer-Lambert depth absorption, shoreline/cascade foam (`depth < 0.8m`), and `cull_mode: None` for underwater upward views.
- [x] **Update `sync_underwater_view` & add character swimming (`mod.rs`, `utils/`, `integration/`, `character.rs`)**
  - Query `map.water_surface_at(pos.x, pos.z)` for underwater fog and smooth the transition over a `0.0..1.5m` depth band (covering lakes and deep rivers, not just `sea_level`).
  - Bridge immersion state to `character` via `utils` + `integration` (e.g., `WaterImmersion { submerged, depth, surface_y, flow, frozen }`), applying buoyancy, water drag, river current drift, and swim up/down controls in `character.rs` while keeping `character` and `terrain` decoupled.

---

### 11.5 Flow-Map & Depth-Absorption Techniques (recorded from the two water tutorials)

- [ ] **Bake world-space flow vectors for rivers & cascades (`hydrology.rs`, `water.rs`)**
  - Export 2D XZ flow direction + speed (`RG` = D-infinity blended flow vector x speed, `B` = slope/rapids turbulence) into river ribbon vertex attributes (or a world-space FlowMap texture), modulated across the stream width via `sin(uv.x * PI)` (faster centre, slower banks) and accelerated on steep slopes.
  - Deflect flow vectors outward around terrain banks, rocks, and waterfalls (distance-field/gradient perturbation) so running water parts around obstacles and sheds foam wakes.
- [x] **Implement 3-phase constant-luminosity flow-map sampling for rivers in the water shader (`water_material.wesl`)**
  - Sample water normal/foam noise in **world XZ** (avoiding spline-UV stretching on bends and seams at confluences) using 3 staggered phases (`0.0`, `1/3`, `2/3`) with per-phase UV offsets and subtle time-distortion noise to break up tiling.
  - Weight phases with trapezoidal constant-luminosity envelopes (`clamp((1.0 - abs((phase - 0.5) * 2.0)) * 1.5, 0.0, 1.0)` summed and divided by `2.0`) to eliminate the mid-blend gray pulsing of 2-phase flow maps.
  - Use dual-scrolling world-space panners (angle-corrected normal blending at mismatched scales/speeds) for low-flow lakes/oceans, and drive river rapids/whitewater foam from `length(flow)` + slope steepness via height-lerp contrast masking.
- [ ] **Add single-layer Beer-Lambert depth absorption, caustics & shore ripples (`water_material.wesl`, `gpu.rs`)**
  - Compute a true water column depth, either from a `DepthPrepass` (which also detects rocks, docks and characters in the water) or from the existing vertex-encoded ground height, and absorb per channel so shallow shelves reveal tinted seabed while deep water absorbs toward teal/navy.
  - Pair it with distance-to-shore animated ripples (`sin(depth * freq - time)`) and shoreline foam.

---

## 12. Additional Bugs, Tuning & Missing Systems

### 12.1 Bugs Found in Current Code
- [x] **Remove panicking leftover debug test in `src/terrain/hydrology.rs`**
  - Delete `dbg_tests::dbg_path` (currently fails `cargo test` with an unconditional `panic!("path: {path:?}")`).
- [ ] **Fix character drop-off / spawn height (`character.rs`, `mod.rs`)**
  - Audit `CHARACTER_SPAWN_OFFSET_Y` (`100.5` in `src/character.rs`) and verify downward gravity / `character_ground_check` fall behavior after removing the oversized frozen sea `Collider::cuboid`.
  - Replace blind `map.height_at(0.0, 0.0)` for `SpawnGroundHeight` with a search for the nearest dry (`>= sea_level`), walkable (`slope <= max_slope_angle_rad`) point near `(0, 0)` so `Archipelago` / `InlandSea` seeds don't spawn underwater or on cliffs.
- [x] **Fix `macro_landform` land-to-sea elevation offset (`macro_landform.rs`)**
  - Adjust `Continent` (`- 0.45`) and `Archipelago` (`- 0.35`) elevation baselines so seeds consistently produce ~30–60% land above `sea_level = 0.0`.
- [x] **Normalize orographic precipitation scale (`hydrology.rs`)**
  - Scale `orographic` in `generate_precipitation` gently with local slope instead of multiplying raw gradient by `relief * 0.5` (`45.0`), which clips values to `0.005` / `1.5`.
- [ ] **Interpolate `slope_at` and add smooth field sampling (`map.rs`, `field.rs`)**
  - Bilinearly interpolate `TerrainMap::slope_at(x, z)` instead of snapping to `grid_of`, and add bicubic / Catmull-Rom sampling for smooth hillshading without bilinear grid creases.
- [x] **Fix grazing `DirectionalLight` sun angle (`mod.rs::setup_sky`)**
  - Raise sun elevation above the current ~9.7° (`looking_at(Vec3::new(-0.15, -0.05, 0.25), Vec3::Y)`) and tune `GlobalAmbientLight` so steep slopes and valleys aren't pitch-black.

### 12.2 World Structure, Scaling & Atmosphere
- [ ] **Switch base Voronoi partitioning to weighted Voronoi (`voronoi.rs`, `map.rs`)**
  - Add per-cell power / multiplicative weights driven by macro landforms/regions so continents, basins, and mountain ranges span larger cells while coastal deltas and calderas stay compact.
- [ ] **Align `CHUNK_MESH_STEPS` with `TerrainConfig::subdivisions` (`chunk.rs`, `map.rs`)**
  - Derive chunk mesh steps from `subdivisions` (currently `200` over `400m` = `2.0m`, while `CHUNK_MESH_STEPS = 48` over `200m` = `4.17m`, discarding half the heightfield/erosion detail and desyncing colliders).
- [ ] **Scale droplet erosion & couple fluvial carving (`hydrology.rs`, `map.rs`)**
  - Scale `ErosionConfig::droplet_count` with `dim * dim` (currently `8_000 × 2` on `200×200`) and couple `carve_rivers` across erosion iterations.
- [x] **Tone down and smooth atmosphere transitions (`atmosphere.rs`, `mod.rs`)**
  - Revert extreme per-region fog/ambient shifts (`visibility: 40..200`, `ambient_boost: 0.55..1.14`, harsh `fog_colour` jumps) to a stable baseline with subtle **humidity** (and gentle altitude/temperature) adjustments.
  - Add temporal smoothing (exponential lerp over ~1–2s) to `TerrainAtmosphere`, `TerrainFog`, `ClearColor`, and `DirectionalLight` in `sync_terrain_atmosphere`.
- [ ] **Add `Finite` world edge policy & `TerrainJob` cancellation (`chunk.rs`, `mod.rs`, `map.rs`)**
  - Define finite map edge behavior (oceanic drop-off + perimeter colliders, or out-of-bounds respawn) so players can't walk off into infinite void.
  - Add request-ID cancellation on `TerrainJob` (`Task<TerrainMap>`) and avoid re-running full droplet/wind erosion synchronously when only `set_cell_base_height` or `SeasonClock` changes.

### 12.3 Previews, Debugging & Future Extensibility
- [ ] **Add a complete top-down composite preview & clean up exports (`preview.rs`)**
  - Add a `world` / `composite` preview layer combining region/surface colors, cliffs, seasonal snow/shore overrides, hillshade, bathymetric sea, clipped lakes, connected river lines, and waterfalls.
  - Add bathymetry to `height` / `season`, render connected rivers and filled lakes in `hydrology_layer` (instead of `3×3` dots), and export `*_tiled` previews **only** for `WorldTopology::FiniteTorus`.
- [ ] **Add feature inspection / teleport debug command (`mod.rs` + `integration/`)**
  - Add `terrain>find_feature` / teleport hook (via `utils`) to jump directly to specific regions, lakes, waterfalls, or river mouths during testing.
- [ ] **Expose surface/scatter query API & static heightmap loading (`map.rs`, `utils/`, `docs/terrain.md` §1)**
  - Provide `surface_info_at(x, z)` (region, substrate, slope, moisture, snow depth, water depth, coast/river proximity) + deterministic per-chunk scatter seeds for future foliage/props.
  - Add `TerrainSource::StaticAsset` (R16/PNG heightmap + optional biome mask import) feeding into the chunk/water/collider pipeline.

  ---

## 13. Deep Code Audit — Remaining Bugs, Performance & Polish

### 13.1 Rendering & Material Bugs
- [x] **Fix `debug_view` failing to switch materials (`mod.rs::rebuild_terrain`)**
  - `rebuild_terrain` checks `if generation_changed || material_changed || season_changed` before calling `chunk_material`, skipping material updates when only `view_changed` is true (leaving `terrain>debug_view <mode>` stuck on textured PBR splats instead of switching to vertex-colored shading, and vice versa).
  - Also skip `bake_splats` whenever `view.0 != DebugView::None` or `*mode != DebugMaterialMode::Textured`.
- [x] **Give `uv_grid_material` an actual procedural checkerboard/UV texture (`material.rs`)**
  - Currently `uv_grid_material()` returns a flat sky-blue `StandardMaterial` (`Color::srgb(0.55, 0.85, 1.0)`), so `terrain>debug_material uv_grid` cannot inspect UV layout or tiling distortion.
- [ ] **Wire `SeasonDelta` & `snow_coverage` into live terrain surface shading (`season.rs`, `splat.rs`, `map.rs`)**
  - `SeasonDelta` / `seasonal_params` are currently `#[cfg(test)]`-only, and `splat::plan_texel` only applies `SurfaceLayer::Snow` inside the coastal `flood_zone`. Apply seasonal temperature deltas and `season::snow_coverage` (shedding off sheer cliffs) across high peaks, tundras, and winter landscapes.
  - Stop re-running `coastal_erosion` and rebuilding all terrain trimesh `Collider`s on every discrete `SeasonClock` tick in `TerrainMap::set_season`, since `flood_zone` is computed once across all 4 seasons and the terrain geometry hasn't changed.

### 13.2 Geometry, Noise & Hydrology Edge Cases
- [x] **Fix `ScalarField` finite-edge gradient & snapping artifacts (`field.rs`)**
  - In `ScalarField::gradient(x, z)` on `WorldTopology::Finite`, use one-sided differences divided by `spacing` at `x == 0 | dim - 1` and `z == 0 | dim - 1` instead of clamping `wrap(-1)` onto `0` and dividing by `2.0 * spacing` (which halves slopes along all map borders).
  - Remove or tighten `lattice_axis`'s `1e-4` snap threshold, which introduces tiny flat micro-ledges along every grid line in `sample_world`.
- [ ] **Fix chunk grid overhang when `size` is not divisible by `chunk_size` (`map.rs`, `chunk.rs`)**
  - Clamp `chunk_bounds` / `height_in_chunk` to `[-size/2, +size/2]` so chunks on non-divisible world sizes (e.g. `size = 300.0`, `DEFAULT_CHUNK_SIZE = 200.0`) don't hang past the map edge with stretched clamped terrain.
- [x] **Fix 4D torus domain warping scale in `NoiseEngine::warped` (`noise.rs`)**
  - On `FiniteTorus`, warp the angular `(u, v)` torus coordinates scaled by `size` before projecting to 4D `(cos u, sin u, cos v, sin v)` rather than adding raw world-meter warp offsets in 4D Euclidean space with a tiny fixed `WARP_FREQUENCY = 0.004` (which barely warps compact/medium worlds and distorts the torus radius).
  - Smoothly attenuate octave amplitudes instead of `.round()`ing blended octave counts in `blend_shapes` so octave count doesn't pop across cell boundaries.
- [ ] **Mask `apply_wind_erosion` by climate/substrate & fix `droplet_erosion` / plunge pool ordering (`hydrology.rs`, `map.rs`)**
  - Gate `apply_wind_erosion` by cell `Substrate` (`Sand` / dry regolith), `Wind` strength, aridity, and `height > sea_level` instead of eroding submerged seabed and tropical rainforests at full strength.
  - Make `droplet_erosion` step distance and `erode_radius` / `deposit_radius` scale-invariant in world meters (currently hardcoded in grid cells, so doubling `subdivisions` halves physical gully width and droplet reach).
  - Run `carve_plunge_pools` before final lake/pool extraction (or emit explicit `WaterBody` plunge pools) so carved waterfall basins hold water rather than sitting as dry pits below cascades.

### 13.3 Parameter Gating, Memory & Camera Integration
- [x] **Fix unreachable `Region` rules under `roll_cell_parameters` (`params.rs`, `region.rs`)**
  - `RockyCoast` and `AbyssalTrench` both require `SLOPE_STEEP_SHEER`, while `roll_slope` only produces `Flat..Moderate` for `CoastalShelf`/`SeaLevel` and `Flat..Gentle` for `Abyssal`—making those regions unreachable except via fallback.
  - Add a multi-thousand-seed statistical audit test measuring fallback rate and verifying every one of the 36 `Region` variants can match `rule_matches` through `roll_cell_parameters`.
- [x] **Eliminate per-sample heap allocations in `VoronoiLayout::cell_weights_at` & fix `vertex_cache` (`voronoi.rs`)**
  - Replace heap-allocated `Vec`s in `cell_weights_at` with stack-allocated buffers (`[ (usize, f32); 8 ]` / `ArrayVec` or scratch buffer) to eliminate hundreds of thousands of allocations during splat baking and preview rendering.
  - Replace `OnceLock<(u64, Arc<VertexBlendCache>)>` with a keyed cache/map so querying a second grid resolution doesn't silently bypass caching.
- [x] **Benchmark full `400×400` worlds & `bake_splats` (`bench.rs`)**
  - Add a benchmark profile for `TerrainConfig::default()` (`size: 400.0`, `subdivisions: 200`) and `bake_splats` alongside `compact()`, tracking total chunk upload cost and memory footprint.
- [x] **Exclude water `Sensor` colliders from `CameraRig` occlusion (`camera.rs`, `integration/`)**
  - Assign dedicated `avian3d` `CollisionLayers` to terrain vs water sensors/volumes and filter `CameraRig`'s `ShapeCast` so swimming/diving never snaps the third-person camera onto the water surface.
- [ ] **Add `terrain>world_summary` console command (`mod.rs`)**
  - Output a copy-friendly summary via `CommandOutput` (seed, topology, shape, size, subdivisions, sea level, land %, biome distribution, lake/river/waterfall counts) for quick world inspection and bug reporting.
  
---

## 14. Additional Codebase Bugs, Rendering Fixes & UX Polish

### 14.1 Splatting, Chunk Mesh & Sky Rendering Bugs
- [x] **Fix cross-boundary cliff height comparison in `splat::plan_texel` (`splat.rs`)**
  - `plan_texel` currently compares `cell.base_height` (the small `[-6, +6]` per-cell jitter) instead of `cell.macro_elevation + cell.base_height` when picking the "highest contributing cell" for steep cliff textures—allowing low-elevation cells with positive jitter to steal cliff textures from adjacent alpine peaks.
- [ ] **Apply `RegionMaterialProfile` tint, roughness & reflectance in splat baking (`splat.rs`, `material.rs`)**
  - `bake_chunk` and `splat_material` ignore `profile.tint`, `profile.roughness`, and `profile.reflectance` (`splat_material` hardcodes `base_color: Color::WHITE, perceptual_roughness: 1.0, reflectance: 0.2`), discarding colour/roughness distinction across regions that share the same base `SurfaceLayer` (e.g. `Savanna` vs `TemperateGrassland` vs `TropicalRainforest`).
  - Re-normalize normal vectors after `imageops::resize` in `splat::read_tile`, and generate mipmaps on uploaded `SplatMaps` images to prevent distant aliasing.
- [ ] **Fix `FlatColor` debug mode & quad triangulation in `chunk.rs`**
  - `chunk_material` currently picks a single `RegionMaterialProfile` from the chunk's geometric center for the entire `200×200m` chunk in `FlatColor` mode instead of blending per-vertex region colours.
  - Flip quad diagonals adaptively in `build_chunk_mesh` (splitting along the shorter height diagonal `|h[a] - h[d]| < |h[b] - h[c]|`) to avoid sawtooth diagonal creasing on steep slopes.
  - Move `Friction::ZERO` / `Restitution::ZERO` handling out of `TerrainChunk` (or avoid `CoefficientCombine::Min` zeroing friction for all future physics props/boulders resting on terrain).
- [x] **Disable fog on `SkyBox` (`mod.rs::setup_sky`)**
  - Set `fog_enabled: false` on the `SkyBox` `StandardMaterial` (currently defaults to `true`, so `DistanceFog` with `visibility = 40..200m` completely fogs out the `SKY_RADIUS = 600.0` sphere and erases the zenith-to-horizon gradient).

### 14.2 Spatial Index, Borders & Hydrology Precision Bugs
- [x] **Fix out-of-bounds positive clamping & boundary hash collision in `voronoi.rs`**
  - `SpatialIndex::raw_axis` clamps negative coordinates with `.max(0)` but does not clamp positive overflow to `(per_axis - 1)`, causing `SpatialIndex::nearest` / `cell_at` / `region_at` to return `None` when queried more than one world-width past `+X` or `+Z` on `Finite` maps.
  - Replace `key.0 as u64 * 4096 + key.1 as u64` in `STAGE_BOUNDARY_BLEND` with `(key.0 as u64) << 32 | (key.1 as u64)` so grids with `cell_grid_dim >= 64` don't collide boundary seeds.
- [x] **Fix crater & plunge-pool border clamping bug on `Finite` worlds (`hydrology.rs`)**
  - Both `apply_craters` and `carve_bowl` call `heights.wrap(gx + dx)` inside `for dz in -reach..=reach { for dx in -reach..=reach }` without skipping out-of-bounds indices when `!heights.topology.wraps()`. When a crater or plunge pool touches a `Finite` map edge, border cells get `heights.add(...)` accumulated multiple times into vertical spikes or trenches.
- [x] **Separate flat-plateau BFS tilt from heightfield in `hydrology::priority_flood`**
  - `tilt_toward_outlets` currently adds `TILT_EPSILON = 1e-3` directly onto `routed` elevation per BFS step; on wide basins/high-res grids (`200+` steps), accumulated tilt can exceed real micro-relief and invert flow over low rims. Track flat-routing distance separately or normalize within each flat plateau's elevation gap.
  - Restrict `FiniteTorus` priority-flood seeds to `ocean_mask` cells rather than seeding every inland pit `<= sea_level`.
- [x] **Improve `macro_landform` distance & orientation consistency (`macro_landform.rs`)**
  - Compute `sea_proximity_field` using physical `distance_xz` (meters) from ocean cells instead of Voronoi hop counts (which vary with `cell_grid_dim`), and rotate `WorldShape::IslandChain` ridges by a seeded azimuth rather than always running along X.

### 14.3 Tooling, Console UX & Robustness
- [x] **Wire `CommandOutput` into all `terrain>*` console commands (`mod.rs`)**
  - Commands like `cell_info`, `inspect_point`, `inspect_hydrology`, `benchmark_terrain`, `set_cell_height`, and `export_preview` currently log only via `info!`/`warn!` without calling `CommandOutput::set(...)`, so running them in the `~` command palette shows no output or copy button.
- [ ] **Fix non-tiled `FiniteTorus` edge rendering in `preview.rs`**
  - In `cells_layer`, draw wrapped boundary edges on both sides of the canonical seam even when `tiled == false` so torus boundary lines don't clip off one side of the preview.
- [ ] **Validate `TerrainConfig` invariants & support optional disk caching (`map.rs`)**
  - Clamp/validate `size > 0.0`, `subdivisions >= 2`, `cell_grid_dim >= 1`, and finite erosion parameters in `TerrainConfig` / `TerrainMap::new` to prevent panics or division-by-zero from invalid configs.
  - Add optional binary/cache serialization (keyed by config + seed hash) so expensive high-resolution (`512×512`+) eroded maps can load near-instantly across restarts.

---

## 15. Subtle Pipeline, Shader & Physics Integration Issues

### 15.1 Mesh, Splat Sampling & Visual Quality
- [x] **Fix chunk-border seam bleeding in `splat.rs::wrapped`**
  - `splat::wrapped` sets `ImageAddressMode::Repeat` on per-chunk splat maps (`albedo`, `normal`, `orm`, `cell_data`), causing linear filtering along chunk edges to wrap and bleed pixels from the opposite side of the same `200m` chunk. Switch chunk splat samplers to `ImageAddressMode::ClampToEdge`.
- [ ] **Generate mesh tangents & remove redundant height lookups in `chunk.rs::build_chunk_mesh`**
  - Generate `Mesh::ATTRIBUTE_TANGENT` (or compute analytic UV-aligned tangents) so Bevy's `StandardMaterial` normal maps don't fall back to lower-quality screen-space derivative tangents.
  - Reuse the already-sampled `(steps + 1)²` height grid for interior finite-difference normals instead of calling `map.height_at` four extra times per vertex.
- [ ] **Use log-scale normalization for `DebugView::Flow` & `preview::hydrology_layer` (`colour.rs`, `preview.rs`)**
  - Flow `accumulation` spans orders of magnitude (`1..5` in headwaters vs `5,000+` at river mouths); dividing linearly by `accumulation_field().max()` makes 99% of tributaries invisible. Normalize with `(1.0 + acc).ln() / (1.0 + peak).ln()`.
- [x] **Fix `ambient_boost` targeting `DirectionalLight` instead of `GlobalAmbientLight` (`mod.rs`, `atmosphere.rs`)**
  - `sync_terrain_atmosphere` multiplies `DirectionalLight.illuminance` by `blended.ambient_boost` while leaving `GlobalAmbientLight` untouched, exaggerating lit-vs-shadowed contrast instead of raising ambient fill.

### 15.2 Hydrology, Precipitation & Climate Precision
- [x] **Fix `carve_rivers` bead-pits & global ridge blurring (`hydrology.rs`)**
  - Clamp river incisions so a carved cell never drops below its downstream neighbour's elevation (currently creates stair-step pits along river channels that `find_lakes` turns into strings of tiny bead-lakes).
  - Restrict `smooth_once` inside `carve_rivers` to river/bank cells rather than blurring the entire heightfield (which dulls distant mountain peaks and ridges).
- [x] **Make `find_waterfalls` slope- and spacing-aware (`hydrology.rs`)**
  - Evaluate waterfall detection using both local slope (`drop / horizontal_dist`) and cumulative drop across a short scarp rather than raw 1-cell `min_drop`, which varies by 2× whenever `subdivisions` doubles.
- [x] **Blend `PrecipitationInput` across Voronoi cell weights (`hydrology.rs`)**
  - `generate_precipitation` currently reads `cell_of_vertex` (hard nearest cell) instead of `cache.weights_at`, producing sharp Voronoi polygon steps in base rainfall and rain shadows.
- [x] **Fix `Wind`, `SunExposure`, and post-relaxation coupling in `params.rs`**
  - `roll_cell_parameters` computes `(ctx.prevailing_wind.length() * 1.5) as usize`, which is always `1` because `prevailing_wind` is unit-length; factor in `sea_proximity` and signed exposure jitter so `Wind` varies naturally.
  - Modulate `SunExposure` by `BaseGradient::aspect` (sun-facing vs shaded slopes) alongside latitude and humidity.
  - Re-verify/re-couple `Salinity` and `Hydrology` invariants after `relax_parameters` shifts cell temperature, humidity, or precipitation.

### 15.3 Lifecycle, Startup & Physics Edge Cases
- [ ] **Sync `TerrainMap` season when `TerrainJob` finishes (`mod.rs::run_terrain_job`)**
  - `TerrainMap::new` initializes `season: Season::Summer`. When `run_terrain_job` delivers a regenerated map while `SeasonClock` is set to another season (e.g. `Winter`), `clock.is_changed()` is `false`, leaving `TerrainMap` on `Summer` sea level until the next season tick. Call `map.sync_season_clock(&clock)` on job delivery.
- [ ] **Move initial terrain generation off synchronous `TerrainPlugin::build` (`mod.rs`)**
  - `app.init_resource::<TerrainMap>()` currently blocks main-thread startup before the window appears. Queue initial generation (or build a lightweight placeholder/loading pass) via `TerrainJob` as generation resolution and droplet counts grow.
- [ ] **Harden `character` downhill snapping & torus-wrap interpolation (`character.rs`, `integration/`)**
  - Scale `SLOPE_SNAP_MAX_SPEED` (currently capped at `4.0 m/s`) relative to horizontal speed so sprinting downhill (`12.0 m/s` over steep slopes) keeps ground contact instead of launching off convex crests.
  - When implementing `FiniteTorus` entity wrapping in `integration`, reset or bypass `TransformInterpolation` on the wrap frame so Avian doesn't visually smear the character across the entire world when crossing `±size/2`.

---

https://www.youtube.com/watch?v=KLl3PZeupFM
https://www.youtube.com/watch?v=FvbPnndigL4

both tutorials nail the exact technical divide between **standing water (oceans & lakes)** and **running water (rivers & waterfalls)**:

---

### 1. Standing Water (Oceans & Lakes) vs. Running Water (Rivers & Cascades)

| Aspect                         | Base Water & Lakes (Oceans / Basins)                                                                                                                                                                          | Running Water (Rivers / Waterfalls)                                                                                                                                                             |
| :----------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Texture Motion**             | **Dual-scrolling UV panners** in world XZ (two normal maps at mismatched scales e.g. `1.0` & `1.73`, and non-aligned speeds e.g. `(0.01, 0.08)` & `(-0.02, 0.01)`, blended via `BlendAngleCorrectedNormals`). | **World-space 3-Phase Flow Maps** (sample noise/normals in world XZ so curves never stretch or seam, and push UVs along a local 2D flow vector).                                                |
| **Why Panners Fail on Rivers** | Not an issue when speed/direction is uniform across an ocean/lake.                                                                                                                                            | Panning spline UVs stretches outer bends, pinches inner bends, seams at junctions, and **jumps across timelines** wherever flow speed or direction varies spatially (rapids, curves, banks).    |
| **Blend Phases**               | Continuous loop (no phase reset needed).                                                                                                                                                                      | **3 staggered phases (`0.0`, `0.333`, `0.667`)** with trapezoidal weights (`saturate((1 - abs((p - 0.5)*2)) * 1.5) / 2.0`), avoiding the gray 50/50 contrast-loss pulsing of 2-phase flow maps. |
| **Speed Profile**              | Driven by local wind / gentle swell.                                                                                                                                                                          | Driven by **D-infinity flow direction** + cross-channel profile (`sin(uv.x * PI)` with a small bank floor so center runs fastest) + slope steepness (rapids/waterfalls).                        |
| **Depth & Foam**               | **Beer-Lambert absorption** (`exp(-depth * absorption * murkiness)`), shallow caustics, shore ripples (`sin(dist - time)`), and shallow/wind whitecaps.                                                       | Same Beer-Lambert absorption, plus **flow-magnitude foam** (`length(flow)` + slope driving height-lerped whitewater) and **obstacle deflection / wake foam** around rocks & banks.              |

---

### 2. How This Maps Directly to Our Codebase (`Bevy 0.20.1` + `src/terrain/`)

The best part is you already have the underlying hydrology data needed for this:
1. **Free Flow-Vector Generation (`hydrology.rs`):** `flow_directions` already computes **D-infinity flow angles** (`downstream`, `secondary`, `primary_weight`), and `accumulation` + `heights.gradient` already give channel volume and slope. You can bake a world-space **FlowMap texture** (`RG` = normalized XZ flow vector × speed, `B` = foam/turbulence intensity) or pass world-space `flow_xz` directly in river vertex attributes (`river_ribbon_mesh` already computes `side`/`flow` and `UV.x` in `[0, 1]` across the stream!).
2. **Bevy 0.20 `WaterMaterial` (`ExtendedMaterial<StandardMaterial, WaterExtension>`):**
   - **Depth for Beer-Lambert & Shorelines:** Compute water depth from either Bevy's `DepthPrepass` (`prepass_depth` vs fragment depth, which also detects rocks/docks/characters in the water) or a fallback heightfield/vertex `ground_y` attribute (`surface_y - ground_y`).
   - **Beer-Lambert Absorption:** Absorb red light fastest (`absorption ≈ vec3(0.28, 0.14, 0.12) * murkiness * 0.01`), turning shallow beaches clear/sandy-green and deep ocean dark teal/navy, plus subtle underwater caustics modulated by `depth`.
   - **Standing vs. Running Branch in One Shader:** If `length(flow_vec) < epsilon` (ocean/lake), use dual world-space panned normal maps + wind ripples; if `length(flow_vec) > 0` (rivers/rapids), run the **3-phase world-space flow-map sampler** with time-distortion noise and flow-speed foam masking.

---

---

## 16. Organic Boundaries, Stochastic Tiling & Scatter Improvements

- [ ] **Domain-warp `VoronoiLayout` boundaries (`voronoi.rs`)**
  - Perturb query coordinates (`p + periodic_warp_fbm(p)`) inside `cell_at` and `cell_weights_at` (wrap-aware on `FiniteTorus`) so biome/cell borders curve organically instead of forming straight polygon edges.
  - Pad `SpatialIndex` search cutoff by `max_warp_amplitude` so warped queries never clip neighbouring bucket candidates.
- [ ] **Add perturbed `F1` Worley cliff plateaus & continental shelf curve (`noise.rs`, `macro_landform.rs`)**
  - Add a domain-warped `F1` hashed-cell plateau pass (or cliff/escarpment noise mode) for rugged jagged cliffs alongside existing `F2 - F1` `Cellular` cracks.
  - Shape `macro_landform` near `sea_level` with a coastal beach + shallow continental-shelf curve so shorelines grade gently for Beer-Lambert water shading.
- [x] **Implement triangle-grid Stochastic Texturing in WGSL (`terrain.wgsl`, `water.wgsl`)**
  - Use simplex/triangle-grid 3-sample tiling (`uv_i = uv + hash22(vertex_i)`) with `textureSampleGrad(..., dpdx(uv), dpdy(uv))` to eliminate `24m` tile repetition on terrain plains and ocean water while preserving hardware mipmaps and normal-map contrast.
- [ ] **Use variable-radius Poisson Disc + Perlin clumping for prop/foliage scatter (`scatter` / `utils/`)**
  - Implement deterministic per-chunk **multi-radius Poisson Disc Sampling** (grid cell size = `min_radius`) for trees and rocks, plus Perlin-clumped random scatter for grass/shrubs, gated by `surface_info_at` (slope, elevation, region, moisture, and water depth).

---

## 17. Terrain Hardness & Material-Driven Erosion / Season Resistance

- [ ] **Define ground `hardness` & erodibility properties on `Substrate` (`params.rs`, `map.rs`)**
  - Add `hardness(substrate) -> f32` (`[0..1]`, e.g. `Sand: 0.10`, `SiltClay: 0.25`, `LimestoneKarst: 0.60`, `Volcanic: 0.72`, `RockCrystalline: 0.90`) alongside `repose_tan`.
  - Precompute a blended `hardness: ScalarField` across Voronoi cell weights (optionally modulated by strata/elevation noise so caprock over softer rock forms natural ledges), and track deposited unconsolidated sediment separately from bedrock where eroded material settles.
- [ ] **Scale hydraulic, fluvial & wind erosion by terrain hardness (`hydrology.rs`, `map.rs`)**
  - In `droplet_erosion`, scale `erode_rate` by `(1.0 - hardness)` at each droplet sample (while letting freshly deposited sediment erode more easily than underlying bedrock).
  - In `carve_rivers`, scale channel incision by `(1.0 - hardness)` so rivers carve wide shallow plains in `Sand`/`SiltClay`, cut steep gorges through `LimestoneKarst`, and naturally form waterfall steps/lips where rivers cross from hard bedrock onto softer rock.
  - In `apply_wind_erosion`, gate material transport by both aridity and low hardness so wind mobilizes dry `Sand` / regolith without smoothing away solid `RockCrystalline` or `Volcanic` peaks.
- [ ] **Gate seasonal `coastal_erosion` & shore material overrides by hardness (`season.rs`, `splat.rs`, `map.rs`)**
  - Pass the `hardness` field into `coastal_erosion(&mut heights, &flood_zone, &hardness, strength)` and scale beach flattening by `(1.0 - hardness)` (e.g. strong flattening on `Sand`/`SiltClay` into smooth beaches and tidal flats, near-zero flattening on `RockCrystalline`/`Volcanic` so `RockyCoast` sea cliffs stay steep).
  - Update `ShoreSurface` / `shore_override` and `splat::plan_texel` so hard stony substrates inside `flood_zone` stay wet darkened rock, shingle/gravel, or winter ice rather than blindly turning into `WetSand` / `Mudflat`.
  - Add unit tests verifying that under identical wave/hydraulic passes, `Sand` flattens into a gentle beach while `RockCrystalline` preserves steep cliff slopes and bedrock material.

---

- [x] review both tutorials and record each point for each video below as a new todo.

---

- [ ] end by ensuring cargo run