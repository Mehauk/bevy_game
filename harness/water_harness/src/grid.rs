use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};
use crest_water::reference::Seabed;

pub const GRID_SIZE: u32 = 129;
pub const GRID_TEXEL: f32 = 1.0;
pub const GRID_ORIGIN: f32 = -64.0;

pub fn seabed_grid_image(seabed: &Seabed) -> Image {
    let mut data = Vec::with_capacity((GRID_SIZE * GRID_SIZE * 4) as usize);
    for j in 0..GRID_SIZE {
        for i in 0..GRID_SIZE {
            let height = seabed.height(
                GRID_ORIGIN + (i as f32 + 0.5) * GRID_TEXEL,
                GRID_ORIGIN + (j as f32 + 0.5) * GRID_TEXEL,
            );
            data.extend_from_slice(&height.to_le_bytes());
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: GRID_SIZE,
            height: GRID_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::R32Float,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage |= TextureUsages::TEXTURE_BINDING;
    image
}
