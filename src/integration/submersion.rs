use bevy::prelude::*;

use crate::character::{Character, CharacterSystems, SubmersionState};
use crate::terrain::{SeaState, Terrain};

pub(super) fn register(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        sync_submersion.before(CharacterSystems::Playing),
    );
}

fn sync_submersion(
    time: Res<Time<Virtual>>,
    sea_state: Res<SeaState>,
    terrain: Res<Terrain>,
    mut characters: Query<(&Transform, &mut SubmersionState), With<Character>>,
) {
    let now = time.elapsed_secs();
    for (transform, mut submersion) in &mut characters {
        let surface = sea_state.shore_surface_height_at(
            &terrain,
            transform.translation.x,
            transform.translation.z,
            now,
        );
        let measured = SubmersionState::measure(Some(surface), transform.translation.y);
        if submersion.depth != measured.depth || submersion.water_height != measured.water_height {
            *submersion = measured;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::FEET_OFFSET;
    use bevy::time::TimeUpdateStrategy;

    fn measure_at(center_y: f32, sea_level: f32) -> SubmersionState {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Terrain>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(
                20,
            )))
            .insert_resource(SeaState {
                sea_level,
                ..default()
            })
            .add_systems(FixedUpdate, sync_submersion);
        let character = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, center_y, 0.0),
                Character::default(),
                SubmersionState::default(),
            ))
            .id();
        for _ in 0..4 {
            app.update();
        }
        *app.world().get::<SubmersionState>(character).unwrap()
    }

    #[test]
    fn depth_is_measured_from_the_feet() {
        let waist = measure_at(0.0, 0.0);
        assert!((waist.depth - FEET_OFFSET).abs() < 1e-6, "waist-deep");

        let dry = measure_at(FEET_OFFSET, 0.0);
        assert!(dry.depth.abs() < 1e-6, "feet on the surface, swell damped at the shore");

        let above = measure_at(FEET_OFFSET + 0.5, 0.0);
        assert!(above.depth < 0.0, "above the surface reads negative");
    }

    #[test]
    fn a_floating_character_is_submerged_but_not_fully() {
        let floating = measure_at(-0.3, 0.0);
        assert!(floating.is_submerged());
        assert!(!floating.is_fully_submerged());

        let sunk = measure_at(-2.5, 0.0);
        assert!(sunk.is_fully_submerged());
    }

    #[test]
    fn deep_water_carries_the_swell() {
        let deep = measure_at(0.0, 10.0);
        let plain = 10.0 - (0.0 - FEET_OFFSET);
        assert!(
            (deep.depth - plain).abs() < 0.25,
            "swell wobble stays inside the summed swell amplitude"
        );
    }
}
