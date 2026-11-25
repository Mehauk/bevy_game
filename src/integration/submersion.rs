use bevy::prelude::*;

use crate::character::{Character, CharacterSystems, SubmersionState};
use crate::terrain::WaterHeightSource;

pub(super) fn register(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        sync_submersion.before(CharacterSystems::Playing),
    );
}

fn sync_submersion(
    sea: Query<&Transform, With<WaterHeightSource>>,
    mut characters: Query<(&Transform, &mut SubmersionState), With<Character>>,
) {
    let water_height = sea.iter().next().map(|transform| transform.translation.y);
    for (transform, mut submersion) in &mut characters {
        let measured = SubmersionState::measure(water_height, transform.translation.y);
        if submersion.depth != measured.depth || submersion.water_height != measured.water_height {
            *submersion = measured;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::FEET_OFFSET;

    fn measure_at(center_y: f32, water_height: Option<f32>) -> SubmersionState {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, sync_submersion);
        if let Some(height) = water_height {
            app.world_mut()
                .spawn(Transform::from_xyz(0.0, height, 0.0))
                .insert(WaterHeightSource);
        }
        let character = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, center_y, 0.0),
                Character::default(),
                SubmersionState::default(),
            ))
            .id();
        app.update();
        *app.world().get::<SubmersionState>(character).unwrap()
    }

    #[test]
    fn depth_is_measured_from_the_feet() {
        let level = measure_at(0.0, Some(0.0));
        assert!((level.depth - FEET_OFFSET).abs() < 1e-6, "waist-deep");

        let dry = measure_at(FEET_OFFSET, Some(0.0));
        assert!(dry.depth.abs() < 1e-6, "feet on the surface reads zero");

        let above = measure_at(FEET_OFFSET + 0.5, Some(0.0));
        assert!(above.depth < 0.0, "above the surface reads negative");
    }

    #[test]
    fn a_floating_character_is_submerged_but_not_fully() {
        let floating = measure_at(-0.3, Some(0.0));
        assert!(floating.is_submerged());
        assert!(!floating.is_fully_submerged());

        let sunk = measure_at(-1.5, Some(0.0));
        assert!(sunk.is_fully_submerged());
    }

    #[test]
    fn no_water_means_no_surface() {
        let dry = measure_at(-20.0, None);
        assert!(!dry.is_submerged());
        assert_eq!(dry.water_height, f32::NEG_INFINITY);
    }
}
