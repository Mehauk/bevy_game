### Water Interaction & Shader Techniques (Upgrades Over Current Code)

#### A. Fix deep-underwater surface wakes & add idle/exit ripples (`src/integration/ripples.rs`)
Currently, `emit_character_splashes` checks `feet_y < water_y + SUBMERGE_MARGIN`. That stays `true` even when you are 20m underwater on the ocean floor (`Terrain` drops to `-30.0`), so diving or walking deep underwater still emits surface wakes above you.
- **Better check:** Only emit surface splashes/wakes when the character **straddles the water surface** (`feet_y < water_y + margin && head_y > water_y - margin`).
- **Missing events worth adding:**
  - **Idle treading rings:** Periodic weak pulses every ~0.7s while floating in place so treading water doesn't look frozen.
  - **Exit / breach splash:** Smaller ring when leaving the surface upward (`was_straddling && !straddling && vel.y > 0.0`).
  - **Scaled wake strength:** Modulate wake strength by cross-section / speed rather than a constant `WAKE_GAIN * horizontal_speed` above `1.2 m/s`.

#### B. Analytical 32-wave array (`RippleSet`) vs. 2D Wave Heightfield Ping-Pong
Right now `src/ripples/mod.rs` + `ripples.wesl` evaluates up to `MAX_RIPPLES = 32` analytical rings (`1..4` loop steps each) per pixel.
- **Weakness:** Hard cap of 32 waves (evicting old ones abruptly), O(N) per-pixel loop cost, **no interaction/reflection off shores or rocks**, **no vertex displacement**, and **no persistent advected foam trail**.
- **Better technique:** A **2D shallow-water / wave-equation ping-pong render texture** (e.g. `256×256` `Rgba16Float` scrolling with the player) updated via compute/fragment shader:
  - Supports unlimited splashes, swimming strokes, and rain in constant time per pixel.
  - Reflects and damps against terrain shores (by sampling terrain height / SDF).
  - Drives both **normal perturbation**, **lingering wake foam**, and **subtle vertical surface displacement**.

#### C. Use the 3D SDFs you are already baking!
You already bake 3D `R32Float` SDFs for both `CharacterBody` (`src/integration/bridges.rs`) and `TerrainSurface` (`src/integration/distance_field.rs`), and `crates/bevy_shader_kit/src/shaders/sdf/volume.wesl` already provides `volume_sample_world` and `volume_gradient_world`—but **`water.wesl` isn't using them yet.**
Instead, `water.wesl` computes `ctx.depth` purely from screen-space `prepass_depth`:
- **Where screen-space `prepass_depth` fails:**
  1. Looking up from underwater (the sky/far plane or terrain behind the camera breaks `surface_view_z - behind_view_z`).
  2. Grazing angles or occluded geometry behind waves/characters.
  3. Thin/fast character limbs where you want a crisp foam collar around the body.
- **Better technique:** Sample the character/terrain `SdfVolume` in `water.wesl` (or combine SDF distance for nearby props/character + prepass depth for far terrain). That gives you **view-independent contact foam** right around the swimming character and accurate underwater ray-exit distance.

#### D. Subdivided Water Mesh + Shared CPU/GPU Gerstner Waves
Right now `Sea` (`src/terrain/water.rs`) is a single flat `Plane3d` (`300×300`, 1 quad). Because the silhouette is completely flat, floating and wading look like moving through a glass floor.
- **Better technique:** Use a subdivided/camera-centered water mesh and evaluate 2–4 **Gerstner waves** in the vertex shader **and** in an identical small Rust function on the CPU (`water_height_at(x, z, time)`).
  - Buoyancy automatically bobs the character with the waves.
  - Your planned partial-submersion meniscus (`tickets/incomplete/post-processing.md`) will match the rendered water surface 1:1 instead of cutting across a flat plane.

#### E. Underwater Camera Behavior & `Sea` Back-Faces
Two things to pair with `tickets/incomplete/post-processing.md`:
1. **`Sea` back-face optics (`crates/bevy_shader_kit/src/shaders/water/water.wesl`):** `Sea` uses `cull_mode: None` with `specular_transmission: 1.0`. Once you can dive under it, `water_volume_in_scatter` will currently still fire when looking up from below (`!front`), and `WATER_IOR` (`1.0 / 1.333`) will refract with air-to-water ratios instead of water-to-air (`1.333 / 1.0`, which produces **Snell's window** + total internal reflection). Branching on `front` fixes looking up from underwater immediately.
2. **Third-person surface flicker (`src/camera.rs`):** In third-person, when the character floats at `water_y` and you orbit the camera near eye level, the camera repeatedly crosses `y = water_y`. Either make sure your partial-submersion post-process handles lens-crossing smoothly (no binary toggle!), or add a small vertical surface clearance/hysteresis in `sync_camera_transform` when the camera is right on the water plane.

---

### Recommended Implementation Order

1. **Movement core (`src/character.rs` + `src/integration/`):** Add `SubmersionState` (continuous depth from `WaterHeightSource`), `MovementState::{Wading, Floating, Swimming, Diving}`, damped spring buoyancy (`Space` float / release sink), quadratic water drag, `Ctrl` diving, and 3D pitch steering via `CameraFacing.pitch`.
2. **Fix surface ripples (`src/integration/ripples.rs`):** Gate splashes/wakes on *straddling* the surface (not deep underwater), add idle treading rings + exit splashes, and tilt `CharacterBody` while swimming.
3. **Underwater vision (`tickets/incomplete/post-processing.md` + `water.wesl` `!front` fix):** Diving immediately feels 10× better once underwater absorption, Snell's window, and the lens meniscus are visible.
4. **Upgrade water fidelity (`crates/bevy_shader_kit`):** Wire the existing `CharacterBody` / `TerrainSurface` SDFs into `water.wesl` for body-contact foam, then upgrade `Plane3d` + `RippleSet` to Gerstner displacement + a 2D wave ping-pong grid if you want physical waves and wake trails.

Here is why, along with the one Gerstner gotcha to watch out for:

### Why CPU evaluation is cheap and better than GPU readback here
1. **The cost is near zero:** 2–4 waves for 1 character (or even 20 floating props) is ~10–50 `sin`/`cos` calls per `FixedUpdate`—less than a microsecond. The GPU still does all the heavy work across the thousands of mesh vertices.
2. **Zero latency:** Reading a height texture back from the GPU in Bevy's pipelined renderer introduces **1–3 frames of lag** plus staging-buffer complexity. Laggy height samples fed into a buoyancy spring in `FixedUpdate` cause oscillation and jitter. Analytical CPU evaluation is synchronous and deterministic.
3. **Only sample the big swells on CPU:** You don't need to evaluate small surface ripples (`RippleField` in `src/ripples/mod.rs`) on the CPU—a character shouldn't bob up and down from 2cm capillary ripples anyway. Keep small ripples GPU-only (normal/foam/optional vertex offset) and match CPU+GPU only on the large swells.

---

### The Gerstner Gotcha (and the 3-line fix)
A **Gerstner wave** displaces a vertex **horizontally** `(dx, dz)` as well as vertically `dy`:
$$\text{displaced}(x, z) = (x + dx,\; dy,\; z + dz)$$

That means if you simply plug the character's `(pos.x, pos.z)` into the Gerstner formula, you're actually asking for the height of the water particle whose *rest* position was `(pos.x, pos.z)`—which has now drifted horizontally to `(pos.x + dx, pos.z + dz)`. On steep ocean waves, that makes floating objects look slightly out of phase with the mesh.

Two ways to handle it cleanly:
- **Option A — 2-step fixed-point solve (standard trick):**
  1. Evaluate the wave displacement `(dx, _, dz)` at `(x, z)`.
  2. Re-evaluate at `(x - dx, z - dz)` (and optionally one more iteration) and take that `y`.
  For typical non-breaking swells, 2 evaluations converge to millimeter accuracy.
- **Option B — Vertical-only swells (`Q = 0`):**
  If your `Sea` is a calm lake or gentle coastal swell rather than steep curling ocean waves, skip horizontal displacement altogether and just sum 2–4 vertical sine/trochoidal waves (e.g. `pow((sin(...) + 1.0) * 0.5, k)` for sharper crests and wider troughs). Then `y = f(x, z, t)` is exact in a single call with zero horizontal mismatch.

---

### When you *wouldn't* do it on the CPU
- **GPU-driven simulation (FFT ocean or 2D shallow-water ping-pong grid):** If waves dynamically reflect off shorelines or collide with boats, there is no closed-form equation anymore—at that point you'd either sample a coarse async-readback height grid (and accept ~2 frames of lag, softened by damping) or use analytical swells on CPU for buoyancy + GPU ping-pong grid purely for local wakes/ripples.
- **One Bevy-specific sync note:** Your `sync_water_params` (`src/terrain/water.rs`) feeds `time.elapsed_secs()` (`Time<Virtual>`) to the GPU, which pauses cleanly with `PlayState::Paused` (`src/state.rs`). Inside `FixedUpdate` (`character_physics_move`), `Res<Time>` is `Time<Fixed>` (which tracks `Time<Virtual>` within one `1/64s` step). That tiny difference is invisible for gentle swells, or you can read `Res<Time<Virtual>>` directly if you want exact parity.