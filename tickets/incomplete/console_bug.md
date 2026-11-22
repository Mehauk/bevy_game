- [x] while typing in the console from topdown, the console is not visible
  - [x] systemic fix not bandaid
  - proof: `target/shots/console_normal_open.png` (console visible in third-person),
    `target/shots/console_topdown_open.png` (console AND the always-on FPS overlay both gone in top-down),
    `target/shots/console_topdown_fixed.png` (console visible in top-down after the fix),
    `target/shots/topdown_after_fix.png` (world still renders, FPS overlay back).
  - diagnosis: the top-down `WorldShotCamera` disables the `MainCamera` (`is_active = false`) but the UI
    layer is rendered by a single *default UI camera*. In Bevy 0.20 a UI root with no `UiTargetCamera`
    (the console menu root and the dev FPS overlay both have none) is assigned to
    `DefaultUiCamera::get()` = the primary-window camera with the highest `(order, entity)`, chosen
    regardless of `is_active` (`bevy_ui-0.20.1/src/ui_node.rs:3459`, `update.rs:35`). The MainCamera still
    wins that selection while inactive, so the UI is bound to a camera that renders nothing — the console
    and the FPS overlay both vanish in top-down mode.
  - fix: spawn the `WorldShotCamera` with `bevy::ui::IsDefaultUiCamera` (`src/integration/world_shot.rs`),
    Bevy's documented marker for exactly the two-camera-on-one-window case. While the shot camera exists it
    is the default UI camera and renders the console/overlays; when `world_shot off` despawns it the
    selection falls back to the MainCamera, so normal-mode UI is unchanged. Regression test in
    `world_shot_spawns_and_restores_cameras_and_cascades` asserts the shot camera carries the marker.
