use bevy::prelude::*;
use crest_water::{OceanSettings, WaterQuery, WaterQueryPoints};

use crate::character::{Character, CharacterSystems, SubmersionState};

#[derive(Component)]
struct WaterSlot(usize);

pub(super) fn register(app: &mut App) {
    app.add_observer(assign_water_slot);
    app.add_systems(
        FixedUpdate,
        (publish_query_points, sync_submersion)
            .chain()
            .before(CharacterSystems::Playing),
    );
}

fn assign_water_slot(trigger: On<Add<Character>>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(WaterSlot(usize::MAX));
}

fn publish_query_points(
    mut points: ResMut<WaterQueryPoints>,
    mut characters: Query<(&mut WaterSlot, &Transform), With<Character>>,
) {
    points.points.clear();
    for (mut slot, transform) in &mut characters {
        slot.0 = points.points.len();
        points.points.push(transform.translation);
    }
}

fn sync_submersion(
    settings: Res<OceanSettings>,
    query: Res<WaterQuery>,
    mut characters: Query<(&WaterSlot, &Transform, &mut SubmersionState), With<Character>>,
) {
    for (slot, transform, mut submersion) in &mut characters {
        let surface = query
            .sample(slot.0, settings.water_level)
            .map(|sample| sample.height)
            .unwrap_or(settings.water_level);
        let measured = SubmersionState::measure(Some(surface), transform.translation.y);
        if submersion.depth != measured.depth || submersion.water_height != measured.water_height {
            *submersion = measured;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{CAPSULE_HEIGHT, CAPSULE_RADIUS};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const FEET_OFFSET: f32 = CAPSULE_HEIGHT * 0.5 + CAPSULE_RADIUS;

    fn measure_at(center_y: f32, water_level: f32) -> SubmersionState {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<WaterQuery>()
            .init_resource::<WaterQueryPoints>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(20)))
            .insert_resource(OceanSettings {
                water_level,
                ..default()
            })
            .add_systems(FixedUpdate, (publish_query_points, sync_submersion).chain());
        let character = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, center_y, 0.0),
                WaterSlot(0),
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
        assert!(dry.depth.abs() < 1e-6, "feet on the surface read zero");

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
    fn feet_above_a_low_water_line_read_dry() {
        let dry = measure_at(FEET_OFFSET, -0.5);
        assert!(
            dry.depth < 0.0,
            "feet resting on the ground above a low water line read dry"
        );
        assert!(!dry.is_submerged());
    }
}
