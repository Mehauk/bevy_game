use avian3d::prelude::*;
use bevy::prelude::*;

use super::MovementInput;
use super::land::accelerate_toward;
use super::state::classify_water_state;
use crate::character::WaterPhysics;
use crate::character::camera_relative_direction;
use crate::character::camera_relative_direction_3d;
use crate::character::types::{Character, GroundState, MovementState, SubmersionState};

const FLOAT_TARGET_DEPTH: f32 = 1.2;
const BREACH_MAX_DEPTH: f32 = 1.5;
const SWIM_SPRINT_SCALE: f32 = 1.6;

fn breach_fires(
    jumping: bool,
    held_last_frame: bool,
    grounded: bool,
    cooldown_left: f32,
    depth: f32,
) -> bool {
    jumping && !held_last_frame && grounded && cooldown_left <= 0.0 && depth <= BREACH_MAX_DEPTH
}

fn buoyancy_accel(water: &WaterPhysics, depth: f32, vertical_velocity: f32) -> f32 {
    let lift = water.buoy_stiffness * (depth - FLOAT_TARGET_DEPTH).max(0.0);
    (lift - water.buoy_damping * vertical_velocity)
        .clamp(-water.buoy_max_accel, water.buoy_max_accel)
}

fn drag_toward_zero(value: f32, drag: f32, dt: f32) -> f32 {
    let loss = drag * value * value * dt;
    if loss >= value.abs() {
        0.0
    } else {
        value - value.signum() * loss
    }
}

fn drag_toward(value: f32, target: f32, drag: f32, dt: f32) -> f32 {
    let relative = value - target;
    let loss = drag * relative * relative * dt;
    if loss >= relative.abs() {
        target
    } else {
        value - relative.signum() * loss
    }
}

fn sprint_scale(sprinting: bool) -> f32 {
    if sprinting { SWIM_SPRINT_SCALE } else { 1.0 }
}

pub(crate) fn move_in_water(
    character: &Character,
    ground: &mut GroundState,
    submersion: &SubmersionState,
    input: &MovementInput,
    velocity: &mut LinearVelocity,
    gravity_scale: &mut GravityScale,
    dt: f32,
) -> MovementState {
    let water = &character.water;
    gravity_scale.0 = 0.0;

    if breach_fires(
        input.jumping,
        ground.jump_held_last_frame,
        ground.is_grounded,
        ground.jump_cooldown_timer,
        submersion.depth,
    ) {
        velocity.y = character.jump_speed;
        ground.is_grounded = false;
        ground.jump_cooldown_timer = character.jump_cooldown;
        return MovementState::Jumping;
    }

    let diving = input.crouching || submersion.is_fully_submerged();
    let steer = if diving {
        camera_relative_direction_3d(input.move_vec, input.yaw, input.pitch)
    } else {
        camera_relative_direction(input.move_vec, input.yaw)
    }
    .normalize_or_zero();

    let speed = water.swim_speed * sprint_scale(input.sprinting);
    let moved = accelerate_toward(
        Vec3::new(velocity.x, 0.0, velocity.z),
        Vec3::new(steer.x, 0.0, steer.z) * speed,
        water.swim_accel,
        dt,
    );
    velocity.x = moved.x;
    velocity.z = moved.z;

    if diving {
        velocity.y += steer.y * water.swim_accel * dt;
    }

    if input.jumping {
        velocity.y += buoyancy_accel(water, submersion.depth, velocity.y) * dt;
    } else if input.crouching {
        velocity.y -= water.dive_accel * sprint_scale(input.sprinting) * dt;
    } else {
        velocity.y -= water.sink_accel * dt;
    }

    velocity.0 = Vec3::new(
        drag_toward(velocity.x, water.current.x, water.drag, dt),
        drag_toward_zero(velocity.y, water.drag, dt),
        drag_toward(velocity.z, water.current.z, water.drag, dt),
    );

    if ground.is_grounded && !input.jumping && velocity.y < 0.0 {
        velocity.y = 0.0;
    }

    classify_water_state(diving, input.jumping)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buoyancy_lifts_only_from_below_the_float_line() {
        let water = WaterPhysics::default();
        assert_eq!(buoyancy_accel(&water, FLOAT_TARGET_DEPTH, 0.0), 0.0);
        assert!(
            buoyancy_accel(&water, FLOAT_TARGET_DEPTH - 0.3, 0.0) <= 0.0,
            "buoyancy never pulls you under"
        );
        assert!(buoyancy_accel(&water, 2.0, 0.0) > 0.0, "deep means lifted");
        assert_eq!(
            buoyancy_accel(&water, 100.0, 0.0),
            water.buoy_max_accel,
            "a hard clamp stops the spring launching you"
        );
        assert!(
            buoyancy_accel(&water, FLOAT_TARGET_DEPTH, 5.0) < 0.0,
            "damping opposes a rise"
        );
    }

    #[test]
    fn drag_bleeds_speed_without_crossing_zero() {
        assert_eq!(drag_toward_zero(0.0, 0.15, 0.016), 0.0);

        let plunge = drag_toward_zero(10.0, 0.15, 1.0 / 64.0);
        assert!(plunge > 0.0 && plunge < 9.9, "a fast entry bleeds hard");

        let glide = drag_toward_zero(0.2, 0.15, 1.0 / 64.0);
        assert!(
            glide > 0.0 && glide < 0.2,
            "a slow glide still loses something"
        );
        assert!(
            plunge / 10.0 < glide / 0.2,
            "drag bites harder the faster you go"
        );

        assert!(drag_toward_zero(-10.0, 0.15, 1.0 / 64.0) > -10.0);
        assert_eq!(
            drag_toward_zero(0.01, 0.15, 1000.0),
            0.0,
            "a step bigger than the speed stops at zero, never past it"
        );
    }

    #[test]
    fn breach_needs_ground_shallow_water_and_a_fresh_press() {
        assert!(breach_fires(true, false, true, 0.0, 1.0));
        assert!(
            !breach_fires(true, true, true, 0.0, 1.0),
            "held is not a press"
        );
        assert!(
            !breach_fires(true, false, false, 0.0, 1.0),
            "nothing to push off"
        );
        assert!(
            !breach_fires(true, false, true, 0.1, 1.0),
            "cooldown pending"
        );
        assert!(
            !breach_fires(true, false, true, 0.0, BREACH_MAX_DEPTH + 0.1),
            "too deep to breach"
        );
    }
}
