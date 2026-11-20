use avian3d::prelude::*;
use bevy::{
    prelude::*,
    transform::TransformSystems,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_enhanced_input::prelude::*;

use crate::{
    input::InputAppExt,
    utils::registry::commands::{CommandAppExt, command},
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
            .add_systems(
                PostUpdate,
                sync_camera_transform.in_set(CameraSystems::UpdateTransform),
            );

        app.register_gameplay_action::<Look, _, _, _>(
            "Camera Look",
            ActionSettings::default(),
            [Binding::mouse_motion()],
            on_camera_look,
        )
        .register_gameplay_hotkey::<ToggleViewMode, _, _, _>(
            "Toggle 1st/3rd View",
            camera_hotkey(),
            [KeyCode::KeyF.with_mod_keys(ModKeys::SHIFT).into()],
            on_toggle_view_mode,
        )
        .register_gameplay_hotkey::<GrabCursor, _, _, _>(
            "Grab Cursor",
            camera_hotkey(),
            [MouseButton::Left.into()],
            on_grab_cursor,
        )
        .register_gameplay_hotkey::<ReleaseCursor, _, _, _>(
            "Unlock Cursor",
            camera_hotkey(),
            [KeyCode::Escape.into()],
            on_release_cursor,
        )
        .register_gameplay_action::<ZoomCamera, _, _, _>(
            "Zoom Camera",
            camera_hotkey(),
            [Binding::MouseWheel {
                mod_keys: ModKeys::SHIFT | ModKeys::CONTROL,
            }],
            on_camera_zoom,
        );

        command!(app, "toggle between first and third person view", toggle_view_mode);
    }
}

#[derive(Resource, Default)]
pub(crate) struct WantedCursorLock(pub(crate) bool);

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

const PITCH_LIMIT: f32 = 1.35;
const ZOOM_EPSILON: f32 = 0.01;
const CAMERA_CLIP_MARGIN: f32 = 0.05;
const MIN_CAMERA_CLIP: f32 = 0.4;

impl CameraRig {
    pub fn apply_look(&mut self, delta: Vec2) {
        self.yaw -= delta.x * self.sensitivity;
        self.pitch = (self.pitch - delta.y * self.sensitivity).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    pub fn apply_zoom(&mut self, scroll_y: f32) {
        if scroll_y.abs() < ZOOM_EPSILON {
            return;
        }

        if scroll_y > 0.0 {
            if self.view_mode == ViewMode::ThirdPerson {
                self.distance -= ZOOM_STEP;
                if self.distance < MIN_THIRD_PERSON_DIST {
                    self.distance = MIN_THIRD_PERSON_DIST;
                    self.view_mode = ViewMode::FirstPerson;
                }
            }
        } else if self.view_mode == ViewMode::FirstPerson {
            self.view_mode = ViewMode::ThirdPerson;
            self.distance = MIN_THIRD_PERSON_DIST + ZOOM_STEP;
        } else {
            self.distance = (self.distance + ZOOM_STEP).min(MAX_ZOOM_DIST);
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

fn camera_hotkey() -> ActionSettings {
    ActionSettings {
        require_reset: true,
        ..default()
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        MainCamera,
        Camera3d::default(),
        CameraRig::default(),
        Transform::default(),
    ));
}

fn on_grab_cursor(_event: On<Start<GrabCursor>>, mut wanted_lock: ResMut<WantedCursorLock>) {
    wanted_lock.0 = true;
}

fn on_release_cursor(_event: On<Start<ReleaseCursor>>, mut wanted_lock: ResMut<WantedCursorLock>) {
    wanted_lock.0 = false;
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
    rig.apply_look(event.value);
}

const MIN_THIRD_PERSON_DIST: f32 = 1.2;
const MAX_ZOOM_DIST: f32 = 36.0;
const ZOOM_STEP: f32 = 2.0;

fn on_camera_zoom(event: On<Fire<ZoomCamera>>, mut rig: Single<&mut CameraRig>) {
    rig.apply_zoom(event.value.y);
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
                    target_dist = occluded_distance(hit.distance);
                }
            }

            cam_transform.translation = eye_pivot + cam_dir * target_dist;
            cam_transform.look_at(eye_pivot, Vec3::Y);
        }
    }
}

fn occluded_distance(hit_distance: f32) -> f32 {
    (hit_distance - CAMERA_CLIP_MARGIN).max(MIN_CAMERA_CLIP)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig_at(distance: f32, mode: ViewMode) -> CameraRig {
        CameraRig {
            distance,
            view_mode: mode,
            ..default()
        }
    }

    #[test]
    fn look_scales_by_sensitivity() {
        let mut rig = CameraRig::default();
        let (yaw, pitch) = (rig.yaw, rig.pitch);
        rig.apply_look(Vec2::new(100.0, 100.0));
        assert!((rig.yaw - (yaw - 100.0 * rig.sensitivity)).abs() < 1e-6);
        assert!((rig.pitch - (pitch - 100.0 * rig.sensitivity)).abs() < 1e-6);
    }

    #[test]
    fn look_clamps_pitch_both_ways() {
        let mut rig = CameraRig::default();
        for _ in 0..10_000 {
            rig.apply_look(Vec2::new(0.0, -1000.0));
        }
        assert!((rig.pitch - PITCH_LIMIT).abs() < 1e-6, "pitch should clamp at +limit");

        let mut rig = CameraRig::default();
        for _ in 0..10_000 {
            rig.apply_look(Vec2::new(0.0, 1000.0));
        }
        assert!((rig.pitch + PITCH_LIMIT).abs() < 1e-6, "pitch should clamp at -limit");
    }

    #[test]
    fn look_leaves_yaw_unbounded() {
        let mut rig = CameraRig::default();
        rig.apply_look(Vec2::new(1e6, 0.0));
        assert!(rig.yaw.abs() > PITCH_LIMIT);
    }

    #[test]
    fn zoom_below_epsilon_is_ignored() {
        let mut rig = CameraRig::default();
        let before = rig.distance;
        rig.apply_zoom(0.0);
        rig.apply_zoom(ZOOM_EPSILON * 0.5);
        assert_eq!(rig.distance, before);
    }

    #[test]
    fn zoom_in_steps_down_then_collapses_to_first_person() {
        let mut rig = rig_at(6.0, ViewMode::ThirdPerson);
        rig.apply_zoom(1.0);
        assert_eq!(rig.distance, 6.0 - ZOOM_STEP);
        assert_eq!(rig.view_mode, ViewMode::ThirdPerson);

        rig.apply_zoom(1.0);
        assert_eq!(rig.distance, 6.0 - 2.0 * ZOOM_STEP);
        assert_eq!(rig.view_mode, ViewMode::ThirdPerson);

        rig.apply_zoom(1.0);
        assert_eq!(rig.distance, MIN_THIRD_PERSON_DIST);
        assert_eq!(rig.view_mode, ViewMode::FirstPerson);
    }

    #[test]
    fn zooming_in_further_while_first_person_does_not_go_negative() {
        let mut rig = rig_at(MIN_THIRD_PERSON_DIST, ViewMode::FirstPerson);
        for _ in 0..10 {
            rig.apply_zoom(1.0);
        }
        assert_eq!(rig.distance, MIN_THIRD_PERSON_DIST);
        assert_eq!(rig.view_mode, ViewMode::FirstPerson);
    }

    #[test]
    fn zoom_out_of_first_person_hands_back_to_third_person() {
        let mut rig = rig_at(MIN_THIRD_PERSON_DIST, ViewMode::FirstPerson);
        rig.apply_zoom(-1.0);
        assert_eq!(rig.view_mode, ViewMode::ThirdPerson);
        assert_eq!(rig.distance, MIN_THIRD_PERSON_DIST + ZOOM_STEP);
    }

    #[test]
    fn zoom_out_clamps_at_the_far_limit() {
        let mut rig = rig_at(MAX_ZOOM_DIST, ViewMode::ThirdPerson);
        for _ in 0..10 {
            rig.apply_zoom(-1.0);
        }
        assert_eq!(rig.distance, MAX_ZOOM_DIST);
        assert_eq!(rig.view_mode, ViewMode::ThirdPerson);
    }

    #[test]
    fn zoom_round_trips_through_first_person() {
        let mut rig = rig_at(6.0, ViewMode::ThirdPerson);
        for _ in 0..10 {
            rig.apply_zoom(1.0);
        }
        assert_eq!(rig.view_mode, ViewMode::FirstPerson);
        rig.apply_zoom(-1.0);
        assert_eq!(rig.view_mode, ViewMode::ThirdPerson);
        assert!(rig.distance > MIN_THIRD_PERSON_DIST);
    }

    #[test]
    fn occluded_distance_keeps_a_margin_and_a_floor() {
        assert_eq!(occluded_distance(10.0), 10.0 - CAMERA_CLIP_MARGIN);
        assert_eq!(occluded_distance(0.0), MIN_CAMERA_CLIP);
        assert_eq!(occluded_distance(0.02), MIN_CAMERA_CLIP);
        assert_eq!(
            occluded_distance(MIN_CAMERA_CLIP + CAMERA_CLIP_MARGIN),
            MIN_CAMERA_CLIP
        );
    }
}
