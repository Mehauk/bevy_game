use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use bevy_shader_kit::CoastField;

use super::terrain::Terrain;

pub const COAST_RESOLUTION: u32 = 512;

fn sortable(level: f32) -> u32 {
    let bits = level.to_bits();
    if bits & 0x8000_0000 != 0 {
        !bits
    } else {
        bits ^ 0x8000_0000
    }
}

#[derive(Resource)]
pub struct CoastMaps {
    pub image: Handle<Image>,
    pub field: CoastField,
}

pub fn bake_coast_maps(terrain: &Terrain, images: &mut Assets<Image>) -> CoastMaps {
    let (width, height) = terrain.size();
    let res = COAST_RESOLUTION;
    let texel = Vec2::new(width / res as f32, height / res as f32);
    let bounds_min = Vec2::new(-width * 0.5, -height * 0.5);
    let count = (res * res) as usize;

    let mut ground = vec![0.0f32; count];
    for iz in 0..res {
        for ix in 0..res {
            let x = bounds_min.x + (ix as f32 + 0.5) * texel.x;
            let z = bounds_min.y + (iz as f32 + 0.5) * texel.y;
            ground[(iz * res + ix) as usize] = terrain.sample_ground_height(x, z);
        }
    }

    let mut spill = vec![f32::INFINITY; count];
    let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
    for line in 0..res {
        for edge in [0u32, res - 1] {
            for idx in [edge * res + line, line * res + edge] {
                spill[idx as usize] = ground[idx as usize];
                heap.push(Reverse((sortable(ground[idx as usize]), idx)));
            }
        }
    }
    while let Some(Reverse((key, idx))) = heap.pop() {
        if key != sortable(spill[idx as usize]) {
            continue;
        }
        let level = spill[idx as usize];
        let ix = idx % res;
        let iz = idx / res;
        for (dx, dz) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let nx = ix as i64 + dx;
            let nz = iz as i64 + dz;
            if nx < 0 || nz < 0 || nx >= res as i64 || nz >= res as i64 {
                continue;
            }
            let neighbor = (nz * res as i64 + nx) as usize;
            let candidate = ground[neighbor].max(level);
            if candidate < spill[neighbor] {
                spill[neighbor] = candidate;
                heap.push(Reverse((sortable(candidate), neighbor as u32)));
            }
        }
    }

    let mut bytes = Vec::with_capacity(count * 8);
    for index in 0..count {
        bytes.extend_from_slice(&ground[index].to_le_bytes());
        bytes.extend_from_slice(&spill[index].to_le_bytes());
    }
    let image = images.add(Image::new(
        Extent3d {
            width: res,
            height: res,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        bytes,
        TextureFormat::Rg32Float,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ));
    CoastMaps {
        image,
        field: CoastField {
            bounds_min,
            texel,
            inv_size: Vec2::new(1.0 / width, 1.0 / height),
            resolution: Vec2::new(res as f32, res as f32),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spill_of(terrain: &Terrain, images: &mut Assets<Image>) -> Vec<f32> {
        let maps = bake_coast_maps(terrain, images);
        let image = images.get(&maps.image).unwrap();
        let bytes = image.data.as_ref().unwrap();
        let count = (COAST_RESOLUTION * COAST_RESOLUTION) as usize;
        (0..count)
            .map(|i| f32::from_le_bytes(bytes[i * 8 + 4..i * 8 + 8].try_into().unwrap()))
            .collect()
    }

    #[test]
    fn the_flat_plain_spills_from_the_ocean_rim() {
        let terrain = Terrain::default();
        let mut images = Assets::<Image>::default();
        let spill = spill_of(&terrain, &mut images);
        assert!(spill.iter().all(|level| level.is_finite()));
        assert!(spill.iter().all(|level| *level <= 0.0));
    }
}
