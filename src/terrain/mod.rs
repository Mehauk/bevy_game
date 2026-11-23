use avian3d::prelude::*;
use bevy::{light::NotShadowCaster, prelude::*};

use crate::{terrain::terrain::Terrain, utils::registry::commands::{CommandAppExt, command}};

mod atmosphere;
pub mod terrain;

pub(crate) use atmosphere::SkyBox;
use atmosphere::{
    TerrainFog, setup_atmosphere, sync_terrain_fog, toggle_atmospheric_fog,
    toggle_fog_light_influence,
};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainFog>()
            .init_resource::<Terrain>()
            .add_systems(Startup, (setup_terrain, setup_water, setup_atmosphere))
            .add_systems(Update, sync_terrain_fog);

        command!(app, "Toggle atmospheric fog", toggle_atmospheric_fog);
        command!(
            app,
            "Toggle directional light fog influence",
            toggle_fog_light_influence
        );
    }
}

fn setup_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    world: Res<Terrain>,
) {
    let terrain_mesh = Mesh::from(world.into_inner());
    let terrain_collider = Collider::trimesh_from_mesh(&terrain_mesh).expect("Failed to create terrain collider");

    commands
        .spawn((
            Name::new("Terrain"),
            RigidBody::Static,
            Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
            Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
            Transform::default(),
            Mesh3d(meshes.add(terrain_mesh)),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.28, 0.42, 0.20),
                perceptual_roughness: 0.92,
                reflectance: 0.15,
                ..default()
            })),
            terrain_collider,
        ));
}

fn setup_water(    
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    world: Res<Terrain>,
) {

    commands
        .spawn((
            Name::new("Terrain"),
            Transform::from_xyz(0.0, -0.1, 0.0),
            Mesh3d(meshes.add(Plane3d::default().mesh().size(world.size().0, world.size().1))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                alpha_mode: AlphaMode::Opaque,
                specular_transmission: 1.0,
                ior: 1.33,
                perceptual_roughness: 0.04,
                reflectance: 0.5,
                thickness: 0.06,
                attenuation_distance: 2.3,
                attenuation_color: Color::srgb(0.1, 0.8, 0.4),
                ..default()
            })),
            NotShadowCaster,
        ));
}