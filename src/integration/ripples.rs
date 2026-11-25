use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use bevy_shader_kit::WaterMaterial;

use crate::character::{CAPSULE_RADIUS, Character, FEET_OFFSET};
use crate::ripples::{RippleField, RippleSystems};
use crate::terrain::{WaterHeightSource, WaterSurface};

const HULL_RADIUS: f32 = CAPSULE_RADIUS + 0.15;
const SUBMERGE_MARGIN: f32 = 0.05;
const WAKE_MIN_SPEED: f32 = 1.2;
const WAKE_SPACING: f32 = 1.0;
const IMPACT_BASE: f32 = 0.06;
const IMPACT_GAIN: f32 = 0.02;
const WAKE_GAIN: f32 = 0.02;

#[derive(Resource, Default)]
struct SplashMemory {
    submerged: bool,
    last_wake: Option<(Vec3, f32)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Splash {
    Impact(f32),
    Wake(f32),
    None,
}

fn plan_splash(
    submerged: bool,
    was_submerged: bool,
    vertical: f32,
    horizontal_speed: f32,
    moved_since_wake: f32,
) -> Splash {
    if !submerged {
        return Splash::None;
    }
    if !was_submerged {
        return Splash::Impact(IMPACT_BASE + IMPACT_GAIN * (-vertical).max(0.0));
    }
    if horizontal_speed > WAKE_MIN_SPEED && moved_since_wake > WAKE_SPACING {
        return Splash::Wake(WAKE_GAIN * horizontal_speed);
    }
    Splash::None
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<SplashMemory>()
        .add_systems(
            Update,
            (
                emit_character_splashes.in_set(RippleSystems::Emit),
                publish_ripples.in_set(RippleSystems::Publish),
            ),
        );
}

fn emit_character_splashes(
    time: Res<Time>,
    sea: Query<&Transform, With<WaterHeightSource>>,
    characters: Query<(&Transform, &LinearVelocity), With<Character>>,
    mut memory: ResMut<SplashMemory>,
    mut field: ResMut<RippleField>,
) {
    let Some(water) = sea.iter().next() else {
        return;
    };
    let Some((transform, velocity)) = characters.iter().next() else {
        return;
    };

    let now = time.elapsed_secs();
    let submerged = transform.translation.y - FEET_OFFSET < water.translation.y + SUBMERGE_MARGIN;
    let horizontal_speed = Vec2::new(velocity.x, velocity.z).length();
    let here = Vec3::new(transform.translation.x, 0.0, transform.translation.z);
    let moved_since_wake = memory
        .last_wake
        .map(|(last, _)| here.distance(last))
        .unwrap_or(f32::MAX);

    match plan_splash(
        submerged,
        memory.submerged,
        velocity.y,
        horizontal_speed,
        moved_since_wake,
    ) {
        Splash::None => {}
        Splash::Impact(strength) => {
            field.splash(transform.translation, strength, HULL_RADIUS);
            memory.last_wake = Some((here, now));
        }
        Splash::Wake(strength) => {
            if let Some((from, birth)) = memory.last_wake {
                field.wake(from, here, birth, strength, HULL_RADIUS);
            }
            memory.last_wake = Some((here, now));
        }
    }
    memory.submerged = submerged;
}

fn publish_ripples(
    field: Res<RippleField>,
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
) {
    let Some(surface) = surface else {
        return;
    };
    let Some(mut material) = materials.get_mut(&surface.material) else {
        return;
    };
    material.extension.waves = field.set();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entering_the_water_splashes_harder_the_faster_you_fall() {
        let gentle = plan_splash(true, false, -1.0, 0.0, f32::MAX);
        let dive = plan_splash(true, false, -10.0, 0.0, f32::MAX);

        assert!(matches!(gentle, Splash::Impact(v) if (v - 0.08).abs() < 1e-6));
        assert!(matches!(dive, Splash::Impact(v) if (v - 0.26).abs() < 1e-6));
    }

    #[test]
    fn rising_back_out_of_the_water_is_silent() {
        assert_eq!(plan_splash(true, false, 5.0, 0.0, f32::MAX), Splash::Impact(IMPACT_BASE));
        assert_eq!(plan_splash(false, true, -5.0, 8.0, f32::MAX), Splash::None);
    }

    #[test]
    fn wakes_need_speed_and_distance() {
        assert_eq!(plan_splash(true, true, 0.0, 0.5, f32::MAX), Splash::None);
        assert_eq!(plan_splash(true, true, 0.0, 6.0, WAKE_SPACING * 0.5), Splash::None);

        let wake = plan_splash(true, true, 0.0, 6.0, WAKE_SPACING * 2.0);
        assert!(matches!(wake, Splash::Wake(v) if (v - WAKE_GAIN * 6.0).abs() < 1e-6));
    }
}
