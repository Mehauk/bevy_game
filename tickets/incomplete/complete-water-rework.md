## Repo-Specific Plan: Port Crest `4.23.0` into This Codebase (Bevy `0.20.1`)
We have implemented custom water to varying degrees of success. our custom implementation is buggy and lacking many features. I would like to completely do away with our water system in favor of slotting in a completely new system. This system can be found at https://github.com/wave-harmonic/crest. It will be your job to port this with as much integrity as possible. 


### Target Workspace Layout

```text
crates/
  bevy_shader_kit/          # Keeps shared non-water tech (bake, field, debug, post_process, wet, compose)
                            # Legacy water.rs, fft.rs, underwater.rs, shaders/water/* deleted in Phase 5
subcrates/
  crest_water/              # New Crest 4.23.0 Bevy 0.20.1 plugin (depends only on bevy_shader_kit)
  world_water/              # Legacy water — kept during Phases 1-4 for comparison, deleted in Phase 5
harness/
  water_harness/            # New workspace member outside crates/ & subcrates/ (never imports world_generation)
tools/
  unity_crest_goldens/      # Batchmode Unity Crest 4.23.0 golden exporter
```

Why this layout passes all existing tests:
- `crates/bevy_shader_kit` has zero workspace deps → `tests/crate_isolation.rs` passes.
- `subcrates/crest_water` depends only on `crates/bevy_shader_kit` (and external `bevy` / `avian3d`) → `tests/subcrate_isolation.rs` passes.
- `harness/water_harness` sits outside `crates/` and `subcrates/` and never depends on `world_generation` (`src/`).
- `src/integration/` wires `crest_water` to `character`, `terrain`, `camera`, `distance_field`, `screenshot`, and `commands` → `tests/module_isolation.rs` passes.

---

### Phase 1: Standalone Harness & Golden Pipeline (`tools/unity_crest_goldens` + `harness/water_harness`)

1. **Workspace Registration (`Cargo.toml`):**
   - Add `"harness/water_harness"` to `[workspace] members`.
   - Depend on `crest_water`, `bevy_shader_kit`, `avian3d`, `bevy` (`0.20.1`), and temporarily `world_water` (until Phase 5 cleanup). Never depend on `world_generation`.

2. **Unity Crest `4.23.0` Reference Generator (`tools/unity_crest_goldens`):**
   - Batchmode Unity scene pinned to `wave-harmonic/crest` tag `4.23.0` (`-batchmode -nographics`, fixed `Time.fixedDeltaTime`, fixed RNG seed).
   - Commit the generated golden fixtures (so CI runs without requiring Unity installed) for 6 scenarios:
     - `calm_ocean` (open deep waves)
     - `storm_swell` (FFT + Gerstner + whitecap foam)
     - `shallow_shore` (sloped seabed depth attenuation + shoreline foam + caustics)
     - `spline_river` (sloped `Spline` height + directional `Flow` feeding into ocean)
     - `boat_wakes` (`BoatProbes` + `SphereWaterInteraction` driven at fixed thrust over `t = 0..10s`)
     - `underwater_curtain` (camera straddling & submerged)
   - Export per preset: `(x, z, t)` height/displacement/normal/flow tables, raw `Texture2DArray` cascade slices, boat transform trajectories, and rendered reference PNGs.

3. **Bevy Standalone Harness (`harness/water_harness`):**
   - **Headless CI Mode (`cargo test -p water_harness`):**
     - Uses `TimeUpdateStrategy::ManualDuration`, offscreen `Image` render target, and `bevy::render::gpu_readback::Readback` (`ReadbackComplete`) to extract cascade slices, query outputs, and rendered frames.
     - Runs **both** `crest_water` (comparing against Unity Crest `4.23.0` goldens after LH↔RH coordinate conversion `z_bevy = -z_unity`) **and** `world_water` on shared baseline probes (sloped shore height, deep-water query, render sanity) during migration.
   - **Interactive Sandbox Mode (`cargo run -p water_harness`):**
     - Minimal testbed scene (sloped shore + spline river + `avian3d` example boats with `BoatProbes` + `SphereWaterInteraction` wakes + free camera) running completely decoupled from `src/main.rs`.

---

### Phase 2: Core Crest `4.23.0` Port (`subcrates/crest_water`)

Implement inside `subcrates/crest_water` following `CLAUDE.md` (zero comments in code, WESL shaders via `load_shader_library!` / `embedded_asset!`):

1. **Clipmap & LOD Transforms (`OceanBuilder` Parity):**
   - `OceanSettings`, `OceanRoot`, `OceanViewer` (replaces `SeaViewer`), `LodTransform` (`lod_count`, snapped origin per cascade, texel size, scale).
   - CCW clipmap mesh with smooth LOD-morph attributes (`lod_alpha`) and frustum-culled tile drawing.
2. **LOD Data Cascades (`Texture2DArray` + Bevy `0.20.1` `Core3d` Systems):**
   - Schedule compute & render passes in `RenderApp` / `Core3d` prior to `Core3dSystems::MainPass` (no legacy `RenderGraph` nodes) in Crest's exact order:
     1. **`SeaFloorDepth`**: Orthographic terrain/mesh depth pass + `DepthInput` / `HeightGrid` upload trait (`OceanBed` or direct `Image`/mesh inputs).
     2. **`AnimatedWaves`**: GPU `ShapeFFT` spectrum/IFFT compute + `ShapeGerstner` + depth attenuation from `SeaFloorDepth`.
     3. **`DynamicWaves`**: GPU ping-pong wave equation compute driven by `DynamicWaveInput` emitters (replaces `src/ripples/`).
     4. **`Flow`**: Horizontal `(vx, vz)` cascade driven by `FlowInput` / Splines.
     5. **`Foam`**: Persistent foam simulation + decay driven by wave folding (Jacobian), `SeaFloorDepth`, `DynamicWaves`, and `FoamInput`.
     6. **`Shadow`**: Directional-light cascade shadow sampling into water volume.
     7. **`ClipSurface`**: Cutout/dry-zone mask (`ClipInput`).
3. **Surface, `WaterBody`, & Underwater WESL Shaders:**
   - **Surface (`Core3dSystems::MainPass`):** `SpecializedRenderPipeline` with WESL shaders reading `AnimatedWaves`, `Foam`, `Flow`, `SeaFloorDepth`, `Shadow`, `ClipSurface`, `DepthPrepass`, and `ScreenSpaceTransmission` (for refraction, absorption, caustics, SSS). Supports `WaterBody` regional overrides.
   - **Underwater:** Implement via `bevy_shader_kit::PostProcessPlugin` (`PostProcessEffect` in `Core3dSystems::EarlyPostProcess`, extended or paired with `AnimatedWaves` binding so the lens meniscus matches the Crest wave surface 1:1).

---

### Phase 3: Splines (`Spline`, `SplineMesh`, River/Lake Inputs) — No Floating Origin

Inside `subcrates/crest_water`:
1. **`Spline` & `SplinePoint` (`SplinePointData`):**
   - Cubic curve evaluation with per-point width/radius, height offset, flow speed, wave multiplier, and foam weight.
2. **`SplineMesh` & Cascade Writers:**
   - Adaptive ribbon tessellation with cross-section edge falloff.
   - Wire into `SeaFloorDepth` / `AnimatedWaves` height inputs (sloped rivers, elevated lakes), `Flow` inputs (river currents), `Foam` inputs, and localized `ShapeFFT`/`ShapeGerstner` shoreline waves.
3. **Floating Origin Explicitly Excluded:** Omit `FloatingOrigin` and origin-shift UV/cascade logic.

---

### Phase 4: Async Queries (`Readback` + FPI), Buoyancy & Example Boats

1. **`WaterQuery` / Collision Provider:**
   - Async `Readback` of active `AnimatedWaves`, `Flow`, and `SeaFloorDepth` LOD slices via `ReadbackComplete`.
   - CPU **Fixed Point Iteration (FPI)** solver exposing `sample_height`, `sample_displacement`, `sample_normal`, and `sample_flow`.
2. **Buoyancy & Example Boats:**
   - `BoatProbes` (multi-point Archimedes buoyancy + drag/lift driven by `WaterQuery`, applying forces/torques via `avian3d`'s `Forces` / `LinearVelocity` + `AngularVelocity`).
   - `SimpleFloatingObject` / `BoatAlignNormal` kinematic helpers.
   - `SphereWaterInteraction` emitting velocity wakes into `DynamicWaves` + `Foam`.
   - Wire into `harness/water_harness` interactive sandbox and `boat_wakes` golden test.

---

### Phase 5: Main Game Integration & Total Legacy Eradication

#### 5A. Rewire `src/integration/` & `src/terrain/` to `crest_water`
- **`src/integration/terrain_viewer.rs`:** Update `MainCamera` sync to drive `crest_water::OceanViewer` (drop `world_water::SeaViewer`).
- **`src/integration/submersion.rs`:** Query `crest_water`'s `WaterQuery` (`sample_height` / `sample_flow`) to populate `character::SubmersionState` (`src/character/types.rs` stays unchanged).
- **`src/integration/currents.rs`:** Read `sample_flow` from `crest_water`'s `WaterQuery` into `character.water.current` (replacing manual `terrain_gradient` approximation).
- **`src/integration/ripples.rs` → `src/integration/water_interactions.rs`:** Attach Crest `SphereWaterInteraction` (or emit `DynamicWaveInput` / `FoamInput` on straddle/move/breach) onto `Character` instead of `RippleField`.
- **`src/integration/body_foam.rs`:** Either remove (replaced by `DynamicWaves` + `Foam` emitters) or feed `CharacterBody` SDF into Crest's `Foam` cascade input.
- **`src/integration/post_process.rs`:** Drive Crest's `UnderwaterPostProcess` component on `MainCamera`, preserve `ScreenshotCaptureCamera` mirroring (`On<Add<ScreenshotCaptureCamera>>`), and keep `DistanceFieldView.active` suppression.
- **`src/integration/distance_field.rs`:** Update `On<Add<Sea>>` to `On<Add<OceanRoot>>` so SDF debug view hides the ocean.
- **`src/integration/water_commands.rs`:** Rebind console commands (`command!`) to Crest settings (`set_water_level`, wind/spectrum presets, cascade toggles, `debug_view!` layers for Crest LODs).
- **`src/terrain/`:**
  - Remove `impl world_water::SeaBed for Terrain` from `src/terrain/terrain.rs`. Feed `Terrain` chunks/`HeightGrid` to `crest_water`'s `SeaFloorDepth` via `src/integration/`.
  - Remove `world_water::WetSandSurface` from `src/terrain/chunks.rs`; keep `TerrainSurfaceMaterial` (`WetSandMaterial`) and sync `Wetness.height` from Crest's water level in `src/integration/`.

#### 5B. Delete Every Legacy Straggler (Zero Leftovers Checklist)
1. **Delete `subcrates/world_water/`** completely.
2. **Delete `src/ripples/`** completely (`RipplePlugin`, `RippleField`, `splash`).
3. **Strip legacy water from `crates/bevy_shader_kit/`:**
   - Delete `src/water.rs`, `src/fft.rs`, `src/underwater.rs`, `src/shaders/water/` (entire folder), and `src/shaders/post_process/underwater.wesl`.
   - Remove water-specific `ShaderHook`s and `water_blueprint()` from `src/compose.rs` (keeping generic `compose_wesl` / `ShaderBlueprint` if used elsewhere, or cleaning unused water hooks).
   - Remove `WaterMaterial`, `FftWaterTextures`, `UnderwaterPostProcess`, and legacy shader registrations from `src/lib.rs`.
   - *(Keep `bake.rs`, `field.rs`, `debug.rs`, `post_process.rs`, `wet.rs`, and non-water shaders used by `terrain`, `distance_field`, and `debug_views`.)*
4. **Update `harness/water_harness/Cargo.toml`:** Remove the temporary `world_water` dependency and legacy-comparison tests so the harness only runs `crest_water` + Crest `4.23.0` goldens.
5. **Update `Cargo.toml` & `src/main.rs`:**
   - Remove `world_water` from root `Cargo.toml` dependencies.
   - Remove `WaterPlugin` and `RipplePlugin` from `src/main.rs`; add `CrestWaterPlugin`.
6. **Update `tests/module_isolation.rs`:**
   - Remove `"ripples"` from `FEATURES` (and add any new feature module if created, though everything should stay in `subcrates/crest_water` + `src/integration/`).
7. **Verification:**
   - `rg -i "world_water|SeaState|SeaViewer|SeaBed|WaterSurface|WetSandSurface|RippleField|WaterMaterialExt"` returns **0 matches**.
   - `cargo test --workspace` (including `crate_isolation`, `subcrate_isolation`, `module_isolation`, and `water_harness` golden suite) passes cleanly.
   - `cargo run` boots the main game with Crest water active, and `cargo run -p water_harness` runs the standalone interactive boat + spline river demo.

---

Your repo already has the skeleton of a Unity exporter in `tools/unity_crest_goldens/` and a Bevy test runner in `harness/water_harness/`, **but they are currently disconnected and have a few blockers that stop true headless Unity golden generation**.

Here is what is broken right now and how to get headless Unity goldens working end-to-end.

---

### 1. Why the current harness isn't actually testing against Unity

1. **The committed `goldens/*.json` are self-generated in Rust, not from Unity.**
   `harness/water_harness/goldens/*.json` were written by `harness/water_harness/src/bin/generate_goldens.rs`, which steps `CpuOcean` (`crest_water::reference`). So `golden_checks.rs` only verifies that Bevy's GPU shaders match your own Rust CPU reference—not Unity Crest 4.23.0.
2. **Incompatible schemas and scenarios.**
   - `tools/unity_crest_goldens/Editor/GoldenSchema.cs` (`GoldenDocumentDto`) exports multi-timestamp `probes`, `.bin` slice dumps (`slices`), boat `trajectories`, `frames` PNGs, and `settings`.
   - `harness/water_harness/src/golden.rs` (`Golden`) only reads a single-frame `samples` array over a 2×2 m patch around `(16, 16)`.
   - `harness/water_harness/src/scenarios.rs` has drifted from `tools/unity_crest_goldens/Editor/ScenarioBuilder.cs` (e.g., different `seabed` depths/slopes, `turbulence`, wind speeds, spline coordinates, and manual `Gerstner` waves in `storm_swell` that Unity doesn't spawn).

---

### 2. Fixing Headless Generation in `tools/unity_crest_goldens/`

#### A. How to run Unity headlessly with GPU compute & readback
In Unity, **`-batchmode` without `-nographics`** is headless (no editor window) while still creating a real GPU device so Crest's compute shaders, `RenderTexture`s, and `AsyncGPUReadback` work:
- **Windows / macOS:** `<UnityEditor> -batchmode -projectPath ... -executeMethod ...` (no `-nographics`).
- **Linux CI / headless box:** wrap with `xvfb-run -a` (or headless Vulkan) without `-nographics`.

#### B. Fix the `-batchmode` Foam Blocker (`GoldenExporter.cs`)
Right now `BuildWork.Tick()` aborts on `storm_swell`, `shallow_shore`, and `spline_river`:
```csharp
if (_ctx.Spec.foamEnabled && ocean._lodDataFoam == null)
    throw new InvalidOperationException("Crest does not create the foam simulation in batch mode...");
```
Crest 4.23 skips creating `_lodDataFoam` when `OceanRenderer.RunningHeadless` is true (`Application.isBatchMode || RunningWithoutGPU`). Because you have a real GPU in `-batchmode` without `-nographics`, the foam compute shader works fine—Crest just skipped instantiating it.

Two clean ways to fix this in your Unity golden project:
1. **Unmodified Crest (via reflection in `BuildWork.Tick`):** If `spec.foamEnabled && ocean._lodDataFoam == null`, construct `LodDataMgrFoam` via `LodDataMgr.Create<LodDataMgrFoam, LodDataMgrFoamSettings>(ocean, ...)` (inspect `OceanRenderer.CreateLodDataManagers` in Crest 4.23 for the exact internal list/dictionary registration).
2. **1-line Crest patch in the exporter project:** Change `RunningHeadless` in `OceanRenderer` from checking `Application.isBatchMode` to `RunningWithoutGPU` (`SystemInfo.graphicsDeviceType == GraphicsDeviceType.Null`).

#### C. Run `GoldenExporter` in Play Mode (instead of Edit Mode `EditorApplication.update`)
Currently `GoldenExporter.ExportAll()` stays in Edit Mode and relies on `EditorApplication.update`, `PumpGpu()` (`Graphics.Blit`), and manually reflecting `BoatProbes.Start`, `BoatProbes.FixedUpdate`, and `SphereWaterInteraction.LateUpdate`. That leaves several Edit Mode hazards:
- `ShapeFFT`, `Spline`, `RegisterSeaFloorDepthInput`, `RegisterFlowInput`, `RegisterFoamInput`, `QueryDisplacements`, and `QueryFlow` depend on Unity's `Awake` / `OnEnable` / `Start` / `Camera` lifecycle, which isn't guaranteed to execute in sync in Edit Mode `-batchmode`.
- `Physics.Simulate()` is called without `Physics.SyncTransforms()`, and `Step(0f)` during `CheckpointWork.Poll` sets `_deltaTime = 0`, which can alter or stall `DynamicWaves`/`Foam` substeps.

**Better approach:** Drive the export using **Unity Test Framework Play Mode tests (`-runTests -testPlatform PlayMode -batchmode`)** or have `ExportAll` enter Play Mode (`EditorApplication.EnterPlaymode()`) and run as a `MonoBehaviour` coroutine/async loop:
- Every Crest component initializes and ticks through Unity's real `PlayerLoop`.
- `AsyncGPUReadback` and `Camera.Render()` resolve naturally each rendered frame without needing `PumpGpu()`.
- You can still control simulation time deterministically via `TimeProviderCustom` (`_overrideTime = true`, `_overrideDeltaTime = true`) and `Physics.simulationMode = SimulationMode.Script`.

#### D. Specific Bugs to Fix in `tools/unity_crest_goldens/`
1. **`SliceWriter.WithJacobian` (`SliceWriter.cs` lines 166–173) is missing `1 +` on the diagonal:**
   ```csharp
   var dxX = (raw[right * 4] - x) / texelWidth;
   var dzZ = (raw[up * 4 + 2] - z) / texelWidth;
   ```
   The deformation Jacobian of $(x + \text{disp}_x, z + \text{disp}_z)$ is $(1 + \partial_x \text{disp}_x)(1 + \partial_z \text{disp}_z) - (\partial_z \text{disp}_x)(\partial_x \text{disp}_z)$ (as `subcrates/crest_water/src/reference/fft.rs` already computes). Without `1.0f +`, calm water reports `jacobian ≈ 0` instead of `~1.0`.
2. **`FrameWork.Tick` (`GoldenExporter.cs`) misses Crest's Underwater effect:**
   `GoldenFrameCamera` is created as a bare `Camera` with no underwater renderer component/post-process attached, so `underwater_curtain`'s `surface_straddle.png` won't actually render the underwater curtain. Attach/enable Crest's `UnderwaterEffect` (or URP/HDRP renderer feature) on `GoldenFrameCamera`.
3. **Export `Seabed` and `Spline` metadata cleanly + `Shader` parity:**
   `GoldenSettingsDto` already records `water_level`, `wind_speed`, `wind_direction`, `gravity`, `chop`, `turbulence`, `amplitude_multiplier`, `loop_period`, `lod_count`, `lod_resolution`, `base_texel_size`, `seabed`, and `splines`. Keep `ScenarioBuilder.cs` as the single source of truth for scenarios so `water_harness` constructs scenarios directly from `golden.json`.

---

### 3. Wiring `harness/water_harness/` to Consume Unity Goldens

Once Unity exports `<output>/<scenario>/golden.json` + `.bin` + `.png` (commit them under e.g. `harness/water_harness/unity_goldens/`):

1. **Parse `GoldenDocumentDto` in `harness/water_harness/src/golden.rs`:**
   Add `serde` structs matching `GoldenSchema.cs` (`settings`, `probes`, `slices`, `trajectories`, `frames`, `tolerances`), plus a loader for little-endian `f32` `.bin` slices.
2. **Apply Unity LH $\leftrightarrow$ Bevy RH coordinate conversion (`z_bevy = -z_unity`):**
   - **Inputs (`settings` -> `OceanSettings` / `Seabed` / `FlowSpline` / `FoamSpline`):**
     - `wind_direction = Vec2::new(wx, -wz)`
     - `seabed.slope_z = -slope_z`
     - `spline.points`: negate each `z`
     - `viewer`: `(0, 2, 0)` (matching `ScenarioBuilder.cs`, instead of `(16, 0, 16)`)
   - **Outputs (Bevy -> Unity comparison):**
     - Query probe at `(probe.x, -probe.z)`
     - Compare `disp`: `(dx, dy, -dz)` vs `probe.displacement`
     - Compare `normal`: `(nx, ny, -nz)` vs `probe.normal`
     - Compare `flow`: `(fx, -fz)` vs `probe.flow`
3. **Query at exact checkpoint timestamps using `WaterQueryPoints` (`WaterQuery`):**
   - `CalmOcean`, `StormSwell`, `ShallowShore`, `SplineRiver`, and `UnderwaterCurtain` have `6..16` probes per checkpoint (`t = 0, 4, 8`), which fits inside `MAX_QUERY_POINTS = 32` (`subcrates/crest_water/src/compute.rs`).
   - Run the Bevy app with `TimeUpdateStrategy::ManualDuration(1/60s)`, stop at each `t` checkpoint (`ManualDuration(ZERO)` while readbacks complete), and compare `WaterQuery` (plus `copy_slice` / `Readback` for full-slice RMS checks against `disp_lod0_t4.bin`, `depth_lod0_t0.bin`, `flow_lod0_t4.bin`, `foam_lod0_t8.bin`).
4. **Fix `crest_water` harness gaps that Unity goldens will expose:**
   - **Boat trajectory (`boat_wakes`):** `ScenarioBuilder.cs` drives a `Rigidbody` with `BoatProbes` + two `SphereWaterInteraction` spheres; `water_harness` currently only fakes a moving `DynWaveEmitter`. Have the `boat_wakes` test spawn an `avian3d` `RigidBody` + `crest_water::buoyancy::BoatProbes` + `SphereWaterInteraction` (note: `buoyancy.rs` currently reads `WaterSampler` instead of `WaterQuery`, so moving outside the `16x16` `WaterQueryPoint` slice falls back to flat `water_level`—wiring `BoatProbes` to `WaterQueryPoints` will be needed for the boat trajectory test to pass).
   - **Foam & `lod_resolution`:** `subcrates/crest_water/src/shaders/fft.wesl` hardcodes `const FFT_SIZE: u32 = 64u;` (keep `lod_resolution = 64` or parameterize `FFT_SIZE`).
   - **Spline river height & dual-channel `SeaFloorDepth`:** `ScenarioBuilder.cs`'s `SplineRiver` slopes from `y = -1.5` to `-1.0`; `crest_water` currently only writes `Flow` and `Foam` from splines without modulating `SeaFloorDepth` / water elevation.
   - **Underwater curtain (`underwater_curtain`):** `subcrates/crest_water/src/underwater.rs` currently toggles `enabled` by `eye.y < surface` on CPU, whereas Unity renders a per-pixel meniscus straddling the camera lens (matching the partial-submersion design in `crates/bevy_shader_kit/src/shaders/post_process/underwater.wesl`).

---

### Recommended Order of Operations

1. **Patch `tools/unity_crest_goldens/`:** fix `SliceWriter.WithJacobian` (`1.0f +`), resolve the `_lodDataFoam` batchmode skip, add `UnderwaterEffect` to `FrameWork`'s camera, and move execution into a Play Mode test / Play Mode runner invoked via `unity -batchmode` (without `-nographics`).
2. **Run Unity once and commit `harness/water_harness/unity_goldens/`** (`golden.json`, `.bin`, `.png`) for all 6 scenarios.
3. **Update `harness/water_harness/`** to build scenarios from `golden.json` with `z -> -z` conversion, compare `WaterQuery` probes + slice RMS + boat trajectory against the Unity goldens, and keep `cargo test -p water_harness` running completely offline in CI.