use avian3d::prelude::*;
use bevy::prelude::*;

use crate::utils::SpawnGroundHeight;
use crate::utils::registry::commands::{CommandAppExt, command};

mod atmosphere;

pub(crate) use atmosphere::SkyBox;
use atmosphere::{
    TerrainFog, setup_atmosphere, sync_terrain_fog, toggle_atmospheric_fog,
    toggle_fog_light_influence,
};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainFog>()
            .init_resource::<SpawnGroundHeight>()
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

const GROUND_EXTENT: f32 = 400.0;

fn setup_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Name::new("Terrain"),
        RigidBody::Static,
        Collider::half_space(Vec3::Y),
        Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
        Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
        Transform::default(),
        Mesh3d(meshes.add(Mesh::from(
            Plane3d::default().mesh().size(GROUND_EXTENT, GROUND_EXTENT),
        ))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.28, 0.42, 0.20),
            perceptual_roughness: 0.92,
            reflectance: 0.15,
            ..default()
        })),
    ));
}
