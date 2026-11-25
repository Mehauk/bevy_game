use bevy::{
    dev_tools::fps_overlay::FpsOverlayConfig, prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_enhanced_input::prelude::*;

use crate::camera::{CameraRig, WantedCursorLock};
use crate::character::{AlignToCamera, Character, Crouch, Jump, Move, Sprint};
use crate::dev::toggle_fps_overlay;
use crate::input::{
    GameplayContextBundle, GameplayInput, HotkeyLabel, InputAppExt, MovementBinding,
};

#[derive(InputAction)]
#[action_output(bool)]
struct ToggleFpsOverlay;

#[derive(InputAction)]
#[action_output(Vec2)]
struct Look;

#[derive(InputAction)]
#[action_output(bool)]
struct ToggleViewMode;

#[derive(InputAction)]
#[action_output(bool)]
struct GrabCursor;

#[derive(InputAction)]
#[action_output(bool)]
struct ReleaseCursor;

#[derive(InputAction)]
#[action_output(Vec2)]
struct ZoomCamera;

fn hotkey() -> ActionSettings {
    ActionSettings {
        require_reset: true,
        ..default()
    }
}

pub(super) fn register(app: &mut App) {
    app.add_observer(attach_character_actions);

    app.register_gameplay_hotkey::<ToggleFpsOverlay, _, _, _>(
        "FPS Overlay",
        hotkey(),
        [KeyCode::F2.into()],
        |_event: On<Start<ToggleFpsOverlay>>, config: ResMut<FpsOverlayConfig>| {
            toggle_fps_overlay(config);
        },
    )
    .register_gameplay_action::<Look, _, _, _>(
        "Camera Look",
        ActionSettings::default(),
        [Binding::mouse_motion()],
        on_camera_look,
    )
    .register_gameplay_hotkey::<ToggleViewMode, _, _, _>(
        "Toggle 1st/3rd View",
        hotkey(),
        [KeyCode::KeyF.with_mod_keys(ModKeys::SHIFT).into()],
        on_toggle_view_mode,
    )
    .register_gameplay_hotkey::<GrabCursor, _, _, _>(
        "Grab Cursor",
        hotkey(),
        [MouseButton::Left.into()],
        on_grab_cursor,
    )
    .register_gameplay_hotkey::<ReleaseCursor, _, _, _>(
        "Unlock Cursor",
        hotkey(),
        [KeyCode::Escape.into()],
        on_release_cursor,
    )
    .register_gameplay_action::<ZoomCamera, _, _, _>(
        "Zoom Camera",
        hotkey(),
        [Binding::MouseWheel {
            mod_keys: ModKeys::SHIFT | ModKeys::CONTROL,
        }],
        on_camera_zoom,
    );
}

fn on_camera_look(
    event: On<Fire<Look>>,
    cursor_options: Single<&CursorOptions, With<PrimaryWindow>>,
    mut rig: Single<&mut CameraRig>,
) {
    if cursor_options.grab_mode != CursorGrabMode::Locked || event.value == Vec2::ZERO {
        return;
    }
    rig.apply_look(event.value);
}

fn on_camera_zoom(event: On<Fire<ZoomCamera>>, mut rig: Single<&mut CameraRig>) {
    rig.apply_zoom(event.value.y);
}

fn on_toggle_view_mode(_event: On<Start<ToggleViewMode>>, mut rig: Single<&mut CameraRig>) {
    rig.toggle_view_mode();
}

fn on_grab_cursor(_event: On<Start<GrabCursor>>, mut wanted_lock: ResMut<WantedCursorLock>) {
    wanted_lock.0 = true;
}

fn on_release_cursor(_event: On<Start<ReleaseCursor>>, mut wanted_lock: ResMut<WantedCursorLock>) {
    wanted_lock.0 = false;
}

fn attach_character_actions(trigger: On<Add<Character>>, mut commands: Commands) {
    let character = trigger.entity;
    commands
        .entity(character)
        .insert((GameplayInput, GameplayContextBundle::default()));

    let move_action = commands
        .spawn((
            Action::<Move>::new(),
            ActionOf::<GameplayInput>::new(character),
            hotkey(),
        ))
        .id();
    commands.spawn((
        Binding::from(KeyCode::KeyW),
        HotkeyLabel("Move Forward"),
        MovementBinding,
        SwizzleAxis::YXZ,
        BindingOf(move_action),
    ));
    commands.spawn((
        Binding::from(KeyCode::KeyS),
        HotkeyLabel("Move Back"),
        MovementBinding,
        Negate::all(),
        SwizzleAxis::YXZ,
        BindingOf(move_action),
    ));
    commands.spawn((
        Binding::from(KeyCode::KeyD),
        HotkeyLabel("Move Right"),
        MovementBinding,
        BindingOf(move_action),
    ));
    commands.spawn((
        Binding::from(KeyCode::KeyA),
        HotkeyLabel("Move Left"),
        MovementBinding,
        Negate::all(),
        BindingOf(move_action),
    ));

    let sprint = commands
        .spawn((
            Action::<Sprint>::new(),
            ActionOf::<GameplayInput>::new(character),
            hotkey(),
            HotkeyLabel("Sprint"),
        ))
        .id();
    commands.spawn((Binding::from(KeyCode::ShiftLeft), BindingOf(sprint)));
    commands.spawn((Binding::from(KeyCode::ShiftRight), BindingOf(sprint)));

    let align = commands
        .spawn((
            Action::<AlignToCamera>::new(),
            ActionOf::<GameplayInput>::new(character),
            hotkey(),
            HotkeyLabel("Align Camera"),
        ))
        .id();
    commands.spawn((Binding::from(KeyCode::AltLeft), BindingOf(align)));
    commands.spawn((Binding::from(KeyCode::AltRight), BindingOf(align)));

    let jump = commands
        .spawn((
            Action::<Jump>::new(),
            ActionOf::<GameplayInput>::new(character),
            hotkey(),
            HotkeyLabel("Jump"),
        ))
        .id();
    commands.spawn((
        Binding::from(KeyCode::Space),
        MovementBinding,
        BindingOf(jump),
    ));

    let crouch = commands
        .spawn((
            Action::<Crouch>::new(),
            ActionOf::<GameplayInput>::new(character),
            hotkey(),
            HotkeyLabel("Crouch"),
        ))
        .id();
    commands.spawn((Binding::from(KeyCode::ControlLeft), BindingOf(crouch)));
    commands.spawn((Binding::from(KeyCode::ControlRight), BindingOf(crouch)));
}
