use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_shader_kit::DistanceFieldView;

use crate::{
    terrain::terrain::Terrain,
    utils::registry::commands::{CommandAppExt, CommandOutput, command},
    utils::registry::debug_info::{DebugInfoAppExt, debug_info},
};

mod atmosphere;
pub mod terrain;
mod water;

pub(crate) use atmosphere::SkyBox;
use atmosphere::{
    TerrainFog, setup_atmosphere, sync_terrain_fog, toggle_atmospheric_fog,
    toggle_fog_light_influence,
};
pub use bevy_shader_kit::{DistanceFieldSource, PrimaryField};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainFog>()
            .init_resource::<Terrain>()
            .add_systems(Startup, (setup_terrain, setup_atmosphere))
            .add_systems(Update, sync_terrain_fog);

        app.add_systems(Startup, water::setup_water_surface);

        command!(app, "Toggle atmospheric fog", toggle_atmospheric_fog);
        command!(
            app,
            "Toggle directional light fog influence",
            toggle_fog_light_influence
        );
        command!(app, "Toggle distance field view", toggle_distance_field);
        command!(app, "Set the water level", water::set_water_level);
        debug_info!(app, "Distance Field", distance_field_debug_info);
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
            PrimaryField,
            DistanceFieldSource {
                clay_alpha: 0.35,
                water_margin: 0.0,
                ..DistanceFieldSource::prop()
            },
        ));
}

fn toggle_distance_field(
    mut view: ResMut<DistanceFieldView>,
    mut output: ResMut<CommandOutput>,
) {
    output.set(if view.toggle() {
        "Distance field view: on"
    } else {
        "Distance field view: off"
    });
}

fn distance_field_debug_info(
    view: Res<DistanceFieldView>,
    primary: Query<&DistanceFieldSource, With<PrimaryField>>,
) -> Option<String> {
    let source = primary.iter().next()?;
    let r = source.params.resolution;
    Some(format!(
        "{} {}x{}x{}",
        if view.active { "on" } else { "off" },
        r.x as i32,
        r.y as i32,
        r.z as i32,
    ))
}
