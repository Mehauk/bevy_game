use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use bevy_shader_kit::WaterMaterial;

use crate::character::{CAPSULE_RADIUS, CAPSULE_TOTAL_HEIGHT, Character, FEET_OFFSET};
use crate::ripples::{RippleField, RippleSystems};
use crate::terrain::{SeaState, WaterSurface};

const HULL_RADIUS: f32 = CAPSULE_RADIUS + 0.15;
const STRADDLE_MARGIN: f32 = 0.05;
const WAKE_MIN_SPEED: f32 = 1.2;
const WAKE_SPACING: f32 = 1.0;
const IMPACT_BASE: f32 = 0.06;
const IMPACT_GAIN: f32 = 0.02;
const WAKE_GAIN: f32 = 0.02;
const TREAD_INTERVAL: f32 = 0.7;
const TREAD_STRENGTH: f32 = 0.02;
const EXIT_BASE: f32 = 0.03;
const EXIT_GAIN: f32 = 0.02;

#[derive(Component, Default)]
struct WaterWakeTracker {
    straddling: bool,
    last_wake: Option<(Vec3, f32)>,
    last_tread: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Splash {
    Impact(f32),
    Wake(f32),
    Tread(f32),
    Exit(f32),
    None,
}

fn straddles(feet_y: f32, head_y: f32, water_y: f32) -> bool {
    feet_y < water_y + STRADDLE_MARGIN && head_y > water_y - STRADDLE_MARGIN
}

fn hull_fraction(feet_y: f32, water_y: f32) -> f32 {
    ((water_y - feet_y) / CAPSULE_TOTAL_HEIGHT).clamp(0.0, 1.0)
}

fn plan_splash(
    straddling: bool,
    was_straddling: bool,
    vertical: f32,
    horizontal_speed: f32,
    moved_since_wake: f32,
    since_tread: f32,
    hull: f32,
) -> Splash {
    if !straddling {
        if was_straddling && vertical > 0.0 {
            return Splash::Exit(EXIT_BASE + EXIT_GAIN * vertical);
        }
        return Splash::None;
    }
    if !was_straddling {
        if vertical < 0.0 {
            return Splash::Impact(IMPACT_BASE + IMPACT_GAIN * (-vertical));
        }
        return Splash::None;
    }
    if horizontal_speed > WAKE_MIN_SPEED && moved_since_wake > WAKE_SPACING {
        return Splash::Wake(WAKE_GAIN * horizontal_speed * hull);
    }
    if horizontal_speed <= WAKE_MIN_SPEED && since_tread >= TREAD_INTERVAL {
        return Splash::Tread(TREAD_STRENGTH);
    }
    Splash::None
}

pub(super) fn register(app: &mut App) {
    app.add_observer(track_character_wake).add_systems(
        Update,
        (
            emit_character_splashes.in_set(RippleSystems::Emit),
            publish_ripples.in_set(RippleSystems::Publish),
        ),
    );
}

fn track_character_wake(trigger: On<Add<Character>>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert(WaterWakeTracker::default());
}

fn emit_character_splashes(
    time: Res<Time>,
    sea_state: Res<SeaState>,
    mut characters: Query<(&Transform, &LinearVelocity, &mut WaterWakeTracker), With<Character>>,
    mut field: ResMut<RippleField>,
) {
    let now = time.elapsed_secs();
    for (transform, velocity, mut memory) in &mut characters {
        let surface =
            sea_state.surface_height_at(transform.translation.x, transform.translation.z, now);
        let feet_y = transform.translation.y - FEET_OFFSET;
        let head_y = transform.translation.y + CAPSULE_TOTAL_HEIGHT - FEET_OFFSET;
        let straddling = straddles(feet_y, head_y, surface);
        let hull = hull_fraction(feet_y, surface);
        let horizontal_speed = Vec2::new(velocity.x, velocity.z).length();
        let here = Vec3::new(transform.translation.x, 0.0, transform.translation.z);
        let moved_since_wake = memory
            .last_wake
            .map(|(last, _)| here.distance(last))
            .unwrap_or(f32::MAX);
        let since_tread = now - memory.last_tread;

        match plan_splash(
            straddling,
            memory.straddling,
            velocity.y,
            horizontal_speed,
            moved_since_wake,
            since_tread,
            hull,
        ) {
            Splash::None => {}
            Splash::Impact(strength) => {
                field.splash(transform.translation, strength, HULL_RADIUS);
                memory.last_wake = Some((here, now));
                memory.last_tread = now;
            }
            Splash::Wake(strength) => {
                if let Some((from, birth)) = memory.last_wake {
                    field.wake(from, here, birth, strength, HULL_RADIUS);
                }
                memory.last_wake = Some((here, now));
                memory.last_tread = now;
            }
            Splash::Tread(strength) => {
                field.splash(transform.translation, strength, HULL_RADIUS);
                memory.last_tread = now;
            }
            Splash::Exit(strength) => {
                field.splash(transform.translation, strength, HULL_RADIUS);
                memory.last_wake = Some((here, now));
                memory.last_tread = now;
            }
        }
        memory.straddling = straddling;
    }
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
    use bevy_shader_kit::SwellSet;

    #[test]
    fn only_the_surface_straddle_makes_waves() {
        assert!(straddles(-0.5, 1.3, 0.0), "waist deep");
        assert!(straddles(-1.7, 0.1, 0.0), "head just above");
        assert!(!straddles(-20.0, -18.2, 0.0), "on the sea floor");
        assert!(!straddles(0.5, 2.3, 0.0), "airborne");
    }

    #[test]
    fn the_hull_grows_as_the_water_rises_the_body() {
        assert!((hull_fraction(-0.9, 0.0) - 0.5).abs() < 1e-6);
        assert!((hull_fraction(-1.8, 0.0) - 1.0).abs() < 1e-6);
        assert!((hull_fraction(-20.0, 0.0) - 1.0).abs() < 1e-6);
        assert!((hull_fraction(0.5, 0.0)).abs() < 1e-6);
    }

    #[test]
    fn entering_the_water_splashes_harder_the_faster_you_fall() {
        let gentle = plan_splash(true, false, -1.0, 0.0, f32::MAX, 0.0, 1.0);
        let dive = plan_splash(true, false, -10.0, 0.0, f32::MAX, 0.0, 1.0);

        assert!(matches!(gentle, Splash::Impact(v) if (v - 0.08).abs() < 1e-6));
        assert!(matches!(dive, Splash::Impact(v) if (v - 0.26).abs() < 1e-6));
    }

    #[test]
    fn crossing_the_straddle_downward_or_upward_without_leaving_is_silent() {
        assert_eq!(plan_splash(true, false, 5.0, 0.0, f32::MAX, 0.0, 1.0), Splash::None);
        assert_eq!(plan_splash(false, true, -5.0, 0.0, f32::MAX, 0.0, 1.0), Splash::None);
        assert_eq!(plan_splash(false, false, 0.0, 0.0, f32::MAX, 9.0, 1.0), Splash::None);
    }

    #[test]
    fn breaching_out_of_the_water_splashes() {
        let exit = plan_splash(false, true, 5.0, 0.0, f32::MAX, 0.0, 1.0);
        assert!(matches!(exit, Splash::Exit(v) if (v - (EXIT_BASE + EXIT_GAIN * 5.0)).abs() < 1e-6));
    }

    #[test]
    fn wakes_need_speed_and_distance_and_scale_with_hull() {
        assert_eq!(plan_splash(true, true, 0.0, 0.5, f32::MAX, 0.0, 1.0), Splash::None);
        assert_eq!(plan_splash(true, true, 0.0, 6.0, WAKE_SPACING * 0.5, 0.0, 1.0), Splash::None);

        let full = plan_splash(true, true, 0.0, 6.0, WAKE_SPACING * 2.0, 0.0, 1.0);
        assert!(matches!(full, Splash::Wake(v) if (v - WAKE_GAIN * 6.0).abs() < 1e-6));

        let shallow = plan_splash(true, true, 0.0, 6.0, WAKE_SPACING * 2.0, 0.0, 0.5);
        assert!(matches!(shallow, Splash::Wake(v) if (v - WAKE_GAIN * 3.0).abs() < 1e-6));
    }

    #[test]
    fn treading_pulses_on_a_timer() {
        assert_eq!(plan_splash(true, true, 0.0, 0.0, f32::MAX, 0.3, 1.0), Splash::None);

        let pulse = plan_splash(true, true, 0.0, 0.0, f32::MAX, TREAD_INTERVAL, 1.0);
        assert_eq!(pulse, Splash::Tread(TREAD_STRENGTH));

        assert_eq!(plan_splash(true, true, 0.0, 6.0, f32::MAX, 9.0, 1.0), Splash::Wake(WAKE_GAIN * 6.0));
    }

    #[test]
    fn every_swimmer_keeps_its_own_wake() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RippleField>()
            .insert_resource(SeaState {
                sea_level: 0.0,
                swells: SwellSet::default(),
                ..default()
            })
            .add_observer(track_character_wake)
            .add_systems(Update, emit_character_splashes);

        for x in [0.0, 6.0] {
            app.world_mut().spawn((
                Transform::from_xyz(x, 0.0, 0.0),
                LinearVelocity(Vec3::new(0.0, -4.0, 0.0)),
                Character::default(),
            ));
        }
        app.update();
        app.update();

        let field = app.world().resource::<RippleField>();
        assert_eq!(
            field.waves.len(),
            2,
            "both swimmers enter the water and neither erases the other"
        );
    }
}
