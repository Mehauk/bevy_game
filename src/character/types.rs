use std::fmt;

use bevy::prelude::*;

pub const CHARACTER_SPAWN_OFFSET_Y: f32 = 1.5;
pub const CAPSULE_RADIUS: f32 = 0.4;
pub const CAPSULE_HEIGHT: f32 = 1.0;
pub const FEET_OFFSET: f32 = CAPSULE_HEIGHT * 0.5 + CAPSULE_RADIUS;
pub const CAPSULE_TOTAL_HEIGHT: f32 = CAPSULE_HEIGHT + CAPSULE_RADIUS * 2.0;

pub(crate) const HORIZ_VEL_EPSILON: f32 = 0.001;

#[derive(Component, Clone)]
pub struct Character {
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub jump_speed: f32,
    pub turn_speed: f32,
    pub acceleration: f32,
    pub air_acceleration: f32,
    pub jump_cooldown: f32,
    pub base_gravity: f32,
    pub fall_gravity: f32,
    pub max_slope_angle_rad: f32,
    pub water: WaterPhysics,
}

impl Default for Character {
    fn default() -> Self {
        Self {
            walk_speed: 6.0,
            sprint_speed: 12.0,
            jump_speed: 11.0,
            turn_speed: 12.0,
            acceleration: 60.0,
            air_acceleration: 15.0,
            jump_cooldown: 0.2,
            base_gravity: 2.5,
            fall_gravity: 3.5,
            max_slope_angle_rad: 50.0_f32.to_radians(),
            water: WaterPhysics::default(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WaterPhysics {
    pub swim_speed: f32,
    pub swim_accel: f32,
    pub drag: f32,
    pub buoy_stiffness: f32,
    pub buoy_damping: f32,
    pub buoy_max_accel: f32,
    pub sink_accel: f32,
    pub dive_accel: f32,
    pub current: Vec3,
}

impl Default for WaterPhysics {
    fn default() -> Self {
        Self {
            swim_speed: 4.0,
            swim_accel: 10.0,
            drag: 0.15,
            buoy_stiffness: 24.0,
            buoy_damping: 6.0,
            buoy_max_accel: 24.0,
            sink_accel: 0.8,
            dive_accel: 3.0,
            current: Vec3::ZERO,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct SubmersionState {
    pub depth: f32,
    pub water_height: f32,
}

impl Default for SubmersionState {
    fn default() -> Self {
        Self {
            depth: 0.0,
            water_height: f32::NEG_INFINITY,
        }
    }
}

impl SubmersionState {
    pub fn measure(water_height: Option<f32>, center_y: f32) -> Self {
        let Some(surface) = water_height else {
            return Self::default();
        };
        Self {
            depth: surface - (center_y - FEET_OFFSET),
            water_height: surface,
        }
    }

    pub fn is_submerged(&self) -> bool {
        self.depth > 0.0
    }

    pub fn is_fully_submerged(&self) -> bool {
        self.depth >= CAPSULE_TOTAL_HEIGHT
    }
}

#[derive(Component, Default)]
pub struct GroundState {
    pub is_grounded: bool,
    pub ground_normal: Vec3,
    pub ground_distance: f32,
    pub jump_cooldown_timer: f32,
    pub jump_held_last_frame: bool,
}

#[derive(Component, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementState {
    #[default]
    Standing,
    Walking,
    Running,
    Falling,
    Jumping,
    Wading,
    Floating,
    Swimming,
    Diving,
}

impl MovementState {
    pub fn is_swimming(self) -> bool {
        matches!(self, Self::Floating | Self::Swimming | Self::Diving)
    }
}

impl fmt::Display for MovementState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            MovementState::Standing => "standing",
            MovementState::Walking => "walking",
            MovementState::Running => "running",
            MovementState::Falling => "falling",
            MovementState::Jumping => "jumping",
            MovementState::Wading => "wading",
            MovementState::Floating => "floating",
            MovementState::Swimming => "swimming",
            MovementState::Diving => "diving",
        };
        write!(f, "{name}")
    }
}

#[derive(Component)]
pub(crate) struct CharacterBody;

#[derive(Component, Default)]
pub(crate) struct CameraFacing {
    pub yaw: f32,
    pub pitch: f32,
    pub first_person: bool,
}

pub(crate) fn horizontal_speed(velocity: Vec3) -> f32 {
    Vec2::new(velocity.x, velocity.z).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capsule_is_one_metre_eight_tall() {
        assert!((CAPSULE_TOTAL_HEIGHT - 1.8).abs() < 1e-6);
        assert!((FEET_OFFSET - 0.9).abs() < 1e-6);
    }

    #[test]
    fn submersion_measures_from_the_capsule_feet() {
        let waist = SubmersionState::measure(Some(0.0), 0.0);
        assert!((waist.depth - FEET_OFFSET).abs() < 1e-6);
        assert_eq!(waist.water_height, 0.0);
        assert!(waist.is_submerged());
        assert!(!waist.is_fully_submerged());

        let sunk = SubmersionState::measure(Some(0.0), -CAPSULE_TOTAL_HEIGHT);
        assert!(sunk.is_fully_submerged());

        let dry = SubmersionState::measure(None, -20.0);
        assert!(!dry.is_submerged());
        assert_eq!(dry.water_height, f32::NEG_INFINITY);
    }

    #[test]
    fn horizontal_speed_ignores_vertical_motion() {
        assert!((horizontal_speed(Vec3::new(3.0, -40.0, 4.0)) - 5.0).abs() < 1e-6);
        assert_eq!(horizontal_speed(Vec3::new(0.0, 9.0, 0.0)), 0.0);
    }
}
