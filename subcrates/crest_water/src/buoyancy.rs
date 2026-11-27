use avian3d::prelude::{Forces, PhysicsSystems, ReadRigidBodyForces, WriteRigidBodyForces};
use bevy::prelude::*;

use crate::query::WaterSampler;
use crate::settings::OceanSettings;

#[derive(Component, Clone, Debug)]
pub struct BoatProbes {
    pub probes: Vec<Vec3>,
    pub probe_radius: f32,
    pub density: f32,
    pub probe_volume: f32,
    pub linear_drag: f32,
    pub angular_drag: f32,
}

impl Default for BoatProbes {
    fn default() -> Self {
        Self {
            probes: Vec::new(),
            probe_radius: 0.5,
            density: 400.0,
            probe_volume: 0.5,
            linear_drag: 5.0,
            angular_drag: 2.0,
        }
    }
}

#[derive(Component, Clone, Debug)]
pub struct SimpleFloatingObject {
    pub offset: f32,
    pub bob_frequency: f32,
    pub bob_amplitude: f32,
    pub smoothing: f32,
}

impl Default for SimpleFloatingObject {
    fn default() -> Self {
        Self {
            offset: 0.0,
            bob_frequency: 0.4,
            bob_amplitude: 0.05,
            smoothing: 8.0,
        }
    }
}

pub(crate) fn surface_height(
    sampler: &WaterSampler,
    world_xz: Vec2,
    water_level: f32,
) -> f32 {
    sampler
        .sample(world_xz, water_level)
        .map(|sample| sample.height)
        .unwrap_or(water_level)
}

fn submerged_fraction(probe_y: f32, surface_y: f32, probe_radius: f32) -> f32 {
    if probe_radius <= 0.0 {
        return 0.0;
    }
    ((surface_y - probe_y) / probe_radius).clamp(0.0, 1.0)
}

fn apply_water_buoyancy(
    settings: Res<OceanSettings>,
    sampler: Res<WaterSampler>,
    mut bodies: Query<(Forces, &BoatProbes)>,
) {
    let gravity = settings.waves.gravity;
    for (mut forces, boat) in &mut bodies {
        let origin = forces.position();
        let rotation = forces.rotation();
        let mut submerged = 0.0;
        for local in &boat.probes {
            let world = origin + rotation * *local;
            let surface = surface_height(&sampler, world.xz(), settings.water_level);
            let fraction = submerged_fraction(world.y, surface, boat.probe_radius);
            if fraction <= 0.0 {
                continue;
            }
            submerged += fraction;
            let buoyancy = Vec3::Y * (boat.density * boat.probe_volume * fraction * gravity);
            forces.apply_force_at_point(buoyancy, world);
            let drag = -forces.velocity_at_point(world) * (boat.linear_drag * fraction);
            forces.apply_force_at_point(drag, world);
        }
        if submerged > 0.0 && !boat.probes.is_empty() {
            let fraction = submerged / boat.probes.len() as f32;
            let torque = -forces.angular_velocity() * (boat.angular_drag * fraction);
            forces.apply_torque(torque);
        }
    }
}

fn follow_surface(
    time: Res<Time>,
    settings: Res<OceanSettings>,
    sampler: Res<WaterSampler>,
    mut floaters: Query<(&mut Transform, &SimpleFloatingObject)>,
) {
    let dt = time.delta_secs();
    let phase = time.elapsed_secs();
    for (mut transform, floater) in &mut floaters {
        let surface = surface_height(&sampler, transform.translation.xz(), settings.water_level);
        let bob = (phase * floater.bob_frequency * std::f32::consts::TAU).sin()
            * floater.bob_amplitude;
        let target = surface + floater.offset + bob;
        let blend = 1.0 - (-floater.smoothing * dt).exp();
        transform.translation.y += (target - transform.translation.y) * blend;
    }
}

pub struct BuoyancyPlugin;

impl Plugin for BuoyancyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedPostUpdate,
            apply_water_buoyancy.in_set(PhysicsSystems::First),
        )
        .add_systems(Update, follow_surface);
    }
}

#[cfg(test)]
mod tests {
    use bevy::time::{TimePlugin, TimeUpdateStrategy};
    use std::time::Duration;

    use super::*;

    fn clock() -> TimeUpdateStrategy {
        TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 60.0))
    }

    fn flat_sampler(offset: f32) -> WaterSampler {
        let pixels = 16 * 16;
        WaterSampler {
            point: Some(Vec3::ZERO),
            origin: Vec2::new(-8.0, -8.0),
            texel: 1.0,
            height: vec![offset; pixels],
            disp_x: vec![0.0; pixels],
            disp_z: vec![0.0; pixels],
            jacobian: vec![1.0; pixels],
            flow_x: vec![0.0; pixels],
            flow_z: vec![0.0; pixels],
            depth: vec![5.0; pixels],
            ready: true,
        }
    }

    #[test]
    fn submerged_fraction_clamps() {
        assert_eq!(submerged_fraction(1.0, 0.0, 0.5), 0.0);
        assert_eq!(submerged_fraction(0.0, 0.0, 0.5), 0.0);
        assert_eq!(submerged_fraction(-0.25, 0.0, 0.5), 0.5);
        assert_eq!(submerged_fraction(-10.0, 0.0, 0.5), 1.0);
        assert_eq!(submerged_fraction(-1.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn surface_height_falls_back_to_still_water() {
        let sampler = WaterSampler::default();
        assert_eq!(surface_height(&sampler, Vec2::ZERO, 0.0), 0.0);
        assert_eq!(surface_height(&sampler, Vec2::ZERO, 3.5), 3.5);
        let sampler = flat_sampler(0.5);
        assert!((surface_height(&sampler, Vec2::ZERO, 0.0) - 0.5).abs() < 1e-5);
        assert_eq!(surface_height(&sampler, Vec2::new(100.0, 100.0), 0.0), 0.0);
    }

    #[test]
    fn floater_rises_to_surface() {
        let mut app = App::default();
        app.add_plugins(TimePlugin)
            .insert_resource(clock())
            .insert_resource(OceanSettings::default())
            .insert_resource(flat_sampler(0.5))
            .add_systems(Update, follow_surface);
        let entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, -2.0, 0.0),
                SimpleFloatingObject {
                    offset: 0.1,
                    bob_amplitude: 0.0,
                    smoothing: 10.0,
                    ..default()
                },
            ))
            .id();
        for _ in 0..60 {
            app.update();
        }
        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!((transform.translation.y - 0.6).abs() < 0.05);
    }

    #[test]
    fn floater_outside_slice_uses_water_level() {
        let mut app = App::default();
        app.add_plugins(TimePlugin)
            .insert_resource(clock())
            .insert_resource(OceanSettings {
                water_level: 2.0,
                ..default()
            })
            .insert_resource(WaterSampler::default())
            .add_systems(Update, follow_surface);
        let entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.0, 0.0),
                SimpleFloatingObject {
                    offset: 0.5,
                    bob_amplitude: 0.0,
                    smoothing: 100.0,
                    ..default()
                },
            ))
            .id();
        for _ in 0..60 {
            app.update();
        }
        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!((transform.translation.y - 2.5).abs() < 0.05);
    }
}
