use avian3d::prelude::*;
use bevy::prelude::*;

use super::types::{CAPSULE_HEIGHT, CAPSULE_RADIUS, Character, GroundState};

pub(crate) const GROUND_PROBE_RADIUS: f32 = 0.35;
pub(crate) const GROUND_PROBE_OFFSET_Y: f32 = -(CAPSULE_HEIGHT * 0.5);
pub(crate) const GROUND_PROBE_MAX_DISTANCE: f32 = 0.5;
const GROUND_LAND_TOLERANCE: f32 = 0.04;
const GROUND_SNAP_DISTANCE: f32 = 0.35;
pub(crate) const SLOPE_SNAP_MIN_DISTANCE: f32 = 0.005;
const SLOPE_SNAP_SPEED: f32 = 10.0;
const SLOPE_SNAP_MAX_SPEED: f32 = 4.0;
pub(crate) const MIN_GROUND_NORMAL_Y: f32 = 0.01;

fn ground_touch_distance(normal_y: f32) -> f32 {
    (CAPSULE_RADIUS - GROUND_PROBE_RADIUS) / normal_y.max(MIN_GROUND_NORMAL_Y)
}

fn ground_gap(hit_distance: f32, normal_y: f32) -> f32 {
    (hit_distance - ground_touch_distance(normal_y)).max(0.0)
}

fn is_walkable(normal: Vec3, max_slope_angle_rad: f32) -> bool {
    normal.angle_between(Vec3::Y) <= max_slope_angle_rad
}

pub(crate) fn slope_tangent_y(vx: f32, vz: f32, normal: Vec3) -> f32 {
    -(vx * normal.x + vz * normal.z) / normal.y
}

pub(crate) fn slope_snap_speed(gap: f32) -> f32 {
    if gap > SLOPE_SNAP_MIN_DISTANCE {
        ((gap - SLOPE_SNAP_MIN_DISTANCE) * SLOPE_SNAP_SPEED).min(SLOPE_SNAP_MAX_SPEED)
    } else {
        0.0
    }
}

pub(crate) fn character_ground_check(
    time: Res<Time>,
    mut query: Query<(&Character, &ShapeHits, &LinearVelocity, &mut GroundState)>,
) {
    let dt = time.delta_secs();
    for (character, hits, velocity, mut ground_state) in &mut query {
        ground_state.jump_cooldown_timer = (ground_state.jump_cooldown_timer - dt).max(0.0);

        let can_probe = ground_state.jump_cooldown_timer <= 0.0
            && (ground_state.is_grounded || velocity.y <= 0.0);
        let max_gap = if ground_state.is_grounded {
            GROUND_SNAP_DISTANCE
        } else {
            GROUND_LAND_TOLERANCE
        };

        let hit = can_probe
            .then(|| {
                hits.iter_sorted().find_map(|hit| {
                    let normal = hit.normal1.normalize();
                    if !is_walkable(normal, character.max_slope_angle_rad) {
                        return None;
                    }
                    let gap = ground_gap(hit.distance, normal.y);
                    (gap <= max_gap).then_some((normal, gap))
                })
            })
            .flatten();

        if let Some((normal, gap)) = hit {
            ground_state.is_grounded = true;
            ground_state.ground_normal = normal;
            ground_state.ground_distance = gap;
        } else {
            ground_state.is_grounded = false;
            ground_state.ground_normal = Vec3::Y;
            ground_state.ground_distance = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOPE_45: Vec3 = Vec3::new(0.707_106_8, 0.707_106_8, 0.0);
    const SLOPE_60: Vec3 = Vec3::new(0.866_025_4, 0.5, 0.0);

    #[test]
    fn ground_gap_matches_the_probe_offset_on_flat_ground() {
        let touch = ground_touch_distance(1.0);
        assert!((touch - (CAPSULE_RADIUS - GROUND_PROBE_RADIUS)).abs() < 1e-6);
        assert!(
            ground_gap(touch, 1.0).abs() < 1e-6,
            "touching means zero gap"
        );
        assert!((ground_gap(touch + 0.2, 1.0) - 0.2).abs() < 1e-6);
        assert_eq!(ground_gap(0.0, 1.0), 0.0, "gap must never go negative");
    }

    #[test]
    fn walkability_respects_the_slope_limit() {
        let max = Character::default().max_slope_angle_rad;
        assert!(is_walkable(Vec3::Y, max), "flat ground");
        assert!(is_walkable(SLOPE_45, max), "45deg is under the 50deg limit");
        assert!(!is_walkable(SLOPE_60, max), "60deg is over the limit");
        assert!(!is_walkable(Vec3::X, max), "vertical wall");
    }

    #[test]
    fn slope_tangent_holds_velocity_on_the_surface() {
        assert!(
            slope_tangent_y(1.0, 0.0, Vec3::Y).abs() < 1e-9,
            "flat ground has no vertical term"
        );
        let v = slope_tangent_y(1.0, 0.0, SLOPE_45);
        assert!(
            (v + 1.0).abs() < 1e-4,
            "moving +X up a 45deg slope should give -1, got {v}"
        );
    }

    #[test]
    fn slope_snap_ramps_and_caps() {
        assert_eq!(slope_snap_speed(SLOPE_SNAP_MIN_DISTANCE), 0.0);
        assert_eq!(slope_snap_speed(SLOPE_SNAP_MIN_DISTANCE / 2.0), 0.0);
        let mid = slope_snap_speed(0.1);
        assert!(
            mid > 0.0 && mid < SLOPE_SNAP_MAX_SPEED,
            "should ramp between the bounds"
        );
        assert_eq!(
            slope_snap_speed(100.0),
            SLOPE_SNAP_MAX_SPEED,
            "must cap, or steps slam"
        );
    }
}
