use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;

use bevy::prelude::*;

use super::erosion;
use super::terrain::TerrainConfig;

pub(crate) fn mix(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

fn sortable(level: f32) -> u32 {
    let bits = level.to_bits();
    if bits & 0x8000_0000 != 0 {
        !bits
    } else {
        bits ^ 0x8000_0000
    }
}

fn compute_spill(ground: &[f32], resolution: UVec2) -> Vec<f32> {
    let count = ground.len();
    let mut spill = vec![f32::INFINITY; count];
    let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
    for iz in 0..resolution.y {
        for ix in [0, resolution.x - 1] {
            let index = (iz * resolution.x + ix) as usize;
            spill[index] = ground[index];
            heap.push(Reverse((sortable(ground[index]), index as u32)));
        }
    }
    for ix in 0..resolution.x {
        for iz in [0, resolution.y - 1] {
            let index = (iz * resolution.x + ix) as usize;
            spill[index] = ground[index];
            heap.push(Reverse((sortable(ground[index]), index as u32)));
        }
    }
    while let Some(Reverse((key, idx))) = heap.pop() {
        if key != sortable(spill[idx as usize]) {
            continue;
        }
        let level = spill[idx as usize];
        let ix = idx % resolution.x;
        let iz = idx / resolution.x;
        for (dx, dz) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let nx = ix as i64 + dx;
            let nz = iz as i64 + dz;
            if nx < 0 || nz < 0 || nx >= resolution.x as i64 || nz >= resolution.y as i64 {
                continue;
            }
            let neighbor = (nz * resolution.x as i64 + nx) as usize;
            let candidate = ground[neighbor].max(level);
            if candidate < spill[neighbor] {
                spill[neighbor] = candidate;
                heap.push(Reverse((sortable(candidate), neighbor as u32)));
            }
        }
    }
    spill
}

#[derive(Clone, Debug)]
pub struct HeightGrid {
    resolution: UVec2,
    bounds_min: Vec2,
    texel: Vec2,
    inv_size: Vec2,
    heights: Vec<f32>,
    spill: Vec<f32>,
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
        let spill = compute_spill(&heights, resolution);
        let size = resolution.as_vec2() * texel;
        Arc::new(Self {
            resolution,
            bounds_min,
            texel,
            inv_size: Vec2::new(1.0 / size.x, 1.0 / size.y),
            heights,
            spill,
        })
    }

    #[cfg(test)]
    pub(crate) fn new(
        resolution: UVec2,
        bounds_min: Vec2,
        texel: Vec2,
        heights: Vec<f32>,
    ) -> Self {
        let spill = compute_spill(&heights, resolution);
        let size = resolution.as_vec2() * texel;
        Self {
            resolution,
            bounds_min,
            texel,
            inv_size: Vec2::new(1.0 / size.x, 1.0 / size.y),
            heights,
            spill,
        }
    }

    pub fn resolution(&self) -> UVec2 {
        self.resolution
    }

    pub fn bounds_min(&self) -> Vec2 {
        self.bounds_min
    }

    pub fn texel(&self) -> Vec2 {
        self.texel
    }

    pub fn inv_size(&self) -> Vec2 {
        self.inv_size
    }

    pub fn heights(&self) -> &[f32] {
        &self.heights
    }

    pub fn spill(&self) -> &[f32] {
        &self.spill
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

    pub fn sample_spill(&self, x: f32, z: f32) -> f32 {
        let origin = corner_pos(self.resolution, self.bounds_min, self.inv_size, x, z).floor();
        let last = self.resolution - UVec2::splat(1);
        let ix = origin.x as usize;
        let iz = origin.y as usize;
        let ix1 = (ix + 1).min(last.x as usize);
        let iz1 = (iz + 1).min(last.y as usize);
        let at = |cx: usize, cz: usize| self.spill[cz * self.resolution.x as usize + cx];
        at(ix, iz)
            .min(at(ix1, iz))
            .min(at(ix, iz1))
            .min(at(ix1, iz1))
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

    fn shader_mirror(grid: &HeightGrid, x: f32, z: f32) -> f32 {
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
    fn the_grid_mirrors_the_coast_shader_exactly() {
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
                shader_mirror(&grid, x, z),
                "the cpu sample must bit-match the coast.wesl bilinear at ({x}, {z})"
            );
        }
    }

    #[test]
    fn the_spill_sample_mirrors_the_coast_shader() {
        let grid = asymmetric_grid(8);
        let spill_mirror = |x: f32, z: f32| -> f32 {
            let resolution = grid.resolution().as_vec2();
            let uv = (Vec2::new(x, z) - grid.bounds_min()) * grid.inv_size();
            let origin = (uv * resolution - Vec2::splat(0.5))
                .clamp(Vec2::ZERO, resolution - Vec2::splat(1.0))
                .floor();
            let last = grid.resolution() - UVec2::splat(1);
            let load = |cx: f32, cz: f32| {
                let ix = (cx as usize).min(last.x as usize);
                let iz = (cz as usize).min(last.y as usize);
                grid.spill()[iz * grid.resolution().x as usize + ix]
            };
            load(origin.x, origin.y)
                .min(load(origin.x + 1.0, origin.y))
                .min(load(origin.x, origin.y + 1.0))
                .min(load(origin.x + 1.0, origin.y + 1.0))
        };
        for iz in 0..9 {
            for ix in 0..9 {
                let x = grid.bounds_min().x + ix as f32 * grid.texel().x;
                let z = grid.bounds_min().y + iz as f32 * grid.texel().y;
                assert_eq!(
                    grid.sample_spill(x, z),
                    spill_mirror(x, z),
                    "the cpu spill sample must bit-match spill_height_at at ({x}, {z})"
                );
            }
        }
    }

    #[test]
    fn spill_never_sits_below_its_own_ground() {
        let grid = asymmetric_grid(8);
        for index in 0..grid.heights().len() {
            assert!(grid.spill()[index] >= grid.heights()[index]);
        }
    }
}
