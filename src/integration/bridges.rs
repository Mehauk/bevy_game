use bevy::{
    prelude::*,
    transform::TransformSystems,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_enhanced_input::prelude::*;

use crate::camera::{
    CameraRig, CameraSystems, CameraTarget, MainCamera, ViewMode, WantedCursorLock,
};
use crate::character::{CameraFacing, Character};
use crate::input::{GameplayInput, set_context_active};
use crate::menus::MenuManager;
use crate::terrain::SkyBox;

pub(super) fn register(app: &mut App) {
    app.add_systems(
        PreUpdate,
        (
            sync_context_activity.before(EnhancedInputSystems::Update),
            sync_camera_facing,
        ),
    )
    .add_observer(attach_camera_target)
    .add_systems(PostUpdate, sync_cursor_with_menus)
    .add_systems(
        PostUpdate,
        sync_skybox_to_camera
            .after(CameraSystems::UpdateTransform)
            .before(TransformSystems::Propagate),
    );
}

fn sync_camera_facing(rig: Single<&CameraRig>, mut facing: Single<&mut CameraFacing>) {
    facing.yaw = rig.yaw;
    facing.first_person = rig.view_mode == ViewMode::FirstPerson;
}

fn attach_camera_target(trigger: On<Add<Character>>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(CameraTarget::default());
}

fn sync_context_activity(
    menu_manager: Res<MenuManager>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut prev_open: Local<Option<bool>>,
    mut held_keys_on_close: Local<Vec<KeyCode>>,
    mut held_buttons_on_close: Local<Vec<MouseButton>>,
    q_gameplay: Query<Entity, With<GameplayInput>>,
    mut commands: Commands,
) {
    let open = menu_manager.any_open();
    let transitioned = *prev_open != Some(open);
    if transitioned {
        *prev_open = Some(open);
        if open {
            held_keys_on_close.clear();
            held_buttons_on_close.clear();
            for entity in &q_gameplay {
                set_context_active::<GameplayInput>(&mut commands, entity, false);
            }
            return;
        }
        *held_keys_on_close = keys.get_pressed().copied().collect();
        *held_buttons_on_close = mouse_buttons.get_pressed().copied().collect();
    }

    if !open
        && (transitioned || !held_keys_on_close.is_empty() || !held_buttons_on_close.is_empty())
    {
        held_keys_on_close.retain(|k| keys.pressed(*k));
        held_buttons_on_close.retain(|b| mouse_buttons.pressed(*b));
        if held_keys_on_close.is_empty() && held_buttons_on_close.is_empty() {
            for entity in &q_gameplay {
                set_context_active::<GameplayInput>(&mut commands, entity, true);
            }
        }
    }
}

fn sync_cursor_with_menus(
    wanted_lock: Res<WantedCursorLock>,
    menu_manager: Res<MenuManager>,
    primary_window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let (window, mut cursor_options) = primary_window.into_inner();
    let should_lock = wanted_lock.0 && !menu_manager.any_open() && window.focused;

    let (desired_visible, desired_grab) = if should_lock {
        (false, CursorGrabMode::Locked)
    } else {
        (true, CursorGrabMode::None)
    };

    if cursor_options.visible != desired_visible {
        cursor_options.visible = desired_visible;
    }
    if cursor_options.grab_mode != desired_grab {
        cursor_options.grab_mode = desired_grab;
    }
}

fn sync_skybox_to_camera(
    camera: Query<&Transform, (With<MainCamera>, Without<SkyBox>)>,
    mut skybox: Single<&mut Transform, (With<SkyBox>, Without<MainCamera>)>,
) {
    if let Some(cam_transform) = camera.iter().next() {
        skybox.translation = cam_transform.translation;
    }
}
