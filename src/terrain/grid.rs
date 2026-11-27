use std::sync::Arc;

use bevy::prelude::*;

use super::erosion;
use super::terrain::TerrainConfig;

pub(crate) fn mix(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

#[derive(Clone, Debug)]
pub struct HeightGrid {
    resolution: UVec2,
    bounds_min: Vec2,
    texel: Vec2,
    inv_size: Vec2,
    heights: Vec<f32>,
}

impl HeightGrid {
    pub fn build(config: &TerrainConfig) -> Arc<Self> {
        let resolution = UVec2::splat(config.grid_resolution.max(2));
        let texel = config.world / resolution.as_vec2();
        let bounds_min = -config.world * 0.5;
        let mut heights = vec![0.0f32; resolution.x as usize * resolution.y as usize];
        for iz in 0..resolution.y {
            for ix in 0..resolution.x {
                let x = bounds_min.x + (ix as f32 + 0.5) * texel.x;
                let z = bounds_min.y + (iz as f32 + 0.5) * texel.y;
                heights[(iz * resolution.x + ix) as usize] =
                    config.generator.sample(config.world, x, z);
            }
        }
        erosion::apply(&config.erosion, &mut heights, resolution, bounds_min, texel);
        let size = resolution.as_vec2() * texel;
        Arc::new(Self {
            resolution,
            bounds_min,
            texel,
            inv_size: Vec2::new(1.0 / size.x, 1.0 / size.y),
            heights,
        })
    }

    #[cfg(test)]
    pub(crate) fn new(
        resolution: UVec2,
        bounds_min: Vec2,
        texel: Vec2,
        heights: Vec<f32>,
    ) -> Self {
        let size = resolution.as_vec2() * texel;
        Self {
            resolution,
            bounds_min,
            texel,
            inv_size: Vec2::new(1.0 / size.x, 1.0 / size.y),
            heights,
        }
    }

    #[cfg(test)]
    pub fn resolution(&self) -> UVec2 {
        self.resolution
    }

    #[cfg(test)]
    pub fn bounds_min(&self) -> Vec2 {
        self.bounds_min
    }

    pub fn texel(&self) -> Vec2 {
        self.texel
    }

    #[cfg(test)]
    pub fn inv_size(&self) -> Vec2 {
        self.inv_size
    }

    #[cfg(test)]
    pub fn heights(&self) -> &[f32] {
        &self.heights
    }

    pub fn sample(&self, x: f32, z: f32) -> f32 {
        bilinear(
            &self.heights,
            self.resolution,
            self.bounds_min,
            self.inv_size,
            x,
            z,
        )
    }
}

fn corner_pos(resolution: UVec2, bounds_min: Vec2, inv_size: Vec2, x: f32, z: f32) -> Vec2 {
    let resolution_f = resolution.as_vec2();
    let uv = (Vec2::new(x, z) - bounds_min) * inv_size;
    (uv * resolution_f - Vec2::splat(0.5)).clamp(Vec2::ZERO, resolution_f - Vec2::splat(1.0))
}

pub(crate) fn bilinear(
    heights: &[f32],
    resolution: UVec2,
    bounds_min: Vec2,
    inv_size: Vec2,
    x: f32,
    z: f32,
) -> f32 {
    let pos = corner_pos(resolution, bounds_min, inv_size, x, z);
    let origin = pos.floor();
    let weight = pos - origin;
    let last = resolution - UVec2::splat(1);
    let ix = origin.x as usize;
    let iz = origin.y as usize;
    let ix1 = (ix + 1).min(last.x as usize);
    let iz1 = (iz + 1).min(last.y as usize);
    let at = |cx: usize, cz: usize| heights[cz * resolution.x as usize + cx];
    let west = at(ix, iz);
    let east = at(ix1, iz);
    let north = at(ix, iz1);
    let south = at(ix1, iz1);
    mix(
        mix(west, east, weight.x),
        mix(north, south, weight.x),
        weight.y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::terrain::{Generator, TerrainConfig};

    fn asymmetric_grid(resolution: u32) -> HeightGrid {
        let texel = Vec2::splat(2.0);
        let bounds_min = -Vec2::splat(resolution as f32);
        let heights = (0..resolution)
            .flat_map(|iz| {
                (0..resolution).map(move |ix| ix as f32 * 3.0 + iz as f32 * 7.0 + 1.0)
            })
            .collect();
        HeightGrid::new(UVec2::splat(resolution), bounds_min, texel, heights)
    }

    fn corner_bilinear(grid: &HeightGrid, x: f32, z: f32) -> f32 {
        let resolution = grid.resolution().as_vec2();
        let uv = (Vec2::new(x, z) - grid.bounds_min()) * grid.inv_size();
        let pos = (uv * resolution - Vec2::splat(0.5))
            .clamp(Vec2::ZERO, resolution - Vec2::splat(1.0));
        let origin = pos.floor();
        let weight = pos - origin;
        let last = grid.resolution() - UVec2::splat(1);
        let load = |cx: f32, cz: f32| {
            let ix = (cx as usize).min(last.x as usize);
            let iz = (cz as usize).min(last.y as usize);
            grid.heights()[iz * grid.resolution().x as usize + ix]
        };
        let west = load(origin.x, origin.y);
        let east = load(origin.x + 1.0, origin.y);
        let north = load(origin.x, origin.y + 1.0);
        let south = load(origin.x + 1.0, origin.y + 1.0);
        mix(
            mix(west, east, weight.x),
            mix(north, south, weight.x),
            weight.y,
        )
    }

    #[test]
    fn the_grid_matches_the_generator_at_texel_centers() {
        let config = TerrainConfig {
            world: Vec2::splat(512.0),
            generator: Generator::Noise {
                seed: 5,
                octaves: 4,
                scale: 0.02,
            },
            ..default()
        };
        let grid = HeightGrid::build(&config);
        let texel = grid.texel();
        for iz in [0u32, 1, 37, 256, 511] {
            for ix in [0u32, 2, 100, 511] {
                let x = grid.bounds_min().x + (ix as f32 + 0.5) * texel.x;
                let z = grid.bounds_min().y + (iz as f32 + 0.5) * texel.y;
                assert_eq!(
                    grid.sample(x, z),
                    config.generator.sample(config.world, x, z),
                    "a texel center must read back exactly the seeded height"
                );
            }
        }
    }

    #[test]
    fn the_grid_sample_is_the_corner_bilinear() {
        let grid = asymmetric_grid(8);
        let texel = grid.texel();
        let mut points = vec![
            (0.0, 0.0),
            (-100.0, -100.0),
            (100.0, 100.0),
            (7.0, 7.0),
            (9.0, 9.0),
        ];
        for iz in 0..8 {
            for ix in 0..8 {
                points.push((
                    grid.bounds_min().x + (ix as f32 + 0.5) * texel.x,
                    grid.bounds_min().y + (iz as f32 + 0.5) * texel.y,
                ));
            }
        }
        for iz in 0..7 {
            for ix in 0..7 {
                points.push((
                    grid.bounds_min().x + (ix as f32 + 1.0) * texel.x,
                    grid.bounds_min().y + (iz as f32 + 1.0) * texel.y,
                ));
                points.push((
                    grid.bounds_min().x + (ix as f32 + 1.3) * texel.x,
                    grid.bounds_min().y + (iz as f32 + 1.7) * texel.y,
                ));
            }
        }
        for (x, z) in points {
            assert_eq!(
                grid.sample(x, z),
                corner_bilinear(&grid, x, z),
                "the bilinear sample must match the corner blend at ({x}, {z})"
            );
        }
    }

}
