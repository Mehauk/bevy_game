use avian3d::prelude::*;
use bevy::prelude::*;

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
            .add_systems(Startup, (setup_terrain, setup_atmosphere))
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
