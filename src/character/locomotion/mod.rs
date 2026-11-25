pub mod land;
pub mod state;
pub mod water;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;

use land::move_on_ground;
use state::{Medium, classify_medium, classify_wading_state, wading_scale};
use water::move_in_water;

use crate::character::types::{
    CameraFacing, Character, GroundState, MovementState, SubmersionState,
};
use crate::character::{Crouch, Jump, Move, Sprint};

#[derive(Clone, Copy)]
pub(crate) struct MovementInput {
    pub move_vec: Vec2,
    pub sprinting: bool,
    pub jumping: bool,
    pub crouching: bool,
    pub yaw: f32,
    pub pitch: f32,
}

pub fn camera_relative_direction(move_vec: Vec2, cam_yaw: f32) -> Vec3 {
    let yaw_rot = Quat::from_rotation_y(cam_yaw);
    let forward = yaw_rot * Vec3::NEG_Z;
    let right = yaw_rot * Vec3::X;
    right * move_vec.x + forward * move_vec.y
}

pub fn camera_relative_direction_3d(move_vec: Vec2, cam_yaw: f32, cam_pitch: f32) -> Vec3 {
    let yaw_rot = Quat::from_rotation_y(cam_yaw);
    let forward = yaw_rot * Quat::from_rotation_x(cam_pitch) * Vec3::NEG_Z;
    let right = yaw_rot * Vec3::X;
    right * move_vec.x + forward * move_vec.y
}

pub(crate) fn character_physics_move(
    time: Res<Time>,
    move_action: Single<&Action<Move>>,
    sprint_action: Single<&Action<Sprint>>,
    jump_action: Single<&Action<Jump>>,
    crouch_action: Single<&Action<Crouch>>,
    facing: Single<&CameraFacing>,
    character_query: Single<(
        &Character,
        &mut GroundState,
        &SubmersionState,
        &mut LinearVelocity,
        &mut GravityScale,
        &mut MovementState,
    )>,
) {
    let (character, mut ground, submersion, mut velocity, mut gravity_scale, mut movement_state) =
        character_query.into_inner();
    let dt = time.delta_secs();

    let input = MovementInput {
        move_vec: ***move_action,
        sprinting: ***sprint_action,
        jumping: ***jump_action,
        crouching: ***crouch_action,
        yaw: facing.yaw,
        pitch: facing.pitch,
    };

    let next_state = match classify_medium(submersion.depth, movement_state.is_swimming()) {
        Medium::Water => move_in_water(
            character,
            &mut ground,
            submersion,
            &input,
            &mut velocity,
            &mut gravity_scale,
            dt,
        ),
        Medium::Wading => classify_wading_state(move_on_ground(
            character,
            &mut ground,
            &input,
            &mut velocity,
            &mut gravity_scale,
            dt,
            wading_scale(submersion.depth),
        )),
        Medium::Land => move_on_ground(
            character,
            &mut ground,
            &input,
            &mut velocity,
            &mut gravity_scale,
            dt,
            1.0,
        ),
    };

    ground.jump_held_last_frame = input.jumping;
    *movement_state = next_state;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_follows_the_camera() {
        let dir = camera_relative_direction(Vec2::new(0.0, 1.0), 0.0);
        assert!(dir.z < -0.99, "expected -Z, got {dir}");
        assert!(dir.x.abs() < 1e-6);

        let turned = camera_relative_direction(Vec2::new(0.0, 1.0), 90f32.to_radians());
        assert!(turned.x < -0.99, "expected -X, got {turned}");
    }

    #[test]
    fn pitch_steers_the_dive_vector() {
        let level = camera_relative_direction_3d(Vec2::new(0.0, 1.0), 0.0, 0.0);
        assert!(level.y.abs() < 1e-6, "level camera stays horizontal");

        let down = camera_relative_direction_3d(Vec2::new(0.0, 1.0), 0.0, -0.5);
        assert!(down.y < 0.0 && down.z < 0.0, "looking down drives you down");

        let up = camera_relative_direction_3d(Vec2::new(0.0, 1.0), 0.0, 0.5);
        assert!(up.y > 0.0, "looking up drives you up");

        let turned = camera_relative_direction_3d(Vec2::new(0.0, 1.0), 90f32.to_radians(), -0.5);
        assert!(turned.x < -0.5 && turned.y < 0.0, "yaw still steers");
    }
}
