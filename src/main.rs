use bevy::{
    dev_tools::fps_overlay::FpsOverlayPlugin,
    mesh::VertexAttributeValues,
    pbr::wireframe::{WireframeConfig, WireframePlugin},
    prelude::*,
};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};

use terrain::setup_terrain;
use utils::dev_utils::toggle_wireframe;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            FpsOverlayPlugin::default(),
            PanOrbitCameraPlugin,
            WireframePlugin::default(),
        ))
        .add_systems(Startup, (setup_terrain, setup,))
        .add_systems(Update, (toggle_wireframe,))
        .run();
}

fn setup(
    mut commands: Commands,
) {
    commands.spawn((
        PanOrbitCamera::default(),
        Transform::from_xyz(150.0, 110.0, 150.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        DirectionalLight::default(),
        Transform::from_translation(Vec3::ONE).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

mod utils {
    mod developer_utils {
        fn toggle_wireframe(
            keyboard_input: Res<ButtonInput<KeyCode>>,
            mut wireframe_config: ResMut<WireframeConfig>,
        ) {
            if keyboard_input.just_pressed(KeyCode::KeyZ) {
                wireframe_config.global = !wireframe_config.global;
            }
        }
    }
}

mod terrain {
    fn setup_terrain(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        let mut terrain = Mesh::from(Plane3d::default().mesh().size(200., 200.).subdivisions(100));

        if let Some(VertexAttributeValues::Float32x3(positions)) =
            terrain.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        {}

        commands.spawn((
            Mesh3d(meshes.add(terrain)),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
        ));
    }
}