use bevy::prelude::*;

use crate::character::{Character, CharacterSystems, SubmersionState};
use crate::terrain::{SeaState, Terrain, terrain_gradient};

const STRAIT_DEPTH: f32 = 3.0;
const CURRENT_GAIN: f32 = 6.0;

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(super) fn register(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        sync_tidal_currents.before(CharacterSystems::Playing),
    );
}

fn sync_tidal_currents(
    sea_state: Res<SeaState>,
    terrain: Res<Terrain>,
    mut characters: Query<(&Transform, &SubmersionState, &mut Character)>,
) {
    for (transform, submersion, mut character) in &mut characters {
        let rest_depth =
            sea_state.sea_level - terrain.sample_ground_height(transform.translation.x, transform.translation.z);
        if !submersion.is_submerged() || rest_depth <= 0.0 || sea_state.level_rate.abs() < 1.0e-5 {
            character.water.current = Vec3::ZERO;
            continue;
        }
        let shallow = 1.0 - smoothstep(0.0, STRAIT_DEPTH, rest_depth);
        let gradient = terrain_gradient(&terrain, transform.translation.x, transform.translation.z);
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
        let terrain = Terrain(TerrainConfig {
            world: Vec2::splat(300.0),
            ..default()
        });
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(terrain)
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
}
