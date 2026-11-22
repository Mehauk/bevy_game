# New docs
- [ ] similar to lua only hooks, a new doc should be generated called commands

# New commands

## screenshots
goal: Ability to take aerial shots from third person view. with robust implementation details for scalability
- [ ] take screenshot of whatever is in the camera's view
  - [ ] make this an abstraction that can work in any view with any camera
- [ ] angle camera
  - [ ] this allows commands to reposition camera instead of only with mouse

## debugging info
goal: Add debugging info under the fps view that can be toggled on or off via a command.needs to be scalable as new things are added.
- [ ] Create a register of info that will need to be added to if something is added for debugging purposes.
- [ ] register
  - [ ] movement state: standing, walking, running, falling, jumping
  - [ ] movement speed

# PLAN
Here is a concrete architectural and implementation plan for `tickets/incomplete/commands.md`, aligned with the codebase’s module-isolation rules (`CLAUDE.md` — feature modules never import one another; `utils/` stays a plugin-free kernel; no code comments; `docs/.compiled/*` generated at runtime only).

---

### 1. Generated Commands Doc (`docs/.compiled/commands.md`)

**Files:** `src/docgen/commands_docs.rs` (new), `src/docgen/mod.rs`

- **`src/docgen/commands_docs.rs`:**
  - Add a `Startup` system `write(registry: Res<CommandRegistry>)` (allowed because `utils::registry::commands` lives in the shared kernel).
  - Extract a pure `format_markdown(commands: &[CommandEntry]) -> String` helper for unit testing.
  - Group `registry.commands` (already sorted alphabetically) by namespace prefix before the first `>` (falling back to `"general"` if none), emitting sections matching `lua_docs.rs`:
    - Header: `# Commands\n\nGenerated at startup by the \`docgen\` plugin. Do not edit by hand.\n\n`
    - Note that the trailing segment after `>` works as a shorthand alias when unique.
    - Under each `## <namespace>`: `- \`<cmd.name>\` — <cmd.description>`.
  - Call `write_doc("commands.md", &out)`.
- **`src/docgen/mod.rs`:**
  - Add `mod commands_docs;` and register `(lua_docs::write, commands_docs::write)` in `Startup`.
- **Verification:** Launch the app and inspect `docs/.compiled/commands.md`; unit-test `format_markdown` with empty, single, and multi-namespace inputs.

---

### 2. Camera Angling & Repositioning (`angle_camera`)

**Files:** `src/camera.rs`

Currently `CameraRig` clamps mouse pitch at `PITCH_LIMIT = 1.35` rad (~77.3°) and mouse zoom at `MAX_ZOOM_DIST = 36.0`, and has no command to orient or inspect the rig.

- **`CameraRig` methods (pure, unit-tested):**
  - Keep `apply_look` (`PITCH_LIMIT = 1.35`) and `apply_zoom` (`MAX_ZOOM_DIST = 36.0`) unchanged so mouse feel and existing tests remain intact.
  - Add command-oriented constants & methods on `CameraRig`:
    - `MAX_COMMAND_PITCH: f32 = 1.54` (~88.2°, avoiding `look_at(..., Vec3::Y)` gimbal lock while enabling near-vertical aerial shots).
    - `MAX_COMMAND_DISTANCE: f32 = 500.0` (useful on the 400×400 `TerrainMap`).
    - `set_angles_deg(&mut self, yaw_deg: f32, pitch_deg: f32)` -> converts to radians, clamps pitch to `[-MAX_COMMAND_PITCH, MAX_COMMAND_PITCH]`.
    - `set_distance(&mut self, distance: f32)` -> clamps to `[MIN_THIRD_PERSON_DIST, MAX_COMMAND_DISTANCE]` and sets `view_mode = ViewMode::ThirdPerson`.
- **Commands registered in `CameraPlugin::build`:**
  - `angle_camera <yaw_deg> <pitch_deg> [distance]` (`camera>angle_camera`):
    - Parses `yaw_deg` and `pitch_deg` from `CommandArgs` (using degrees for console ergonomics; remember negative `pitch_deg`, e.g. `-75`, puts the third-person camera above the target looking down).
    - If optional 3rd arg `distance` is provided, calls `rig.set_distance(dist)` (switching to `ThirdPerson`). Otherwise preserves the current `ViewMode` (works in both 1st and 3rd person).
    - Writes result or usage string to `CommandOutput`.
  - `camera_info` (`camera>camera_info`):
    - Outputs current `view_mode`, `yaw` (deg), `pitch` (deg), `distance`, and `Transform.translation` via `CommandOutput` so aerial angles can be inspected and tuned.
- **Pause-time behavior:** `CameraSystems::UpdateTransform` (`sync_camera_transform`) and `sync_skybox_to_camera` run in `PostUpdate` (ungated by `PlayState::Running`), so `angle_camera` updates the view immediately even while the `~` command palette has the game paused.
- **Also:** Add `Name::new("MainCamera")` when spawning the camera in `setup_camera` so camera-selection commands can target it by name.

---

### 3. Camera-Agnostic Screenshot Abstraction

**Files:** `src/screenshot.rs` (new), `src/main.rs`

Running a screenshot command from the `~` command palette happens while the console UI is covering the bottom third of `PrimaryWindow`. Capturing `PrimaryWindow` directly would include the console and would not generalize to offscreen/secondary cameras.

- **Decoupled design (`ScreenshotPlugin` in `src/screenshot.rs`):**
  - Depends only on Bevy (`Camera`, `RenderTarget`, `Projection`, `GlobalTransform`, `Screenshot`, `ScreenshotCaptured`, `save_to_disk`, `Image`) and `utils::registry::commands`. Never imports `camera.rs` or any other feature module.
  - **Core abstraction types:**
    - `pub enum CameraSelector { ActivePrimary, Named(String), Entity(Entity) }`
      - `ActivePrimary`: selects the active `Camera` targeting `RenderTarget::Window(WindowRef::Primary)` (preferring lowest `order` / `Camera3d`), falling back to the first active `Camera`.
      - `Named(String)`: matches any camera entity carrying a matching `Name`.
    - `pub enum CaptureMode { CameraView, RenderTarget }`
      - `CameraView` (default): captures **only what is in the camera's view** (no UI/command palette) regardless of whether that camera renders to a window, viewport, or texture.
      - `RenderTarget`: captures the camera's own `RenderTarget` directly via `Screenshot(render_target.clone())` (useful when HUD/overlays are wanted, or when capturing an existing persistent `RenderTarget::Image`).
    - `#[derive(Message, Clone, Debug)] pub struct CaptureScreenshot`:
      - `{ camera: CameraSelector, mode: CaptureMode, resolution: Option<UVec2>, output_path: Option<PathBuf> }`
      - Any future system, hotkey, or Lua hook can trigger screenshots simply by writing `CaptureScreenshot`.
- **How `CaptureMode::CameraView` works (Bevy 0.20):**
  1. On receiving `CaptureScreenshot`, resolve the target camera entity and read `(&Camera, &GlobalTransform, &Projection)` plus optional view components (`DistanceFog`, `Exposure`, `Msaa`, `Tonemapping`, `RenderLayers`).
  2. Determine resolution: `req.resolution` -> `camera.physical_viewport_size()` -> fallback `UVec2::new(1920, 1080)`.
  3. Create `Image::new_target_texture(w, h, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb))` and ensure `TextureUsages::COPY_SRC` is set on `image.texture_descriptor.usage` (`new_target_texture` sets `TEXTURE_BINDING | COPY_DST | RENDER_ATTACHMENT`, and `COPY_SRC` is needed for GPU readback).
  4. Spawn a temporary capture camera entity with `Camera3d::default()`, `Camera { order: -1, ..default() }`, `RenderTarget::Image(image_handle.clone().into())`, cloned `Transform::from_matrix(global_transform.to_matrix())`, `Projection`, and cloned lighting/fog/layer components, plus a `PendingOffscreenCapture { image, path, ready_to_capture: false }` component.
  5. Wait 1 frame (`ready_to_capture` flips `false -> true` on the first `Update` tick after spawn, guaranteeing `PostUpdate` has propagated transforms/projection and the offscreen pass has rendered at least once).
  6. On that tick, create `screenshots/` parent directory (`fs::create_dir_all`), spawn `Screenshot::image(image_handle)` with:
     - `.observe(save_to_disk(path.clone()))`
     - a cleanup observer on `On<ScreenshotCaptured>` that despawns the temporary capture camera entity (and logs `Saved screenshot to ...`).
- **Commands registered in `ScreenshotPlugin::build`:**
  - `screenshot [filename] [width height]` (`screenshot>screenshot`) — captures `ActivePrimary` in `CaptureMode::CameraView` to `screenshots/<filename_or_timestamp>.png`.
  - `screenshot_window [filename]` (`screenshot>screenshot_window`) — captures `ActivePrimary` in `CaptureMode::RenderTarget` (includes UI).
  - `screenshot_camera <camera_name> [filename]` (`screenshot>screenshot_camera`) — captures any named camera in `CaptureMode::CameraView`.
  - All write the destination path to `CommandOutput`.
- **Wire in `src/main.rs`:** Add `mod screenshot;` and `ScreenshotPlugin` to the second `.add_plugins((...))` tuple (currently 8 plugins, well under the 15-tuple limit).

---

### 4. Scalable Debug Info Registry & Overlay

**Files:** `src/utils/registry/debug_info.rs` (new), `src/utils/registry/mod.rs`, `src/character.rs`, `src/dev.rs`

Because `dev` and `character` cannot import each other, the debug register must live in the kernel (`utils/registry/`), mirroring `CommandRegistry` / `CommandAppExt` / `command!`.

#### A. Kernel Registry (`src/utils/registry/debug_info.rs`)
- No feature imports, no `Plugin` (matches `CLAUDE.md` rule for `utils/`).
- `DebugInfoEntry`: `{ pub category: &'static str, pub label: &'static str, pub system_id: SystemId<(), Option<String>> }`.
- `#[derive(Resource, Default)] pub struct DebugInfoRegistry { pub entries: Vec<DebugInfoEntry> }`:
  - `register(&mut self, category, label, system_id)` deduplicates by `(category, label)` and sorts by `(category, label)` for deterministic display order.
- `pub trait DebugInfoAppExt`:
  - `register_debug_info<F, M>(&mut self, category: &'static str, label: &'static str, system: F) -> &mut Self` where `F: IntoSystem<(), Option<String>, M> + 'static`.
  - Lazily calls `self.init_resource::<DebugInfoRegistry>()` if missing so plugin registration order in `main.rs` never matters.
- `debug_info!` macro (mirroring `command!`):
  - `debug_info!(app, "Movement State", debug_movement_state)` derives `category` from `module_path!()` at the call site (e.g. `"character"`).

#### B. Character Movement State & Speed (`src/character.rs`)
- Introduce `#[derive(Component, Default, Clone, Copy, Debug, PartialEq, Eq)] pub enum MovementState { #[default] Standing, Walking, Running, Falling, Jumping }` + `impl fmt::Display`.
- Extract pure helpers (with unit tests for all 5 states + speed):
  - `horizontal_speed(velocity: Vec3) -> f32` -> `Vec2::new(velocity.x, velocity.z).length()`
  - `classify_movement_state(is_grounded: bool, horiz_speed: f32, velocity_y: f32, sprinting: bool) -> MovementState`:
    - If `!is_grounded`: `velocity_y > 0.0` -> `Jumping`, else `Falling`.
    - Else if `horiz_speed <= MOVEMENT_SPEED_EPSILON` (`0.05`): `Standing`.
    - Else if `sprinting`: `Running`.
    - Else: `Walking`.
    *(Checking `!is_grounded` first prevents walking down slopes from misreporting as `Falling`.)*
- Insert `MovementState::default()` in `setup_character`, update it at the end of `character_physics_move`, and reset it to `MovementState::default()` in `respawn_character`.
- Register both entries in `CharacterPlugin::build`:
  - `debug_info!(app, "Movement State", debug_movement_state)` -> queries `Query<&MovementState, With<Character>>`, returns `Some(state.to_string())`.
  - `debug_info!(app, "Movement Speed", debug_movement_speed)` -> queries `Query<&LinearVelocity, With<Character>>`, returns `Some(format!("{:.2} m/s", horizontal_speed(vel.0)))`.

#### C. Overlay & Toggle Command (`src/dev.rs`)
- Add `#[derive(Resource)] pub struct DebugOverlayConfig { pub enabled: bool }` (default `false`).
- Ensure `app.init_resource::<DebugInfoRegistry>()` and `app.init_resource::<DebugOverlayConfig>()` in `DevUtilsPlugin::build`.
- Register `command!(app, "Toggle debugging info overlay", toggle_debug_overlay)` (yields `dev>toggle_debug_overlay`), optionally updating `CommandOutput` with `"Debug overlay: on"` / `"off"`.
- Spawn `DebugOverlayRoot` UI node at `Startup`:
  - `Node { position_type: PositionType::Absolute, left: Val::Px(4.0), top: Val::Px(4.0), flex_direction: FlexDirection::Column, padding: UiRect::all(Val::Px(6.0)), ..default() }`
  - `BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5))`, `GlobalZIndex(FPS_OVERLAY_ZINDEX)`, `Pickable::IGNORE`, `Visibility::Hidden`, with a `DebugOverlayText` `Text` child.
- Add two `Update` systems in `DevUtilsPlugin`:
  1. `sync_debug_overlay_layout`:
     - Sets `Visibility` from `DebugOverlayConfig.enabled`.
     - Positions `DebugOverlayRoot` directly beneath the FPS overlay by querying `(&ComputedNode, &GlobalZIndex)` where `z.0 == FPS_OVERLAY_ZINDEX` (`Without<DebugOverlayRoot>`): when `FpsOverlayConfig.enabled` is true and a non-zero height is computed, set `node.top = Val::Px(computed.size().y * computed.inverse_scale_factor() + 4.0)`; when `FpsOverlayConfig.enabled` is false, set `node.top = Val::Px(4.0)`.
  2. `update_debug_overlay_text(world: &mut World)` (exclusive system, matching `apply_dom_ops`):
     - Returns immediately if `!world.resource::<DebugOverlayConfig>().enabled` (zero overhead when off).
     - Clones `(category, label, system_id)` list from `DebugInfoRegistry`, runs each `world.run_system(entry.system_id)`, formats grouped lines (`[category]\n  label: value`), and writes to `DebugOverlayText`'s `Text` only when the string changed.

---

### Suggested Implementation Order

1. **`docs/.compiled/commands.md`** (`src/docgen/commands_docs.rs`, `src/docgen/mod.rs`) — small, self-contained, and immediately documents every new command added in steps 2–4.
2. **Debug info registry + `MovementState` + `dev` overlay** (`src/utils/registry/debug_info.rs`, `src/character.rs`, `src/dev.rs`) — zero rendering edge cases, easy to unit-test.
3. **Camera angling** (`src/camera.rs`) — extends `CameraRig` + adds `angle_camera` / `camera_info`.
4. **Screenshot abstraction** (`src/screenshot.rs`, `src/main.rs`) — verify visually with `angle_camera` -> `screenshot` from the `~` console while paused.