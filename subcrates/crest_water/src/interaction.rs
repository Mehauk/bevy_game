use bevy::prelude::*;

use crate::buoyancy::surface_height;
use crate::inputs::{DynWaveEmitter, FoamEmitter};
use crate::query::WaterSampler;
use crate::settings::OceanSettings;

const MIN_WAKE_SPEED: f32 = 0.05;

#[derive(Component, Clone, Debug)]
pub struct SphereWaterInteraction {
    pub radius: f32,
    pub weight: f32,
    pub foam_speed_threshold: f32,
    previous_position: Option<Vec3>,
}

impl SphereWaterInteraction {
    pub fn new(radius: f32, weight: f32) -> Self {
        Self {
            radius,
            weight,
            foam_speed_threshold: 1.5,
            previous_position: None,
        }
    }
}

fn wake_strength(weight: f32, speed: f32) -> f32 {
    weight * (speed / (speed + 1.0))
}

fn near_surface(entity_y: f32, surface_y: f32, radius: f32) -> bool {
    entity_y - radius <= surface_y + radius && entity_y + radius >= surface_y - radius
}

fn update_sphere_wakes(
    time: Res<Time>,
    settings: Res<OceanSettings>,
    sampler: Res<WaterSampler>,
    mut commands: Commands,
    mut spheres: Query<(Entity, &GlobalTransform, &mut SphereWaterInteraction)>,
) {
    let dt = time.delta_secs().max(1e-5);
    for (entity, transform, mut interaction) in &mut spheres {
        let position = transform.translation();
        let velocity = interaction
            .previous_position
            .map(|previous| (position - previous) / dt)
            .unwrap_or(Vec3::ZERO);
        interaction.previous_position = Some(position);
        let surface = surface_height(&sampler, position.xz(), settings.water_level);
        let speed = velocity.xz().length();
        if !near_surface(position.y, surface, interaction.radius) || speed < MIN_WAKE_SPEED {
            commands.entity(entity).remove::<(DynWaveEmitter, FoamEmitter)>();
            continue;
        }
        let strength = wake_strength(interaction.weight, speed);
        let emitter_position = Vec3::new(position.x, surface, position.z);
        commands.entity(entity).insert(DynWaveEmitter {
            position: emitter_position,
            radius: interaction.radius,
            strength,
            velocity: velocity.xz(),
        });
        if speed >= interaction.foam_speed_threshold {
            commands.entity(entity).insert(FoamEmitter {
                position: emitter_position,
                radius: interaction.radius,
                strength,
            });
        } else {
            commands.entity(entity).remove::<FoamEmitter>();
        }
    }
}

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_sphere_wakes);
    }
}

#[cfg(test)]
mod tests {
    use bevy::time::{TimePlugin, TimeUpdateStrategy};
    use std::time::Duration;

    use super::*;

    fn test_app() -> App {
        let mut app = App::default();
        app.add_plugins(TimePlugin)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                1.0 / 60.0,
            )))
            .insert_resource(OceanSettings::default())
            .insert_resource(WaterSampler::default())
            .add_systems(Update, update_sphere_wakes);
        app
    }

    #[test]
    fn wake_strength_grows_with_weight_and_speed() {
        assert_eq!(wake_strength(0.0, 5.0), 0.0);
        assert!(wake_strength(2.0, 5.0) > wake_strength(1.0, 5.0));
        assert!(wake_strength(1.0, 5.0) > wake_strength(1.0, 1.0));
        assert!(wake_strength(1.0, 1000.0) < 1.0001);
    }

    #[test]
    fn near_surface_window() {
        assert!(near_surface(0.0, 0.0, 1.0));
        assert!(near_surface(1.5, 0.0, 1.0));
        assert!(!near_surface(5.0, 0.0, 1.0));
        assert!(!near_surface(-5.0, 0.0, 1.0));
    }

    #[test]
    fn moving_sphere_emits_wake_and_foam() {
        let mut app = test_app();
        let entity = app
            .world_mut()
            .spawn((
                GlobalTransform::from_translation(Vec3::ZERO),
                SphereWaterInteraction::new(0.5, 3.0),
            ))
            .id();
        app.update();
        assert!(app.world().get::<DynWaveEmitter>(entity).is_none());
        *app.world_mut().get_mut::<GlobalTransform>(entity).unwrap() =
            GlobalTransform::from_translation(Vec3::new(2.0, 0.0, 0.0));
        app.update();
        let emitter = app.world().get::<DynWaveEmitter>(entity).unwrap();
        assert_eq!(emitter.radius, 0.5);
        assert!(emitter.strength > 0.0);
        assert!(emitter.velocity.x > 0.0);
        assert!(app.world().get::<FoamEmitter>(entity).is_some());
    }

    #[test]
    fn sphere_far_above_water_has_no_emitters() {
        let mut app = test_app();
        let entity = app
            .world_mut()
            .spawn((
                GlobalTransform::from_translation(Vec3::ZERO),
                SphereWaterInteraction::new(0.5, 3.0),
            ))
            .id();
        app.update();
        *app.world_mut().get_mut::<GlobalTransform>(entity).unwrap() =
            GlobalTransform::from_translation(Vec3::new(2.0, 0.0, 0.0));
        app.update();
        assert!(app.world().get::<DynWaveEmitter>(entity).is_some());
        *app.world_mut().get_mut::<GlobalTransform>(entity).unwrap() =
            GlobalTransform::from_translation(Vec3::new(4.0, 20.0, 0.0));
        app.update();
        assert!(app.world().get::<DynWaveEmitter>(entity).is_none());
        assert!(app.world().get::<FoamEmitter>(entity).is_none());
    }
}
