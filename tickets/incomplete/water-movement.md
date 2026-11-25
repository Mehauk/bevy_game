### Character Movement & Physics Techniques (What You're Missing)

Right now `src/character.rs` only has `Standing | Walking | Running | Falling | Jumping` and `ground.is_grounded` vs. air gravity (`base_gravity = 2.5`, `fall_gravity = 3.5`), while `Crouch` (`ControlLeft`/`ControlRight`) isn't wired into movement yet.

#### A. Use continuous submersion depth instead of a binary `in_water` check
In `src/integration/ripples.rs`, you currently check a single point:
`transform.translation.y - FEET_OFFSET < water.translation.y + SUBMERGE_MARGIN`.
Using a binary check for movement causes jarring snaps in ankle-deep water.

Instead, compute continuous submersion along your capsule (`CAPSULE_HEIGHT = 1.0`, `CAPSULE_RADIUS = 0.4` → `1.8m` total height, `FEET_OFFSET = 0.9m`):
- **`0.0 < depth < ~1.1m` (Wading):** Keep the character grounded! Smoothly reduce walk/sprint speed and jump height as depth rises instead of abruptly switching to swimming.
- **`depth >= ~1.15m` (Chest/neck submerged → Swimming/Floating):** Disable ground snapping (`slope_snap_speed` / `GROUND_SNAP_DISTANCE`) so Avian doesn't fight buoyancy and glue your feet to the seabed, and switch to fluid movement.
- **Shoreline exit / breaching:** When swimming into a walkable slope (`is_walkable`), allow `Jump` near the surface (or an upward step bias) to push you cleanly onto land so you don't get stuck bobbing at the shoreline.

#### B. Damped spring buoyancy + quadratic drag (instead of constant up/down speed)
A common mistake for *"Space to float, let go to sink"* is setting `velocity.y = +const` / `-const`. That causes high-frequency bobbing jitter at `water_height` and kills entry momentum.

**Better technique:**
- **Floating (`Space` held):** Drive the capsule toward an equilibrium float line (e.g., chin/chest at `water_height`) with a clamped damped spring:
  $$a_{\text{buoy}} = \text{clamp}\big(k_p (y_{\text{target}} - y) - k_d v_y,\; -a_{\max},\; a_{\max}\big)$$
- **Sinking (`Space` released):** Apply reduced underwater gravity (e.g. `0.25–0.4 × base_gravity`, or slightly negative buoyancy) so you sink gently rather than dropping like a stone.
- **Quadratic water drag:** Apply `v *= (-drag * dt).exp()` (or `-c * v * |v|`) when submerged. This gives you **natural splash deceleration** when jumping into deep water—you plunge briefly below the surface before buoyancy carries you back up—and gives swimming a weighty glide instead of `acceleration = 60.0` land snapping.

#### C. Split surface swimming vs. 3D camera-pitch diving
Right now `CameraFacing` (`src/integration/bridges.rs`) only forwards `rig.yaw` and `first_person`, and `camera_relative_direction` only moves in `XZ`.
- Forward `rig.pitch` into `CameraFacing` too.
- **At the surface (floating / treading):** Keep `WASD` movement horizontal (`XZ`) so looking slightly down at the water or up at the sky doesn't accidentally nose-dive or stall you.
- **While diving (`Ctrl` / `Crouch` held, or fully submerged):** Steer forward/back along the full 3D camera forward vector (`Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Vec3::NEG_Z`), with `Space` adding world `+Y` ascent and `Ctrl` adding world `-Y` descent.

#### D. Tilt `CharacterBody` for visual feedback
In `character_rotate_body`, `CharacterBody` only slews around `Y`. Tilting `CharacterBody` forward along velocity/dive pitch (and adding a subtle bob/roll while treading or swimming) immediately sells swimming motion even with a capsule + visor mesh.

#### E. Keep module isolation clean
Per `CLAUDE.md`, `src/character.rs` shouldn't reach into `terrain` (note `character.rs` currently imports `Terrain` directly). Add a `SubmersionState` / `WaterImmersion` component on `Character` (plus `Wading`, `Floating`, `Swimming`, `Diving` variants in `MovementState`), and populate `SubmersionState` from `WaterHeightSource` in `src/integration/`, matching how `CameraFacing` and `ripples.rs` already work.
