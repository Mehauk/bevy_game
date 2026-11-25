use bevy::{light::NotShadowCaster, pbr::StandardMaterial, prelude::*};
use bevy_shader_kit::{
    DistanceFieldViewHidden, WaterHeightSource, WaterMaterial, WaterMaterialExt, WaterSurface,
    dummy_field_image,
};

use crate::utils::registry::commands::{CommandArgs, CommandOutput};

use super::terrain::Terrain;

#[derive(Component)]
pub struct Sea;

pub fn setup_water_surface(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut images: ResMut<Assets<Image>>,
    world: Res<Terrain>,
) {
    let dummy = dummy_field_image(&mut images);
    let material = water_materials.add(WaterMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.6, 0.75, 0.72),
            alpha_mode: AlphaMode::Opaque,
            cull_mode: None,
            specular_transmission: 1.0,
            ior: 1.333,
            perceptual_roughness: 0.04,
            reflectance: 0.35,
            thickness: 0.1,
            attenuation_distance: 2.5,
            attenuation_color: Color::srgb(0.2, 0.7, 0.5),
            ..default()
        },
        extension: WaterMaterialExt {
            field0: dummy.clone(),
            field1: dummy.clone(),
            field2: dummy.clone(),
            field3: dummy.clone(),
            ..default()
        },
    });
    commands.insert_resource(WaterSurface {
        material: material.clone(),
        dummy: dummy.clone(),
    });
    commands.spawn((
        Name::new("Sea"),
        Sea,
        WaterHeightSource,
        Transform::from_xyz(0.0, -0.1, 0.0),
        Mesh3d(
            meshes.add(
                Plane3d::default()
                    .mesh()
                    .size(world.size().0, world.size().1),
            ),
        ),
        MeshMaterial3d(material),
        NotShadowCaster,
        DistanceFieldViewHidden,
    ));
}

pub(super) fn set_water_level(
    args: Res<CommandArgs>,
    mut sea: Query<&mut Transform, With<Sea>>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(height) = args.get_f32(0) else {
        output.set("Usage: set_water_level <height>");
        return;
    };
    let Ok(mut transform) = sea.single_mut() else {
        output.set("error: no sea found");
        return;
    };
    transform.translation.y = height;
    output.set(format!("Water level: {height:.2}"));
}
