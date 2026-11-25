Here is the updated `refactoring.md` with 6 plans—updating Plans 1–3 to match the exact current state of the codebase, and adding Plans 4–6 to address water/optics synchronization, terrain chunking/collision scaling, and module-isolation plus lock-free scheduling:

# Codebase Refactoring Plans

This document outlines 6 concrete architectural refactorings grounded in the current codebase (`Bevy 0.20.1`, `Avian3d 0.8`, `bevy_enhanced_input 0.27`, `bevy_mod_scripting 0.24`, `edition = "2024"`).

---

# Refactoring Plan 1: Modularization & Locomotion Decoupling of the Character Controller

## 1. Motivation & Current Problems
`src/character.rs` is a ~780-line monolith that bundles five distinct responsibilities into a single file:
1. **Kinematic & Ground Physics**: Avian3d shape-casting (`ShapeHits`), slope walkability angles, slope tangents, and ground snap speeds.
2. **Locomotion State Machine**: Multi-medium classification (`Medium::{Land, Wading, Water}`), transitions (`Standing`, `Walking`, `Running`, `Falling`, `Jumping`, `Wading`, `Floating`, `Swimming`, `Diving`), and hysteresis depth latching (`swim_latch`).
3. **Fluid Simulation**: Buoyancy spring integration (`buoyancy_accel`), quadratic drag (`drag_toward_zero`), and 3D camera-pitch diving (`camera_relative_direction_3d`).
4. **Procedural Presentation**: Visual mesh yaw slerp, dive/swimming pitch tilts (`body_tilt`), procedural floating bob/roll, and first-person vs. third-person visibility switching (`sync_body_visibility`).
5. **Console Commands & Debug Info**: Speed mutations (`set_character_speed`, `increase_character_speed`), respawn logic, and `debug_info!` providers.

Testing sub-algorithms in isolation is cumbersome, `character_physics_move` mixes medium dispatch with state updates, and `setup_character` mixes physical entity construction with `crate::input` bindings.

---

## 2. Target Architecture

Decompose `src/character.rs` into a structured module `src/character/`:

```text
src/character/
  mod.rs              # CharacterPlugin, CharacterSystems, public re-exports, setup_character
  types.rs            # Character, WaterPhysics, SubmersionState, GroundState, MovementState, CameraFacing, CharacterBody
  ground.rs           # Shape-cast probing, ground_touch_distance, ground_gap, is_walkable, slope_tangent_y, character_ground_check
  locomotion/
    mod.rs            # FixedUpdate movement dispatcher (character_physics_move) & MovementInput
    land.rs           # move_on_ground, accelerate_toward, jump_fires, air_gravity_scale
    water.rs          # move_in_water, buoyancy_accel, drag_toward_zero, breach_fires, sprint_scale
    state.rs          # Medium, classify_medium, swim_latch, wading_scale, classify_movement_state, classify_wading_state, classify_water_state
  presentation.rs     # body_tilt, character_rotate_body, sync_body_visibility
  commands.rs         # respawn_character, set_character_speed, increase_character_speed, debug_info! systems
```

---

## 3. Implementation Steps

### Phase 1: Types & Command Extraction
1. **`src/character/types.rs`**:
   - Move `Character`, `WaterPhysics`, `SubmersionState`, `GroundState`, `MovementState`, `CameraFacing`, and `CharacterBody`.
   - Preserve all public constants (`CHARACTER_SPAWN_OFFSET_Y`, `CAPSULE_RADIUS`, `CAPSULE_HEIGHT`, `FEET_OFFSET`, `CAPSULE_TOTAL_HEIGHT`), constructors, helper methods (`is_submerged`, `is_fully_submerged`, `is_swimming`), and `Display` implementations.
2. **`src/character/commands.rs`**:
   - Move `respawn_character`, `with_player_character`, `set_character_speed`, and `increase_character_speed`.
   - Move `debug_movement_state`, `debug_movement_speed`, `debug_water_depth`, and `debug_position`.

### Phase 2: Ground Checking & Geometry Isolation
1. **`src/character/ground.rs`**:
   - Extract pure geometry helpers: `ground_touch_distance`, `ground_gap`, `is_walkable`, `slope_tangent_y`, `slope_snap_speed`.
   - Move `character_ground_check` as a dedicated sensor system reading `&ShapeHits` and writing exclusively to `&mut GroundState`.

### Phase 3: Pure Locomotion Engines
1. **`src/character/locomotion/state.rs`**:
   - Move `Medium`, `classify_medium`, `swim_latch`, `wading_scale`, `classify_movement_state`, `classify_wading_state`, and `classify_water_state`.
2. **`src/character/locomotion/land.rs`**:
   - Move `move_on_ground`, `accelerate_toward`, `jump_fires`, and `air_gravity_scale`.
3. **`src/character/locomotion/water.rs`**:
   - Move `move_in_water`, `buoyancy_accel`, `drag_toward_zero`, `breach_fires`, and `sprint_scale`.
4. **`src/character/locomotion/mod.rs`**:
   - Move `camera_relative_direction`, `camera_relative_direction_3d`, `MovementInput`, and `character_physics_move`.

### Phase 4: Presentation & Public Surface
1. **`src/character/presentation.rs`**:
   - Move `body_tilt`, `character_rotate_body`, and `sync_body_visibility`.
2. **`src/character/mod.rs`**:
   - Assemble `CharacterPlugin` and re-export the identical public API so `src/integration/*` and `src/main.rs` require zero import path changes.

---

## 4. Verification & Constraints
* **Zero Comments**: All created and modified files must contain zero code comments (`CLAUDE.md`).
* **Schedule Separation**: Simulation (`FixedUpdate`) and visual presentation (`Update` / `PostUpdate`) stay strictly separated under `CharacterSystems::Playing`.
* **Unit Testing**: Move unit tests into their corresponding submodules (`ground.rs`, `locomotion/land.rs`, `locomotion/water.rs`, `locomotion/state.rs`, `presentation.rs`) and verify `cargo test --workspace` passes with 100% parity.

---
---

# Refactoring Plan 2: GPU-Accelerated Signed Distance Field (SDF) Compute Pipeline

## 1. Motivation & Current Problems
Currently, signed distance field generation (`src/distance_field/baker.rs`) runs synchronously on the CPU:
* The root `Cargo.toml` retains `mesh_to_sdf = "0.4"` solely for `sdf_from_triangles`, which executes `mesh_to_sdf::generate_grid_sdf` directly on the main thread during `Update` (`bake_distance_fields`).
* For dense meshes (up to `MAX_TEXELS = 4_000_000`), main-thread raycast SDF baking freezes the frame loop for **200–500 ms**, triggering frame spikes (`src/profiler.rs`) on initialization.
* CPU generation then allocates a full `Vec<u8>` copy (`f32_bytes`) and uploads it across PCIe into `Assets<Image>`.
* `crates/bevy_shader_kit` already hosts WESL shader infrastructure and `SdfVolume` (`src/field.rs`), yet lacks a GPU compute pass for baking.

Moving SDF baking to a **GPU compute pipeline** reduces frame freezes to a **1–5 ms GPU pass** and populates the 3D `R32Float` texture directly in VRAM with zero CPU staging.

---

## 2. Target Architecture

Split cleanly across the workspace isolation boundary (`tests/crate_isolation.rs`):

```text
crates/bevy_shader_kit/
  src/
    shaders/sdf/
      bake_compute.wesl   # 3D Compute kernel: triangle unsigned distance + pseudonormal sign
    bake.rs               # GpuSdfBakePlugin, RenderApp compute pipeline, bind groups, dispatch
    field.rs              # SdfVolume, DistanceFieldViewExt, material definitions

src/
  distance_field/
    baker.rs              # Extracts Mesh3d opaque vertex/index buffers, issues GpuBakeSdfRequest
    view.rs               # DistanceFieldView material and visibility synchronization
    mod.rs                # DistanceFieldPlugin, component markers, console commands
  integration/
    body_foam.rs          # Binds baked character SDF to WaterMaterialExt
```

### Component & Data Flow
1. **Gameplay World (`src/distance_field/baker.rs`)**:
   - Collects opaque `Mesh3d` parts and transforms them into source-local space.
   - Allocates the destination 3D `Image` (`TextureDimension::D3`, `TextureFormat::R32Float`) with `TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING`.
   - Inserts `GpuBakeSdfRequest` containing packed `Vec<Vec3>` vertices, `Vec<[u32; 3]>` triangles, target `Handle<Image>`, and `SdfVolume`.
2. **Render App (`crates/bevy_shader_kit/src/bake.rs`)**:
   - Extracts `GpuBakeSdfRequest` (one-shot request, removed after GPU dispatch).
   - Prepares storage buffers (`array<vec4<f32>>` / vertex & index buffers + `SdfVolume` uniform) and binds the `R32Float` 3D image view as `texture_storage_3d<r32float, write>`.
   - Dispatches workgroups in `Render` / `Core3d` prior to material passes:
     ```rust
     compute_pass.dispatch_workgroups(
         res.x.div_ceil(4),
         res.y.div_ceil(4),
         res.z.div_ceil(4),
     );
     ```
3. **Completion**:
   - The 3D texture is immediately sampled in VRAM by `DistanceFieldViewExt.sdf` (`view_mode.wesl`) and `WaterMaterialExt.body_sdf` (`water.wesl` via `body_foam.rs`).

---

## 3. Implementation Steps

### Phase 1: WESL Compute Kernel in `bevy_shader_kit`
1. **`crates/bevy_shader_kit/src/shaders/sdf/bake_compute.wesl`**:
   - Declare bindings:
     - `@binding(0)`: `texture_storage_3d<r32float, write>`
     - `@binding(1)`: `var<uniform> volume: SdfVolume`
     - `@binding(2)`: storage buffer of vertex positions
     - `@binding(3)`: storage buffer of triangle indices + edge/vertex pseudonormals (or ray-parity/winding sign pass)
   - Implement `@compute @workgroup_size(4, 4, 4)`: compute voxel world/local point from `global_invocation_id`, evaluate closest triangle distance, determine sign, and call `textureStore`.

### Phase 2: Compute Pipeline in `bevy_shader_kit`
1. **`crates/bevy_shader_kit/src/bake.rs`**:
   - Implement `GpuSdfBakePlugin` using Bevy `0.20.1` `RenderApp` (`RenderStartup`, `RenderSystems::Prepare`, render graph/node or compute pass system).
   - Register `bake_compute.wesl` via `embedded_asset!`.
   - Clean up completed `GpuBakeSdfRequest` entries once queued so each volume bakes exactly once.
2. Update `crates/bevy_shader_kit/src/lib.rs`:
   - Register `GpuSdfBakePlugin` inside `ShaderKitPlugin` and re-export `GpuBakeSdfRequest`.

### Phase 3: Root Crate Refactor & `mesh_to_sdf` Removal
1. **`Cargo.toml` (root)**:
   - Remove `mesh_to_sdf = "0.4"`.
2. **`src/distance_field/baker.rs`**:
   - Remove `sdf_from_triangles`.
   - Extract `PartGeometry` vertices/indices directly into `GpuBakeSdfRequest`, allocate the uninitialized GPU-writable `Image`, attach `DistanceFieldVolume`, and mark `source.baked = true`.

---

## 4. Verification & Constraints
* **Crate Isolation**: `cargo test --test crate_isolation` must pass (`bevy_shader_kit` has zero workspace dependencies).
* **Zero Comments**: No comments in any new or modified code.
* **Performance Benchmark**: Verify via `src/profiler.rs` (`profiler_report`) that startup/bake frame spikes drop from `>200 ms` to `<5 ms`.
* **Unit Testing**: Preserve `FieldBounds` resolution/budget tests and add GPU compute output validation tests (cube interior negative, exterior positive, smooth gradient).

---
---

# Refactoring Plan 3: Event-Driven, Parallel DOM Runtime in `bevy_script_menus`

## 1. Motivation & Current Problems
`crates/bevy_script_menus` has four core structural bottlenecks:
1. **Exclusive World Locks (`apply_dom_ops` and `build_loaded_menu_doms`)**: Both `apply_dom_ops(world: &mut World)` (`dom/mod.rs`) and `build_loaded_menu_doms(world: &mut World)` (`lifecycle.rs`) require `&mut World`. `apply_dom_ops` runs every frame in `MenuSystems::ApplyDom`, forcing a schedule synchronization barrier during `Update`.
2. **Dual Node Registry & $O(N)$ Pruning**: `DomRuntime` maintains a separate `HashMap<i64, DomNode>` with `Entity::PLACEHOLDER` slots before spawning. `DomOp::Clear` and `DomOp::Remove` set `needs_prune = true`, triggering `prune_dead_nodes` to scan every entry in `DomRuntime.nodes`.
3. **Manual Upward Ancestor Traversal**: `DomEventCtx` (`dom/events.rs`) manually walks `parents.iter_ancestors(target)` in both `find_handler` and `emit` on clicks, hover enter/exit, wheel scrolling, and key presses.
4. **Coarse Hot-Reloading**: `reload_menus_on_asset_change` (`assets.rs`) closes **all** open menus (`while close_top_menu(world) {}`) and reopens them from scratch whenever any single menu's `.html` or `.lua` changes, wiping focus and transient UI state across unaffected menus.

---

## 2. Target Architecture

Convert `bevy_script_menus` into an entity-native, parallel, observer-driven UI runtime:

```text
crates/bevy_script_menus/src/
  dom/
    mod.rs            # DomNodeComponent, DomLookup index, On<Despawn> cleanup, parallel process_dom_ops
    node.rs           # NodeSpec parsing, EntityCommands spawning, styling & observer attachment
    events.rs         # Native Bevy observer bubbling (On<PointerClick/Over/Out>) & dispatch_dom_wheel
  assets.rs           # Per-menu granular hot-reload tracking
  lifecycle.rs        # Parallel build_loaded_menu_doms & MenuRoot lifecycle
  scripting.rs        # Lua `dom`, `menus`, and `clipboard` bindings
```

---

## 3. Implementation Steps

### Phase 1: Entity-Native Storage & $O(1)$ Cleanup
1. **`crates/bevy_script_menus/src/dom/mod.rs`**:
   - Introduce `#[derive(Component)] pub(crate) struct DomNodeMeta { pub id: i64, pub menu_root: Entity, pub html_id: Option<String>, pub classes: Vec<String> }`.
   - Maintain a `DomIndex` resource mapping `i64 -> Entity` and `(Entity, String) -> i64` for $O(1)$ handle and `dom.get_by_id` resolution (removing linear scans in `find_by_html_id`).
   - Register a global `On<Despawn, DomHandle>` observer to remove despawned DOM nodes from `DomIndex` in $O(1)$ time, eliminating `prune_dead_nodes`.

### Phase 2: Parallelization of DOM & Lifecycle Systems
1. **`crates/bevy_script_menus/src/dom/node.rs` & `dom/mod.rs`**:
   - Convert `spawn_node_entity` to accept `&mut Commands` (`EntityCommands`) instead of `&mut World`.
   - Replace `apply_dom_ops(world: &mut World)` with a standard parallel system `process_dom_ops` using `Commands`, `ResMut<DomRuntime>`, `ResMut<DomIndex>`, `Query<(&mut Text)>`, `Query<&mut TextSpan>`, `Query<&mut EditableText>`, `Query<&mut DomHandlers>`, `Query<&mut DomNodeMeta>`, `ResMut<MenuFocusState>`, and `ResMut<InputFocus>`.
2. **`crates/bevy_script_menus/src/lifecycle.rs`**:
   - Replace `build_loaded_menu_doms(world: &mut World)` with a standard query system over `Assets<HtmlAsset>` and `Query<(Entity, &mut MenuRoot)>`, queuing parsed XML subtrees directly into `DomRuntime` for `process_dom_ops` in the same frame.

### Phase 3: Native Observer Event Bubbling
1. **`crates/bevy_script_menus/src/dom/events.rs`**:
   - Rely on Bevy `0.20.1` native `EntityEvent` bubbling (`PointerClick`, `PointerOver`, `PointerOut`) or scoped handler lookup without redundant full ancestor scans when a node carries `DomHandlers`.
   - Cache `menu_root: Entity` directly on `DomNodeMeta` so `DomEventCtx::emit` resolves the owning `MenuRoot` in $O(1)$ without walking `parents.iter_ancestors(target_entity)`.

### Phase 4: Granular Asset Hot-Reloading
1. **`crates/bevy_script_menus/src/assets.rs` & `lifecycle.rs`**:
   - Match modified `AssetId<ScriptAsset>` and `AssetId<HtmlAsset>` handles against individual `(Entity, &MenuRoot)` entries.
   - Reload only the affected `MenuRoot` entity (preserving z-order, open stack position, and sibling menus).

---

## 4. Verification & Constraints
* **Crate Isolation**: `cargo test --test crate_isolation` must pass (`bevy_script_menus` depends on no workspace crates).
* **Zero Comments**: No comments in any new or modified code.
* **Behavioral Parity**: All existing unit tests (`dom/node.rs`, `dom/events.rs`) pass, and `assets/menus/command_palette` + `assets/menus/keyboard` retain identical focus, input, and hot-reload behavior.

---
---

# Refactoring Plan 4: Unified `WaterMedium` State, Swell-Synced Optics & Per-Entity Water Interactions

## 1. Motivation & Current Problems
Water configuration and state synchronization are currently fragmented across `bevy_shader_kit`, `src/terrain/`, and `src/integration/`:
1. **Duplicated Optical Structs & Manual Frame Mirroring**: `WaterParams` (`crates/bevy_shader_kit/src/water.rs`) and `UnderwaterPostProcess` (`crates/bevy_shader_kit/src/underwater.rs`) declare 9 identical fields (`water_height`, `time`, `max_depth`, `phase_g`, `scattering`, `absorption`, `sun_direction`, `sun_illuminance`, `ambient_illuminance`). Every frame, `sync_underwater_post_process` (`src/integration/post_process.rs`) manually copies 7 of those fields from `WaterMaterial.extension.params` into `UnderwaterPostProcess`.
2. **Disconnected Sun & Atmosphere Lighting**: `WaterParams::default()` and `UnderwaterPostProcess::default()` hardcode `sun_direction: Vec3::new(0.707, 0.0, 0.707)` and static illuminance values, completely ignoring the actual `DirectionalLight` spawned in `src/terrain/atmosphere.rs` (`looking_at(Vec3::new(-0.15, -0.05, 0.25), Vec3::Y)`).
3. **Swell Mismatch Between Surface, Post-Process, and Ripples**: `src/terrain/water.rs` defines a private `const SWELLS: SwellSet` and `sync_water_params` pushes `water.translation.y` (flat plane height). Meanwhile:
   - `underwater.wesl` approximates meniscus height with `scrolling_fbm_2d` instead of `SwellSet`, and `lens_submerged` in `post_process.rs` checks flat `water.translation.y` instead of `sea_swell_height`.
   - `emit_character_splashes` (`src/integration/ripples.rs`) checks `straddles` against flat `water.translation.y` instead of swell-displaced surface height.
4. **Single-Entity Ripple State**: `SplashMemory` (`src/integration/ripples.rs`) is a global `Resource` reading `characters.iter().next()`, preventing multiple characters or dynamic props from generating independent wakes and splashes.

---

## 2. Target Architecture

Unify optical parameters and swell evaluation into a single source of truth shared between GPU shaders and CPU gameplay systems:

```text
crates/bevy_shader_kit/src/
  water.rs                  # WaterOptics (shared ShaderType), WaterParams, WaterMaterialExt, SwellSet
  underwater.rs             # UnderwaterPostProcess embedding WaterOptics + SwellSet
  shaders/
    water/optics_types.wesl # Shared WaterOptics WGSL/WESL struct
    water/types.wesl        # WaterParams embedding WaterOptics
    post_process/underwater.wesl  # Uses WaterOptics + swell_offset/height from swells.wesl

src/terrain/
  atmosphere.rs             # Publishes SunLightState / syncs DirectionalLight direction & color
  water.rs                  # SeaState resource (base_level, swells, optics) + sync_sea_state

src/integration/
  post_process.rs           # Reads SeaState + swell height at camera XZ
  ripples.rs                # Per-entity WaterWakeTracker component + swell-aware straddle checks
  submersion.rs             # Reads SeaState instead of WaterHeightSource + sea_swell_height free fn
```

---

## 3. Implementation Steps

### Phase 1: Shared `WaterOptics` in `bevy_shader_kit`
1. **`crates/bevy_shader_kit/src/water.rs` & `shaders/water/optics_types.wesl`**:
   - Extract `#[derive(ShaderType)] pub struct WaterOptics` containing `max_depth`, `phase_g`, `scattering`, `absorption`, `sun_direction`, `sun_illuminance`, and `ambient_illuminance`.
   - Embed `optics: WaterOptics` inside both `WaterParams` and `UnderwaterPostProcess`, and optionally pass `swells: SwellSet` to `UnderwaterPostProcess` so `underwater.wesl` evaluates true Gerstner meniscus height at `lens.xz` via `swells.wesl`.
   - Add `ShaderType` byte-size / layout unit tests matching the WESL structs.

### Phase 2: `SeaState` Resource & Sun Sync in `src/terrain/`
1. **`src/terrain/water.rs`**:
   - Replace the private `const SWELLS` and ad-hoc `WaterHeightSource` transform reads with a `SeaState` resource (`base_level`, `swells: SwellSet`, `optics: WaterOptics`), exposing `SeaState::surface_height_at(x, z, time) -> f32`.
   - Keep `Sea`'s `Transform.translation.y` synchronized from `SeaState.base_level` (so `set_water_level` updates `SeaState.base_level` directly).
2. **`src/terrain/atmosphere.rs`**:
   - Add a system updating `SeaState.optics.sun_direction` and `sun_illuminance` whenever the scene's `DirectionalLight` transform or color/illuminance changes.

### Phase 3: Integration Cleanup & Per-Entity `WaterWakeTracker`
1. **`src/integration/post_process.rs`**:
   - Replace the 7-field manual copy in `sync_underwater_post_process` with a single assignment (`settings.optics = sea_state.optics; settings.swells = sea_state.swells;`).
   - Evaluate `sea_state.surface_height_at(cam.x, cam.z, time)` in `lens_submerged` and `submersion_debug_info`.
2. **`src/integration/ripples.rs`**:
   - Replace the global `SplashMemory` resource with a `#[derive(Component, Default)] struct WaterWakeTracker` attached via observer to `Character` (and any future buoyant bodies).
   - Measure `straddles` and `hull_fraction` against `sea_state.surface_height_at(pos.x, pos.z, now)`.

---

## 4. Verification & Constraints
* **Crate Isolation**: `bevy_shader_kit` remains strictly independent (`tests/crate_isolation.rs`).
* **Zero Comments**: No comments in any new or modified code.
* **Shader Layout Tests**: Verify `WaterOptics::min_size()`, `WaterParams::min_size()`, and `UnderwaterPostProcess::min_size()` match WESL `var<uniform>` layouts; validate `cargo test --workspace`.

---
---

# Refactoring Plan 5: Chunked Async Terrain Generation, Heightfield Collision & 2D Coastal Heightmap

## 1. Motivation & Current Problems
`src/terrain/terrain.rs` and `src/terrain/mod.rs` currently construct the entire world as a single monolithic mesh:
1. **2-Million-Triangle Startup Bottleneck**: `Terrain::default()` configures a `300.0 x 300.0` `Plane3d` with `subdivisions: 1000` (`1,002,001` vertices and `2,000,000` triangles). `setup_terrain` generates that entire mesh AND runs `Collider::trimesh_from_mesh(&terrain_mesh)` synchronously on the main thread during `Startup`.
2. **Expensive Terrain Distance Field Baking**: `src/integration/distance_field.rs` attaches `PrimaryField` and `DistanceFieldSource` onto `TerrainSurface`. Every triangle of the 2M-triangle terrain mesh is iterated during `bake_distance_fields`—wasting a 3D SDF volume (`MAX_TEXELS = 4_000_000`) on what is fundamentally a 2.5D heightfield.
3. **Flat Unbounded Water Mesh**: `src/terrain/water.rs` spawns a single `Plane3d` (`300x300`, `160` subdivisions) with no coastal depth information beyond screen-space `DEPTH_PREPASS`, which fails when terrain is occluded behind waves or viewed at grazing angles.

---

## 2. Target Architecture

Decompose `Terrain` into spatial chunks with decoupled collision, async mesh building, and a GPU 2D heightmap:

```text
src/terrain/
  mod.rs              # TerrainPlugin, chunk lifecycle, TerrainSurface marker
  terrain.rs          # TerrainConfig, TerrainSampler trait/fn, chunked Mesh builder
  chunks.rs           # TerrainChunk, ChunkGrid, AsyncComputeTaskPool mesh tasks
  heightmap.rs        # 2D R32Float TerrainHeightTexture generation for shaders
  water.rs            # WaterSurface + optional TerrainHeightTexture sampling
  atmosphere.rs       # DirectionalLight, SkyBox, TerrainFog
```

### Key Architectural Changes
1. **Spatial Chunk Grid (`TerrainChunk`)**:
   - Split the terrain into e.g. `64x64` world-unit chunks (`64` subdivisions per chunk instead of one `1000`-subdivision monolith), spawning mesh generation tasks on Bevy's `AsyncComputeTaskPool` and inserting finished `Mesh3d` components as tasks complete.
2. **Avian3d `Collider::heightfield` Instead of 2M-Triangle Trimesh**:
   - Replace `Collider::trimesh_from_mesh` with Avian3d's native `Collider::heightfield(heights, scale)` per chunk (or coarse collision grid), drastically cutting physics BVH memory and startup build time while maintaining exact ground contact.
3. **2D `TerrainHeightMap` Texture Instead of 3D Terrain SDF**:
   - Bake a compact 2D `R32Float` height texture (`512x512` or `1024x1024`, ~1–4 MB) from `Terrain::sample_ground_height` for `WaterMaterialExt` coastal depth/shoreline damping, freeing `DistanceFieldSource` / `PrimaryField` to focus exclusively on 3D props and characters.

---

## 3. Implementation Steps

### Phase 1: Chunked Mesh & Heightfield Extraction
1. **`src/terrain/terrain.rs`**:
   - Separate `TerrainConfig` (`world_size`, `chunk_size`, `chunk_subdivisions`, `collider_subdivisions`) from `TerrainSampler` (`sample_ground_height(x, z) -> f32` and analytical/finite-difference normal calculation).
   - Implement `build_chunk_mesh(origin: Vec2, size: f32, subdivisions: u32, sampler: &Terrain)` and `build_chunk_heightfield(origin: Vec2, size: f32, subdivisions: u32, sampler: &Terrain) -> Collider`.

### Phase 2: Async Chunk Task Pipeline
1. **`src/terrain/chunks.rs`**:
   - Spawn `TerrainChunk { coord: IVec2 }` entities at `Startup` with `Collider::heightfield(...)` immediately available for `Character` spawn/physics, and enqueue mesh generation on `AsyncComputeTaskPool`.
   - Poll completed `Task<Mesh>` handles in `Update` and attach `Mesh3d` + `MeshMaterial3d`.

### Phase 3: 2D Heightmap Texture & Distance Field Decoupling
1. **`src/terrain/heightmap.rs`**:
   - Generate a 2D `R32Float` `Image` + world-to-UV bounds uniform representing terrain elevation.
2. **`src/integration/distance_field.rs`**:
   - Remove automatic `DistanceFieldSource` attachment from `TerrainSurface` (or make it an explicit debug-only toggle), eliminating the multi-second 2M-triangle terrain SDF bake on startup.

---

## 4. Verification & Constraints
* **Module Isolation**: `src/terrain/` must not import `src/distance_field/`, `src/character/`, or `src/camera/`.
* **Zero Comments**: No comments in any new or modified code.
* **Startup & Frame Budget**: Verify startup time and `ProfilerState` worst spike drop dramatically (`Terrain` collider + mesh generation no longer stalls frame 0), and run `cargo test --workspace` covering chunk seam continuity and heightfield bounds.

---
---

# Refactoring Plan 6: Feature Module Isolation Enforcement & Lock-Free Debug/Command Scheduling

## 1. Motivation & Current Problems
Two systemic issues affect modularity and frame-time concurrency in `src/`:
1. **Active Violations of `CLAUDE.md` Module Isolation**:
   `CLAUDE.md` specifies that feature modules (`input`, `menus`, `camera`, `terrain`, `state`, `character`) never import one another, and cross-feature wiring lives strictly in `src/integration/`. Currently, five files violate this boundary:
   - `src/character.rs` imports `crate::input::{GameplayContextBundle, GameplayInput, HotkeyLabel, MovementBinding}`.
   - `src/camera.rs` imports `crate::input::InputAppExt`.
   - `src/dev.rs` imports `crate::input::InputAppExt`.
   - `src/terrain/atmosphere.rs` imports `crate::distance_field::DistanceFieldDimmedColor`.
   - `src/terrain/water.rs` imports `crate::distance_field::DistanceFieldViewHidden`.
2. **Per-Frame Exclusive World Lock in `src/dev.rs` & Singleton Command Coupling in `src/commands.rs`**:
   - `DebugOverlayConfig::default()` starts with `enabled: true`. Every single frame in `Update`, `update_debug_overlay_text(world: &mut World)` runs as an exclusive system—mutably borrowing the entire `World`, calling `std::mem::take` on `DebugInfoRegistry::entries`, and running `world.run_system(entry.system_id)` sequentially. This stalls parallel execution across `Update` on every frame.
   - `src/commands.rs` (`handle_run_commands`) queues exclusive `&mut World` closures and mutates global singleton resources (`CommandArgs`, `CommandOutput`) one command at a time.

---

## 2. Target Architecture

Enforce strict unidirectional dependency boundaries and eliminate per-frame exclusive `&mut World` systems:

```text
src/utils/
  registry/
    commands.rs       # CommandRegistry + queued CommandInvocation { args, system_id }
    debug_info.rs     # Parallel DebugInfoSnapshot resource + DebugInfoAppExt registering normal Update systems

src/integration/
  input_wiring.rs     # Registers Character, Camera, and Dev input actions/hotkeys via InputAppExt
  distance_field.rs   # Attaches DistanceFieldDimmedColor to SkyBox & DistanceFieldViewHidden to Sea via observers

src/dev.rs            # Lock-free sync_debug_overlay_text reading Res<DebugInfoSnapshot>
```

---

## 3. Implementation Steps

### Phase 1: Parallel `DebugInfoSnapshot` (Eliminating `world: &mut World` in `src/dev.rs`)
1. **`src/utils/registry/debug_info.rs`**:
   - Replace `SystemId<(), Option<String>>` + `DebugInfoRegistry` exclusive execution with a `DebugInfoSnapshot` resource (`entries: BTreeMap<(&'static str, &'static str), String>`, `dirty: bool`).
   - Update `DebugInfoAppExt::register_debug_info` (and `debug_info!`) to register a standard parallel `Update` system wrapped in a closure that only executes when `DebugInfoEnabled(bool)` is true and writes its `Option<String>` directly into `ResMut<DebugInfoSnapshot>` (no `&mut World` required).
2. **`src/dev.rs`**:
   - Sync `DebugOverlayConfig.enabled` to `DebugInfoEnabled`.
   - Replace `update_debug_overlay_text(world: &mut World)` with a standard query system `sync_debug_overlay_text(config: Res<DebugOverlayConfig>, snapshot: Res<DebugInfoSnapshot>, ...)` running in parallel with the rest of `Update`.

### Phase 2: Decouple `character`, `camera`, `dev`, and `terrain` from Peer Features
1. **`src/integration/input_wiring.rs`**:
   - Move `ToggleFpsOverlay` hotkey registration out of `src/dev.rs` (triggered via message/command or `FpsOverlayConfig` in integration).
   - Move `Look`, `ToggleViewMode`, `GrabCursor`, `ReleaseCursor`, `ZoomCamera` `InputAppExt` wiring out of `src/camera.rs` (exposing camera action messages or public methods/actions) and `Move`, `Sprint`, `AlignToCamera`, `Jump`, `Crouch` input bundle insertion out of `src/character.rs` (`On<Add<Character>>` observer in `src/integration/input_wiring.rs`).
2. **`src/integration/distance_field.rs`**:
   - Attach `DistanceFieldDimmedColor` on `On<Add<SkyBox>>` and `DistanceFieldViewHidden` on `On<Add<Sea>>` inside `src/integration/distance_field.rs`, removing all `crate::distance_field` imports from `src/terrain/`.
3. **Architecture Guard Test (`tests/module_isolation.rs`)**:
   - Add an integration test (mirroring `tests/crate_isolation.rs`) that scans `src/{character,camera,input,terrain,state,distance_field,ripples,dev,screenshot}/` and asserts zero `crate::<peer_feature>` imports exist outside `src/integration/` and `src/main.rs`, and zero `crate::<feature>` imports exist inside `src/utils/`.

### Phase 3: Re-entrant Command Invocation (`src/commands.rs`)
1. **`src/commands.rs` & `src/utils/registry/commands.rs`**:
   - Pass invocation arguments cleanly and capture output per invocation without leaving stale `CommandArgs` state across runs, while keeping `command!` ergonomics intact.

---

## 4. Verification & Constraints
* **Module & Crate Isolation**: Both `cargo test --test crate_isolation` and the new `tests/module_isolation.rs` must pass with zero violations.
* **Zero Comments**: No comments in any new or modified code.
* **Tests**: All existing `commands.rs` and `debug_info.rs` unit tests continue passing; verify `DebugOverlayConfig` toggles cleanly and displays all registered categories without blocking `Update`.

---
---

# Summary of the 6 Refactoring Plans

| Plan                                        | Target Modules                                          | Primary Technical Change                                                                                                                                                          | Major Impact                                                                                                                 |
| :------------------------------------------ | :------------------------------------------------------ | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------------- |
| **1. Locomotion & Physics Decoupling**      | `src/character/`                                        | Splits ~780-line monolith into `types`, `ground`, `locomotion/{state,land,water}`, `presentation`, and `commands`.                                                                | Separates `FixedUpdate` simulation from visual presentation and isolates physics math for unit testing.                      |
| **2. GPU SDF Compute Pipeline**             | `bevy_shader_kit` & `src/distance_field/`               | Replaces CPU `mesh_to_sdf` raycasting with a WESL compute pipeline in `RenderApp`; drops `mesh_to_sdf` dependency.                                                                | Eliminates 200–500 ms CPU frame freezes and PCIe texture uploads by baking 3D SDFs directly in VRAM.                         |
| **3. Event-Driven DOM Runtime**             | `crates/bevy_script_menus/`                             | Replaces `world: &mut World` DOM/lifecycle systems, `HashMap` sweeps, manual ancestor walks, and full reloads with entity-native components, `On<Despawn>`, and per-menu reloads. | Unblocks `Update` schedule concurrency, delivers $O(1)$ node lookup/cleanup, and preserves sibling menu focus on hot-reload. |
| **4. Unified `WaterMedium` & Interactions** | `bevy_shader_kit`, `src/terrain/`, `src/integration/`   | Extracts shared `WaterOptics`, syncs `DirectionalLight` into `SeaState`, evaluates `SwellSet` in `underwater.wesl`, and moves `SplashMemory` to a per-entity component.           | Eliminates manual 7-field frame mirroring, aligns sun/swells across surface and lens, and supports multi-entity wakes.       |
| **5. Chunked Async Terrain & Heightfield**  | `src/terrain/`, `src/integration/distance_field.rs`     | Breaks the `1000`-subdivision (`2M` triangle) monolith into async `TerrainChunk`s, `Collider::heightfield`, and a 2D `R32Float` height texture.                                   | Eliminates multi-second startup mesh/collider/SDF stalls and gives water shaders view-independent coastal depth.             |
| **6. Module Isolation & Lock-Free Debug**   | `src/dev.rs`, `src/utils/registry/`, `src/integration/` | Removes cross-feature imports in `character`, `camera`, `dev`, and `terrain`; replaces `update_debug_overlay_text(world: &mut World)` with parallel `DebugInfoSnapshot` writers.  | Enforces `CLAUDE.md` module isolation via automated test and eliminates the per-frame exclusive `Update` lock.               |