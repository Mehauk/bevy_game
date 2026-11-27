use bevy::prelude::*;

use world_water::{SeaState, terrain_gradient};

use crate::character::{Character, CharacterSystems, SubmersionState};
use crate::terrain::Terrain;
use crate::utils::registry::commands::{CommandAppExt, CommandOutput, command};

const STRAIT_DEPTH: f32 = 3.0;
const CURRENT_GAIN: f32 = 6.0;

#[derive(Resource)]
struct CurrentsToggle {
    enabled: bool,
}

impl Default for CurrentsToggle {
    fn default() -> Self {
        Self { enabled: true }
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<CurrentsToggle>()
        .add_systems(
            FixedUpdate,
            sync_tidal_currents.before(CharacterSystems::Playing),
        );

    command!(app, "Toggle tidal currents", toggle_currents);
}

fn toggle_currents(mut toggle: ResMut<CurrentsToggle>, mut output: ResMut<CommandOutput>) {
    toggle.enabled = !toggle.enabled;
    output.set(if toggle.enabled {
        "Tidal currents: on"
    } else {
        "Tidal currents: off"
    });
}

fn sync_tidal_currents(
    toggle: Res<CurrentsToggle>,
    sea_state: Res<SeaState>,
    terrain: Res<Terrain>,
    mut characters: Query<(&Transform, &SubmersionState, &mut Character)>,
) {
    for (transform, submersion, mut character) in &mut characters {
        if !toggle.enabled {
            character.water.current = Vec3::ZERO;
            continue;
        }
        let rest_depth =
            sea_state.sea_level - terrain.sample_ground_height(transform.translation.x, transform.translation.z);
        if !submersion.is_submerged() || rest_depth <= 0.0 || sea_state.level_rate.abs() < 1.0e-5 {
            character.water.current = Vec3::ZERO;
            continue;
        }
        let shallow = 1.0 - smoothstep(0.0, STRAIT_DEPTH, rest_depth);
        let gradient =
            terrain_gradient(&*terrain, transform.translation.x, transform.translation.z);
        let speed = sea_state.level_rate * CURRENT_GAIN * shallow;
        character.water.current = Vec3::new(gradient.x, 0.0, gradient.y) * speed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::FEET_OFFSET;
    use crate::terrain::terrain::TerrainConfig;
    use bevy::time::TimeUpdateStrategy;

    fn current_at(sea_level: f32, rate: f32, position: Vec3) -> Vec3 {
        current_at_with(sea_level, rate, position, true)
    }

    fn current_at_with(
        sea_level: f32,
        rate: f32,
        position: Vec3,
        currents_enabled: bool,
    ) -> Vec3 {
        let terrain = Terrain::new(TerrainConfig {
            world: Vec2::splat(300.0),
            ..default()
        });
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(terrain)
            .insert_resource(CurrentsToggle {
                enabled: currents_enabled,
            })
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(20),
            ))
            .insert_resource(SeaState {
                sea_level,
                level_rate: rate,
                ..default()
            })
            .add_systems(FixedUpdate, sync_tidal_currents);
        let character = app
            .world_mut()
            .spawn((
                Transform::from_translation(position),
                Character::default(),
                SubmersionState {
                    depth: sea_level - (position.y - FEET_OFFSET),
                    water_height: sea_level,
                },
            ))
            .id();
        for _ in 0..4 {
            app.update();
        }
        app.world()
            .get::<Character>(character)
            .unwrap()
            .water
            .current
    }

    #[test]
    fn the_flat_plain_feels_no_current() {
        let current = current_at(1.0, 0.05, Vec3::new(0.0, 0.5, 0.0));
        assert!(current.length() <= 1.0e-6);
    }

    #[test]
    fn a_rising_tide_pushes_water_up_the_rim() {
        let current = current_at(-5.0, 0.05, Vec3::new(60.0, -5.5, 0.0));
        assert!(current.x < 0.0, "the current climbs the slope toward the plain");
        assert!(current.length() > 1.0e-3);
    }

    #[test]
    fn currents_off_zero_the_flow() {
        let current = current_at_with(-5.0, 0.05, Vec3::new(60.0, -5.5, 0.0), false);
        assert_eq!(current, Vec3::ZERO);
    }
}
