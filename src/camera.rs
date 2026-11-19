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
    pub first_person: bool,
}

impl Default for CameraTarget {
    fn default() -> Self {
        Self {
            offset: Vec3::new(0.0, 0.65, 0.0),
            first_person: false,
        }
    }
}

fn setup_camera(mut commands: Commands) {
    let reset_settings = ActionSettings {
        require_reset: true,
        ..default()
    };

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

    let look_action = commands
        .spawn((
            Action::<Look>::new(),
            ActionOf::<GameplayInput>::new(cam_entity),
        ))
        .id();
    commands.spawn((Binding::mouse_motion(), BindingOf(look_action)));

    let toggle_action = commands
        .spawn((
            Action::<ToggleViewMode>::new(),
            ActionOf::<GameplayInput>::new(cam_entity),
            reset_settings,
            HotkeyLabel("Toggle 1st/3rd View"),
        ))
        .id();
    commands.spawn((
        Binding::from(KeyCode::KeyF.with_mod_keys(ModKeys::SHIFT)),
        BindingOf(toggle_action),
    ));

    let grab_action = commands
        .spawn((
            Action::<GrabCursor>::new(),
            ActionOf::<GameplayInput>::new(cam_entity),
            reset_settings,
        ))
        .id();
    commands.spawn((Binding::from(MouseButton::Left), BindingOf(grab_action)));

    let release_action = commands
        .spawn((
            Action::<ReleaseCursor>::new(),
            ActionOf::<GameplayInput>::new(cam_entity),
            reset_settings,
            HotkeyLabel("Unlock Cursor"),
        ))
        .id();
    commands.spawn((Binding::from(KeyCode::Escape), BindingOf(release_action)));
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

fn sync_camera_transform(
    spatial_query: SpatialQuery,
    camera_query: Single<(&mut CameraRig, &mut Transform), Without<CameraTarget>>,
    mut target_query: Query<(Entity, &Transform, &mut CameraTarget), Without<MainCamera>>,
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

    let Ok((_, target_transform, mut cam_target)) = target_query.get_mut(target_entity) else {
        return;
    };

    cam_target.first_person = rig.view_mode == ViewMode::FirstPerson;
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