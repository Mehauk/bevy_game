use bevy::prelude::{Resource, Vec2};
use glam27::DVec3 as VoroDVec3;
use meshless_voronoi::VoronoiIntegrator;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

#[derive(Clone, Debug)]
pub struct TerrainCell {
    pub id: usize,
    pub center: Vec2,
    pub base_height: f32,
}

#[derive(Resource, Clone)]
pub struct TerrainMap {
    pub size: f32,
    pub subdivisions: u32,
    pub amplitude: f32,
    pub cell_grid_dim: u32,
    pub cell_base_height_range: f32,
    pub cell_blend_width: f32,
    pub cell_seed: u64,
    cells: Vec<TerrainCell>,
    generation: u64,
    fbm: Fbm<Perlin>,
}

impl Default for TerrainMap {
    fn default() -> Self {
        let fbm = Fbm::<Perlin>::new(42)
            .set_octaves(4)
            .set_frequency(0.009)
            .set_persistence(0.42)
            .set_lacunarity(2.0);

        let mut map = Self {
            size: 400.0,
            subdivisions: 200,
            amplitude: 22.0,
            cell_grid_dim: 8,
            cell_base_height_range: 4.0,
            cell_blend_width: 8.0,
            cell_seed: 1337,
            cells: Vec::new(),
            generation: 0,
            fbm,
        };
        map.rebuild_voronoi_cells();
        map
    }
}

impl TerrainMap {
    pub fn rebuild_voronoi_cells(&mut self) {
        let dim = self.cell_grid_dim.max(1);
        let half = self.size * 0.5;
        let cell_span = self.size / dim as f32;

        let mut generators = Vec::with_capacity((dim * dim) as usize);
        for gz in 0..dim {
            for gx in 0..dim {
                let idx = (gz * dim + gx) as u64;
                let jx = 0.15 + 0.70 * hash_unit_f32(self.cell_seed, idx, 0);
                let jz = 0.15 + 0.70 * hash_unit_f32(self.cell_seed, idx, 1);
                let x = -half + (gx as f32 + jx) * cell_span;
                let z = -half + (gz as f32 + jz) * cell_span;
                generators.push(VoroDVec3::new(x as f64, z as f64, 0.0));
            }
        }

        let anchor = VoroDVec3::new(-half as f64, -half as f64, -0.5);
        let width = VoroDVec3::new(self.size as f64, self.size as f64, 1.0);
        let voronoi = VoronoiIntegrator::build(
            &generators,
            None,
            anchor,
            width,
            2.try_into().unwrap(),
            false,
        );

        let mut cells = Vec::with_capacity(generators.len());
        for i in 0..generators.len() {
            if let Some(cell) = voronoi.get_cell_at(i) {
                let base_height = (hash_unit_f32(self.cell_seed, cell.idx as u64, 2) * 2.0 - 1.0)
                    * self.cell_base_height_range;
                cells.push(TerrainCell {
                    id: cell.idx,
                    center: Vec2::new(cell.loc.x as f32, cell.loc.y as f32),
                    base_height,
                });
            }
        }

        self.cells = cells;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn cells(&self) -> &[TerrainCell] {
        &self.cells
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn cell_at(&self, x: f32, z: f32) -> Option<&TerrainCell> {
        let p = Vec2::new(x, z);
        self.cells
            .iter()
            .min_by(|a, b| p.distance_squared(a.center).total_cmp(&p.distance_squared(b.center)))
    }

    pub fn set_cell_base_height(&mut self, cell_id: usize, base_height: f32) -> bool {
        let Some(cell) = self.cells.iter_mut().find(|c| c.id == cell_id) else {
            return false;
        };
        cell.base_height = base_height;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    pub fn cell_base_height_at(&self, x: f32, z: f32) -> f32 {
        if self.cells.is_empty() {
            return 0.0;
        }
        let p = Vec2::new(x, z);

        let mut min_dist = f32::INFINITY;
        let mut nearest_height = 0.0;
        for cell in &self.cells {
            let d = p.distance(cell.center);
            if d < min_dist {
                min_dist = d;
                nearest_height = cell.base_height;
            }
        }

        if self.cell_blend_width <= 0.0 {
            return nearest_height;
        }

        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;
        for cell in &self.cells {
            let delta = p.distance(cell.center) - min_dist;
            if delta < self.cell_blend_width {
                let u = (delta / self.cell_blend_width).clamp(0.0, 1.0);
                let w = 1.0 - (3.0 * u * u - 2.0 * u * u * u);
                weighted_sum += cell.base_height * w;
                total_weight += w;
            }
        }

        if total_weight > 0.0 {
            weighted_sum / total_weight
        } else {
            nearest_height
        }
    }

    pub fn max_height_extent(&self) -> f32 {
        (self.amplitude + self.cell_base_height_range).max(1.0)
    }

    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let broad = self.fbm.get([x as f64, z as f64]) as f32;
        let swell = ((x * 0.012).sin() + (z * 0.012).cos()) * 0.15;
        let noise_height = (broad + swell) * self.amplitude;
        noise_height + self.cell_base_height_at(x, z)
    }
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn hash_unit_f32(seed: u64, index: u64, channel: u64) -> f32 {
    let h = splitmix64(
        seed ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ channel.wrapping_mul(0xbf58_476d_1ce4_e5b9),
    );
    ((h >> 40) as f32) / ((1u64 << 24) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_height_is_deterministic_and_varied() {
        let map = TerrainMap::default();
        assert_eq!(
            map.height_at(12.5, -3.25),
            map.height_at(12.5, -3.25),
            "height must be a pure function of x,z"
        );
        assert!(map.height_at(12.5, -3.25).is_finite());
        assert_ne!(map.height_at(0.0, 0.0), map.height_at(40.0, 40.0), "terrain must vary");
    }

    #[test]
    fn voronoi_cells_cover_map_with_positive_and_negative_base_heights() {
        let map = TerrainMap::default();
        let expected_count = (map.cell_grid_dim * map.cell_grid_dim) as usize;
        assert_eq!(map.cells().len(), expected_count);

        let half = map.size * 0.5;
        for cell in map.cells() {
            assert!(cell.center.x >= -half && cell.center.x <= half);
            assert!(cell.center.y >= -half && cell.center.y <= half);
            assert!(cell.base_height.abs() <= map.cell_base_height_range);
        }

        assert!(map.cells().iter().any(|c| c.base_height < 0.0), "must have negative cell heights");
        assert!(map.cells().iter().any(|c| c.base_height > 0.0), "must have positive cell heights");
    }

    #[test]
    fn cell_base_height_blends_smoothly_and_hard_steps_when_zero_blend() {
        let mut map = TerrainMap {
            size: 100.0,
            subdivisions: 10,
            amplitude: 0.0,
            cell_grid_dim: 2,
            cell_base_height_range: 10.0,
            cell_blend_width: 4.0,
            ..Default::default()
        };
        map.cells = vec![
            TerrainCell {
                id: 0,
                center: Vec2::new(-20.0, 0.0),
                base_height: -8.0,
            },
            TerrainCell {
                id: 1,
                center: Vec2::new(20.0, 0.0),
                base_height: 12.0,
            },
        ];

        assert!((map.cell_base_height_at(-20.0, 0.0) - (-8.0)).abs() < 1e-5);
        assert!((map.cell_base_height_at(20.0, 0.0) - 12.0).abs() < 1e-5);
        assert!((map.cell_base_height_at(0.0, 0.0) - 2.0).abs() < 1e-5);

        map.cell_blend_width = 0.0;
        assert_eq!(map.cell_base_height_at(-0.5, 0.0), -8.0);
        assert_eq!(map.cell_base_height_at(0.5, 0.0), 12.0);
    }

    #[test]
    fn modifying_cell_base_height_updates_terrain_height() {
        let mut map = TerrainMap::default();
        let cell = map.cells()[0].clone();
        let before = map.height_at(cell.center.x, cell.center.y);
        let new_height = -18.5;

        assert!(map.set_cell_base_height(cell.id, new_height));
        let after = map.height_at(cell.center.x, cell.center.y);

        assert!((after - before - (new_height - cell.base_height)).abs() < 1e-4);
        assert!(!map.set_cell_base_height(usize::MAX, 0.0));
    }
}
