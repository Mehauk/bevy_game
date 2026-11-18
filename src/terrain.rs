use bevy::{mesh::VertexAttributeValues, prelude::*};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_terrain);
    }
}

fn setup_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut terrain = Mesh::from(Plane3d::default().mesh().size(200., 200.).subdivisions(100));

    if let Some(VertexAttributeValues::Float32x3(_positions)) =
        terrain.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {}

    commands.spawn((
        Mesh3d(meshes.add(terrain)),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
    ));
}