use bevy::prelude::Vec2;

use crate::terrain::macro_landform::WorldShape;
use crate::terrain::params::CellParameters;
use crate::terrain::region::Region;
use crate::terrain::seed::WorldSeed;
use crate::terrain::topology::WorldTopology;

#[derive(Clone, Debug)]
pub struct TerrainCell {
    pub id: usize,
    pub center: Vec2,
    pub base_height: f32,
    pub parameters: CellParameters,
    pub region: Region,
    pub macro_elevation: f32,
    pub sub_variant: usize,
}

impl TerrainCell {
    pub fn sub_variant_label(&self) -> Option<&'static str> {
        self.region.sub_variants().get(self.sub_variant).copied()
    }
}

#[derive(Clone, Debug)]
pub struct TerrainConfig {
    pub size: f32,
    pub subdivisions: usize,
    pub amplitude: f32,
    pub cell_grid_dim: usize,
    pub cell_base_height_range: f32,
    pub topology: WorldTopology,
    pub shape: WorldShape,
    pub seed: WorldSeed,
    pub relief: f32,
    pub crater_count: usize,
    pub crater_degradation: f32,
    pub erosion_strength: f32,
    pub droplet_count: usize,
    pub erosion_iterations: usize,
    pub repose_scale: f32,
    pub wind_erosion: f32,
    pub wind_iterations: usize,
    pub river_threshold: f32,
    pub blend_width: f32,
    pub flood_margin: f32,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            size: 400.0,
            subdivisions: 200,
            amplitude: 22.0,
            cell_grid_dim: 10,
            cell_base_height_range: 6.0,
            topology: WorldTopology::Finite,
            shape: WorldShape::Continent,
            seed: WorldSeed::default(),
            relief: 90.0,
            crater_count: 5,
            crater_degradation: 0.45,
            erosion_strength: 1.6,
            droplet_count: 8_000,
            erosion_iterations: 2,
            repose_scale: 1.0,
            wind_erosion: 0.12,
            wind_iterations: 6,
            river_threshold: 26.0,
            blend_width: 9.0,
            flood_margin: 1.5,
        }
    }
}

impl TerrainConfig {
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = WorldSeed::new(seed);
        self
    }

    pub fn with_topology(mut self, topology: WorldTopology) -> Self {
        self.topology = topology;
        self
    }

    pub fn with_shape(mut self, shape: WorldShape) -> Self {
        self.shape = shape;
        self
    }

    pub fn compact() -> Self {
        Self {
            size: 120.0,
            subdivisions: 32,
            cell_grid_dim: 4,
            crater_count: 1,
            amplitude: 14.0,
            river_threshold: 10.0,
            droplet_count: 2_000,
            erosion_iterations: 2,
            wind_iterations: 4,
            ..Default::default()
        }
    }

    pub fn sanitized(&self) -> TerrainConfig {
        let mut next = self.clone();
        let default = TerrainConfig::default();
        if !next.size.is_finite() || next.size <= 0.0 {
            next.size = default.size;
        }
        if next.subdivisions < 2 {
            next.subdivisions = default.subdivisions;
        }
        if next.cell_grid_dim < 1 {
            next.cell_grid_dim = default.cell_grid_dim;
        }
        let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };
        next.amplitude = finite(next.amplitude, default.amplitude);
        next.cell_base_height_range =
            finite(next.cell_base_height_range, default.cell_base_height_range);
        next.relief = finite(next.relief, default.relief);
        next.crater_degradation = finite(next.crater_degradation, default.crater_degradation);
        next.erosion_strength = finite(next.erosion_strength, default.erosion_strength);
        next.repose_scale = finite(next.repose_scale, default.repose_scale);
        next.wind_erosion = finite(next.wind_erosion, default.wind_erosion);
        next.river_threshold = finite(next.river_threshold, default.river_threshold);
        next.blend_width = finite(next.blend_width, default.blend_width);
        next.flood_margin = finite(next.flood_margin, default.flood_margin);
        next
    }
}
