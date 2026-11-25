use avian3d::prelude::*;
use bevy::prelude::*;

use super::MovementInput;
use super::state::classify_movement_state;
use crate::character::camera_relative_direction;
use crate::character::ground::{MIN_GROUND_NORMAL_Y, slope_snap_speed, slope_tangent_y};
use crate::character::types::{
    Character, GroundState, HORIZ_VEL_EPSILON, MovementState, horizontal_speed,
};

pub(crate) fn accelerate_toward(current: Vec3, target: Vec3, accel: f32, dt: f32) -> Vec3 {
    let diff = target - current;
    let max_change = accel * dt;
    if diff.length_squared() <= max_change * max_change {
        target
    } else {
        current + diff.normalize() * max_change
    }
}

fn jump_fires(jumping: bool, held_last_frame: bool, grounded: bool, cooldown_left: f32) -> bool {
    jumping && !held_last_frame && grounded && cooldown_left <= 0.0
}

fn air_gravity_scale(velocity_y: f32, jumping: bool, character: &Character) -> f32 {
    if velocity_y < 0.0 || !jumping {
        character.fall_gravity
    } else {
        character.base_gravity
    }
}

pub(crate) fn move_on_ground(
    character: &Character,
    ground: &mut GroundState,
    input: &MovementInput,
    velocity: &mut LinearVelocity,
    gravity_scale: &mut GravityScale,
    dt: f32,
    speed_scale: f32,
) -> MovementState {
    let move_dir = camera_relative_direction(input.move_vec, input.yaw).normalize_or_zero();
    let base_speed = if input.sprinting {
        character.sprint_speed
    } else {
        character.walk_speed
    };
    let target_horiz = move_dir * base_speed * speed_scale;

    let accel = if ground.is_grounded {
        character.acceleration
    } else {
        character.air_acceleration
    };

    let current_horiz = Vec3::new(velocity.x, 0.0, velocity.z);
    let new_horiz = accelerate_toward(current_horiz, target_horiz, accel, dt);
    velocity.x = new_horiz.x;
    velocity.z = new_horiz.z;

    if jump_fires(
        input.jumping,
        ground.jump_held_last_frame,
        ground.is_grounded,
        ground.jump_cooldown_timer,
    ) {
        velocity.y = character.jump_speed * speed_scale;
        ground.is_grounded = false;
        ground.jump_cooldown_timer = character.jump_cooldown;
        gravity_scale.0 = character.base_gravity;
    } else if ground.is_grounded {
        gravity_scale.0 = 0.0;

        let has_horiz_vel =
            velocity.x.abs() > HORIZ_VEL_EPSILON || velocity.z.abs() > HORIZ_VEL_EPSILON;
        if has_horiz_vel && ground.ground_normal.y > MIN_GROUND_NORMAL_Y {
            velocity.y = slope_tangent_y(velocity.x, velocity.z, ground.ground_normal)
                - slope_snap_speed(ground.ground_distance);
        } else {
            velocity.y = 0.0;
        }
    } else {
        gravity_scale.0 = air_gravity_scale(velocity.y, input.jumping, character);
    }

    classify_movement_state(
        ground.is_grounded,
        horizontal_speed(velocity.0),
        velocity.y,
        input.sprinting,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acceleration_is_capped_per_step() {
        let out = accelerate_toward(Vec3::ZERO, Vec3::X * 6.0, 60.0, 1.0 / 60.0);
        assert!(
            (out.x - 1.0).abs() < 1e-5,
            "should gain at most accel*dt this step, got {out}"
        );
    }

    #[test]
    fn acceleration_lands_exactly_on_target_when_close() {
        let out = accelerate_toward(Vec3::X * 5.5, Vec3::X * 6.0, 60.0, 1.0 / 60.0);
        assert_eq!(out, Vec3::X * 6.0);
    }

    #[test]
    fn acceleration_with_no_accel_is_a_no_op() {
        assert_eq!(
            accelerate_toward(Vec3::X * 3.0, Vec3::X * 6.0, 0.0, 1.0),
            Vec3::X * 3.0
        );
    }

    #[test]
    fn jump_requires_a_press_edge() {
        assert!(jump_fires(true, false, true, 0.0));
        assert!(
            !jump_fires(true, true, true, 0.0),
            "holding the key must not bunny-hop"
        );
        assert!(!jump_fires(false, false, true, 0.0));
    }

    #[test]
    fn jump_requires_ground_and_a_spent_cooldown() {
        assert!(!jump_fires(true, false, false, 0.0), "airborne");
        assert!(!jump_fires(true, false, true, 0.1), "cooldown pending");
    }

    #[test]
    fn gravity_is_heavier_once_falling_or_released() {
        let c = Character::default();
        assert_eq!(air_gravity_scale(1.0, true, &c), c.base_gravity);
        assert_eq!(air_gravity_scale(1.0, false, &c), c.fall_gravity);
        assert_eq!(air_gravity_scale(-1.0, true, &c), c.fall_gravity);
    }
}
