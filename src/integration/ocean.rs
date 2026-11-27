use bevy::{
    asset::RenderAssetUsages,
    pbr::StandardMaterial,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};
use crest_water::{
    OceanSettings, SeaFloorHeightGrid,
    material::{OceanSurfaceExt, OceanSurfaceMaterial, spawn_ocean_surface},
};

use crate::terrain::Terrain;

const SEABED_SAMPLES: u32 = 256;
const WATER_LEVEL: f32 = -0.1;

fn seabed_image(terrain: &Terrain) -> (Image, Vec2, f32) {
    let world = terrain.config().world;
    let texel = (world.max_element() / SEABED_SAMPLES as f32).max(f32::EPSILON);
    let origin = -world * 0.5;
    let mut data = Vec::with_capacity((SEABED_SAMPLES * SEABED_SAMPLES * 4) as usize);
    for j in 0..SEABED_SAMPLES {
        for i in 0..SEABED_SAMPLES {
            let x = origin.x + (i as f32 + 0.5) * texel;
            let z = origin.y + (j as f32 + 0.5) * texel;
            data.extend_from_slice(&terrain.sample_ground_height(x, z).to_le_bytes());
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: SEABED_SAMPLES,
            height: SEABED_SAMPLES,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::R32Float,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage |= TextureUsages::TEXTURE_BINDING;
    (image, origin, texel)
}

pub(super) fn register(app: &mut App) {
    app.add_systems(Startup, setup_ocean);
}

fn setup_ocean(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut ocean_materials: ResMut<Assets<OceanSurfaceMaterial>>,
    terrain: Res<Terrain>,
    mut settings: ResMut<OceanSettings>,
) {
    settings.water_level = WATER_LEVEL;
    let (image, origin, texel) = seabed_image(&terrain);
    let image = images.add(image);
    commands.insert_resource(SeaFloorHeightGrid {
        image,
        origin,
        texel,
        size: SEABED_SAMPLES,
        present: true,
    });
    let material = ocean_materials.add(OceanSurfaceMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.05, 0.25, 0.30),
            perceptual_roughness: 0.05,
            reflectance: 0.35,
            ..default()
        },
        extension: OceanSurfaceExt::default(),
    });
    spawn_ocean_surface(&mut commands, &mut meshes, material);
}
