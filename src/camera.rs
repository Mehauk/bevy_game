use avian3d::prelude::*;
use bevy::{
    prelude::*,
    transform::TransformSystems,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_enhanced_input::prelude::*;

use crate::{
    input::{ContextActive, GameplayInput, HotkeyLabel},
    menus::MenuManager,
    utils::commands::{CommandAppExt, command},
};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CameraSystems {
    UpdateTransform,
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WantedCursorLock>()
            .configure_sets(
                PostUpdate,
                CameraSystems::UpdateTransform.before(TransformSystems::Propagate),
            )
            .add_systems(Startup, setup_camera)
            .add_observer(on_grab_cursor)
            .add_observer(on_release_cursor)
            .add_observer(on_toggle_view_mode)
            .add_observer(on_camera_look)
            .add_observer(on_camera_zoom)
            .add_systems(
                PostUpdate,
                (
                    sync_cursor_with_menus,
                    sync_camera_transform.in_set(CameraSystems::UpdateTransform),
                ),
            );

        command!(
            app,
            "toggle between first and third person view",
            toggle_view_mode
        );
    }
}

#[derive(Resource, Default)]
struct WantedCursorLock(bool);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    FirstPerson,
    #[default]
    ThirdPerson,
}

#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
pub struct CameraRig {
    pub yaw: f32,
    pub pitch: f32,
    pub sensitivity: f32,
    pub distance: f32,
    pub camera_radius: f32,
    pub view_mode: ViewMode,
    pub target: Option<Entity>,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: -0.25,
            sensitivity: 0.0025,
            distance: 6.0,
            camera_radius: 0.2,
            view_mode: ViewMode::ThirdPerson,
            target: None,
        }
    }
}

#[derive(Component)]
pub struct CameraTarget {
    pub offset: Vec3,
}

impl Default for CameraTarget {
    fn default() -> Self {
        Self {
            offset: Vec3::new(0.0, 0.65, 0.0),
        }
    }
}

fn spawn_gameplay_action<A: InputAction>(
    commands: &mut Commands,
    cam_entity: Entity,
    label: &'static str,
    reset: bool,
    binding: Binding,
) {
    let action = commands
        .spawn((
            Action::<A>::new(),
            ActionOf::<GameplayInput>::new(cam_entity),
            ActionSettings {
                require_reset: reset,
                ..default()
            },
            HotkeyLabel(label),
        ))
        .id();
    commands.spawn((binding, BindingOf(action)));
}

fn setup_camera(mut commands: Commands) {
    let cam_entity = commands
        .spawn((
            MainCamera,
            Camera3d::default(),
            CameraRig::default(),
            Transform::default(),
            GameplayInput,
            ContextActive(true),
            ContextActivity::<GameplayInput>::ACTIVE,
        ))
        .id();

    spawn_gameplay_action::<Look>(
        &mut commands,
        cam_entity,
        "Camera Look",
        false,
        Binding::mouse_motion(),
    );
    spawn_gameplay_action::<ToggleViewMode>(
        &mut commands,
        cam_entity,
        "Toggle 1st/3rd View",
        true,
        Binding::from(KeyCode::KeyF.with_mod_keys(ModKeys::SHIFT)),
    );
    spawn_gameplay_action::<GrabCursor>(
        &mut commands,
        cam_entity,
        "Grab Cursor",
        true,
        Binding::from(MouseButton::Left),
    );
    spawn_gameplay_action::<ReleaseCursor>(
        &mut commands,
        cam_entity,
        "Unlock Cursor",
        true,
        Binding::from(KeyCode::Escape),
    );
    spawn_gameplay_action::<ZoomCamera>(
        &mut commands,
        cam_entity,
        "Zoom Camera",
        true,
        Binding::MouseWheel {
            mod_keys: ModKeys::SHIFT | ModKeys::CONTROL,
        },
    );
}

fn on_grab_cursor(_event: On<Start<GrabCursor>>, mut wanted_lock: ResMut<WantedCursorLock>) {
    wanted_lock.0 = true;
}

fn on_release_cursor(_event: On<Start<ReleaseCursor>>, mut wanted_lock: ResMut<WantedCursorLock>) {
    wanted_lock.0 = false;
}

fn sync_cursor_with_menus(
    wanted_lock: Res<WantedCursorLock>,
    menu_manager: Res<MenuManager>,
    mut prev_locked: Local<Option<bool>>,
    mut cursor_options: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let should_lock = wanted_lock.0 && !menu_manager.any_open();
    if *prev_locked == Some(should_lock) {
        return;
    }
    *prev_locked = Some(should_lock);

    if should_lock {
        cursor_options.visible = false;
        cursor_options.grab_mode = CursorGrabMode::Locked;
    } else {
        cursor_options.visible = true;
        cursor_options.grab_mode = CursorGrabMode::None;
    }
}

fn toggle_view_mode(mut rig: Single<&mut CameraRig>) {
    rig.view_mode = match rig.view_mode {
        ViewMode::FirstPerson => ViewMode::ThirdPerson,
        ViewMode::ThirdPerson => ViewMode::FirstPerson,
    };
}

fn on_toggle_view_mode(_event: On<Start<ToggleViewMode>>, rig: Single<&mut CameraRig>) {
    toggle_view_mode(rig);
}

fn on_camera_look(
    event: On<Fire<Look>>,
    cursor_options: Single<&CursorOptions, With<PrimaryWindow>>,
    mut rig: Single<&mut CameraRig>,
) {
    if cursor_options.grab_mode != CursorGrabMode::Locked || event.value == Vec2::ZERO {
        return;
    }
    rig.yaw -= event.value.x * rig.sensitivity;
    rig.pitch = (rig.pitch - event.value.y * rig.sensitivity).clamp(-1.35, 1.35);
}

const MIN_THIRD_PERSON_DIST: f32 = 1.2;
const MAX_ZOOM_DIST: f32 = 36.0;
const ZOOM_STEP: f32 = 2.0;

fn on_camera_zoom(
    event: On<Fire<ZoomCamera>>,
    mut rig: Single<&mut CameraRig>,
) {
    let scroll_y = event.value.y;
    if scroll_y.abs() < 0.01 {
        return;
    }

    if scroll_y > 0.0 {
        // Zoom in; going past the minimum distance collapses into first person.
        if rig.view_mode == ViewMode::ThirdPerson {
            rig.distance -= ZOOM_STEP;
            if rig.distance < MIN_THIRD_PERSON_DIST {
                rig.distance = MIN_THIRD_PERSON_DIST;
                rig.view_mode = ViewMode::FirstPerson;
            }
        }
    } else if rig.view_mode == ViewMode::FirstPerson {
        // Zoom out from first person pops back to third person at the minimum distance.
        rig.view_mode = ViewMode::ThirdPerson;
        rig.distance = MIN_THIRD_PERSON_DIST + ZOOM_STEP;
    } else {
        rig.distance = (rig.distance + ZOOM_STEP).min(MAX_ZOOM_DIST);
    }
}

fn sync_camera_transform(
    spatial_query: SpatialQuery,
    camera_query: Single<(&mut CameraRig, &mut Transform), Without<CameraTarget>>,
    target_query: Query<(Entity, &Transform, &CameraTarget), Without<MainCamera>>,
) {
    let (mut rig, mut cam_transform) = camera_query.into_inner();
    let cam_yaw_rot = Quat::from_rotation_y(rig.yaw);
    let cam_pitch_rot = Quat::from_rotation_x(rig.pitch);

    let target = rig
        .target
        .filter(|&e| target_query.contains(e))
        .or_else(|| target_query.iter().next().map(|(e, _, _)| e));

    let Some(target_entity) = target else {
        cam_transform.rotation = cam_yaw_rot * cam_pitch_rot;
        return;
    };
    rig.target = Some(target_entity);

    let Ok((_, target_transform, cam_target)) = target_query.get(target_entity) else {
        return;
    };

    let eye_pivot = target_transform.translation + cam_target.offset;

    match rig.view_mode {
        ViewMode::FirstPerson => {
            cam_transform.translation = eye_pivot;
            cam_transform.rotation = cam_yaw_rot * cam_pitch_rot;
        }
        ViewMode::ThirdPerson => {
            let cam_dir = (cam_yaw_rot * cam_pitch_rot * Vec3::Z).normalize();
            let mut target_dist = rig.distance;

            if let Ok(dir) = Dir3::new(cam_dir) {
                let filter = SpatialQueryFilter::from_excluded_entities([target_entity]);
                let sphere = Collider::sphere(rig.camera_radius);
                let config = ShapeCastConfig::from_max_distance(rig.distance);
                if let Some(hit) = spatial_query.cast_shape(
                    &sphere,
                    eye_pivot,
                    Quat::IDENTITY,
                    dir,
                    &config,
                    &filter,
                ) {
                    target_dist = (hit.distance - 0.05).max(0.4);
                }
            }

            cam_transform.translation = eye_pivot + cam_dir * target_dist;
            cam_transform.look_at(eye_pivot, Vec3::Y);
        }
    }
}