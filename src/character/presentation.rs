use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;

use crate::character::camera_relative_direction;
use crate::character::types::{
    CameraFacing, Character, CharacterBody, HORIZ_VEL_EPSILON, MovementState, horizontal_speed,
};
use crate::character::{AlignToCamera, Move};

const DIVE_TILT: f32 = 0.5;
const SWIM_TILT: f32 = 0.25;
const BOB_TILT: f32 = 0.07;
const BOB_ROLL: f32 = 0.05;
const BOB_HEIGHT: f32 = 0.05;
const BOB_FREQ: f32 = 2.0;
const BOB_FOLLOW: f32 = 12.0;

fn body_tilt(state: MovementState, horiz_speed: f32, swim_speed: f32, time: f32) -> (f32, f32) {
    let effort = (horiz_speed / swim_speed.max(HORIZ_VEL_EPSILON)).clamp(0.0, 1.0);
    match state {
        MovementState::Diving => (-DIVE_TILT * (0.4 + 0.6 * effort), 0.0),
        MovementState::Swimming => (-SWIM_TILT * effort, 0.0),
        MovementState::Floating => (
            (time * BOB_FREQ).sin() * BOB_TILT,
            (time * BOB_FREQ * 0.5).sin() * BOB_ROLL,
        ),
        _ => (0.0, 0.0),
    }
}

pub(crate) fn character_rotate_body(
    time: Res<Time>,
    move_action: Single<&Action<Move>>,
    align_action: Single<&Action<AlignToCamera>>,
    facing: Single<&CameraFacing>,
    character: Single<&Character>,
    velocity: Single<&LinearVelocity, With<Character>>,
    movement_state: Single<&MovementState, With<Character>>,
    mut body_transform: Single<&mut Transform, (With<CharacterBody>, Without<Character>)>,
) {
    let dt = time.delta_secs();
    let turn_alpha = (character.turn_speed * dt).min(1.0);
    let cam_yaw = facing.yaw;
    let yaw_rot = Quat::from_rotation_y(cam_yaw);

    if facing.first_person {
        body_transform.rotation = yaw_rot;
        body_transform.translation = Vec3::ZERO;
        return;
    }

    let current_yaw = Quat::from_rotation_y(body_transform.rotation.to_euler(EulerRot::YXZ).0);
    let direction = camera_relative_direction(***move_action, cam_yaw);

    let heading = if ***align_action {
        yaw_rot
    } else if direction != Vec3::ZERO {
        let move_dir = direction.normalize();
        Quat::from_rotation_y((-move_dir.x).atan2(-move_dir.z))
    } else {
        current_yaw
    };

    let (pitch_tilt, roll_tilt) = body_tilt(
        **movement_state,
        horizontal_speed(velocity.0),
        character.water.swim_speed,
        time.elapsed_secs(),
    );
    let target = current_yaw.slerp(heading, turn_alpha)
        * Quat::from_rotation_x(pitch_tilt)
        * Quat::from_rotation_z(roll_tilt);
    body_transform.rotation = body_transform.rotation.slerp(target, turn_alpha);

    let bob = if movement_state.is_swimming() {
        (time.elapsed_secs() * BOB_FREQ).sin() * BOB_HEIGHT
    } else {
        0.0
    };
    body_transform.translation.y = body_transform
        .translation
        .y
        .lerp(bob, (BOB_FOLLOW * dt).min(1.0));
}

pub(crate) fn sync_body_visibility(
    facing: Single<&CameraFacing>,
    mut body_visibility: Single<&mut Visibility, With<CharacterBody>>,
) {
    let desired = if facing.first_person {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    if **body_visibility != desired {
        **body_visibility = desired;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diving_tilts_the_body_nose_down() {
        let (dive, _) = body_tilt(MovementState::Diving, 4.0, 4.0, 0.0);
        let (swim, _) = body_tilt(MovementState::Swimming, 4.0, 4.0, 0.0);
        let (idle_swim, _) = body_tilt(MovementState::Swimming, 0.0, 4.0, 0.0);
        let (stand, roll) = body_tilt(MovementState::Standing, 0.0, 4.0, 0.0);
        assert!(dive < 0.0, "diving pitches forward");
        assert!(swim < 0.0, "swimming pitches forward");
        assert!(dive < swim, "diving tilts harder than swimming");
        assert_eq!(idle_swim, 0.0, "treading does not pitch");
        assert_eq!((stand, roll), (0.0, 0.0), "standing stays flat");
    }

    #[test]
    fn floating_bobs_instead_of_holding_still() {
        let (low, _) = body_tilt(MovementState::Floating, 0.0, 4.0, 0.0);
        let (high, _) = body_tilt(
            MovementState::Floating,
            0.0,
            4.0,
            std::f32::consts::FRAC_PI_4 / BOB_FREQ,
        );
        assert!(high > low, "the bob swings through its cycle");
    }
}
