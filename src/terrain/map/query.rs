use bevy::prelude::{IVec2, Vec2};

use crate::terrain::field::ScalarField;
use crate::terrain::hydrology::{self, Lake, Waterfall};
use crate::terrain::macro_landform::WorldShape;
use crate::terrain::params::{CellParameters, Salinity};
use crate::terrain::region::{self, Region};
use crate::terrain::seed::WorldSeed;
use crate::terrain::topology::WorldTopology;
use crate::terrain::voronoi::VoronoiLayout;
use crate::terrain::water::WaterBody;

use super::{TerrainCell, TerrainConfig, TerrainMap};

impl TerrainMap {
    pub fn config(&self) -> &TerrainConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut TerrainConfig {
        &mut self.config
    }

    pub fn size(&self) -> f32 {
        self.config.size
    }

    pub fn topology(&self) -> WorldTopology {
        self.config.topology
    }

    pub fn seed(&self) -> WorldSeed {
        self.config.seed
    }

    pub fn shape(&self) -> WorldShape {
        self.config.shape
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn cells(&self) -> &[TerrainCell] {
        &self.cells
    }

    pub fn cell(&self, id: usize) -> Option<&TerrainCell> {
        self.cells.get(id)
    }

    pub fn layout(&self) -> &VoronoiLayout {
        &self.layout
    }

    pub fn heights(&self) -> &ScalarField {
        &self.heights
    }

    pub fn base_heights(&self) -> &ScalarField {
        &self.base_heights
    }

    pub fn accumulation_field(&self) -> &ScalarField {
        &self.accumulation
    }

    pub fn lakes(&self) -> &[Lake] {
        &self.lakes
    }

    pub fn lake_planes(&self) -> &[WaterBody] {
        &self.lake_planes
    }

    pub fn lake_salinity(&self) -> &[Salinity] {
        &self.lake_salinity
    }

    pub fn ocean_mask(&self) -> &[bool] {
        &self.ocean_mask
    }

    pub fn water_frozen(&self) -> &[bool] {
        &self.water.frozen
    }
    pub fn waterfalls(&self) -> &[Waterfall] {
        &self.waterfalls
    }

    pub fn river_paths(&self) -> &[Vec<usize>] {
        &self.river_paths
    }

    pub fn cell_id_at(&self, x: f32, z: f32) -> Option<usize> {
        self.layout.cell_at(Vec2::new(x, z))
    }

    pub fn cell_at(&self, x: f32, z: f32) -> Option<&TerrainCell> {
        self.cell_id_at(x, z).and_then(|id| self.cells.get(id))
    }

    pub fn cell_weights_at(&self, x: f32, z: f32) -> Vec<(usize, f32)> {
        self.layout.cell_weights_at(Vec2::new(x, z))
    }

    pub fn region_at(&self, x: f32, z: f32) -> Option<Region> {
        self.cell_id_at(x, z).and_then(|id| self.regions.get(id).copied())
    }

    pub fn region_of_cell(&self, id: usize) -> Option<Region> {
        self.regions.get(id).copied()
    }

    pub fn parameters_at(&self, x: f32, z: f32) -> Option<&CellParameters> {
        self.cell_at(x, z).map(|cell| &cell.parameters)
    }

    pub fn cell_neighbours(&self, id: usize) -> &[usize] {
        self.cell_neighbours
            .get(id)
            .map(|n| n.as_slice())
            .unwrap_or(&[])
    }

    pub fn eligible_regions_at(&self, x: f32, z: f32) -> Vec<Region> {
        self.parameters_at(x, z)
            .map(region::eligible_regions)
            .unwrap_or_default()
    }

    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        self.heights.sample_world(Vec2::new(x, z))
    }

    pub fn base_height_at(&self, x: f32, z: f32) -> f32 {
        self.base_heights.sample_world(Vec2::new(x, z))
    }

    pub fn precipitation_at(&self, x: f32, z: f32) -> f32 {
        self.precipitation.sample_world(Vec2::new(x, z))
    }

    pub fn accumulation_at(&self, x: f32, z: f32) -> f32 {
        self.accumulation.sample_world(Vec2::new(x, z))
    }

    pub fn slope_at(&self, x: f32, z: f32) -> f32 {
        self.heights.gradient_world(Vec2::new(x, z)).length()
    }

    pub fn height_at_smooth(&self, x: f32, z: f32) -> f32 {
        self.heights.sample_catmull_rom(Vec2::new(x, z))
    }

    pub fn hardness_at(&self, x: f32, z: f32) -> f32 {
        self.hardness.sample_world(Vec2::new(x, z)).clamp(0.0, 1.0)
    }

    pub fn hardness_field(&self) -> &ScalarField {
        &self.hardness
    }

    pub fn flow_field(&self) -> &hydrology::FlowField {
        &self.flow
    }

    pub fn flow_direction_at(&self, x: f32, z: f32) -> Vec2 {
        self.flow.direction_at(Vec2::new(x, z))
    }

    pub fn flow_slope_at(&self, x: f32, z: f32) -> f32 {
        self.flow.slope_at(Vec2::new(x, z))
    }
    pub fn max_height_extent(&self) -> f32 {
        ((self.heights.max() - self.heights.min()) * 0.5).max(1.0)
    }
    pub fn chunk_count(&self, chunk_size: f32) -> IVec2 {
        let per_axis = (self.config.size / chunk_size.max(1.0)).ceil() as i32;
        IVec2::new(per_axis.max(1), per_axis.max(1))
    }

    pub fn chunk_keys(&self, chunk_size: f32) -> Vec<IVec2> {
        let count = self.chunk_count(chunk_size);
        (0..count.y)
            .flat_map(|y| (0..count.x).map(move |x| IVec2::new(x, y)))
            .collect()
    }

    pub fn chunk_bounds(&self, chunk: IVec2, chunk_size: f32) -> (Vec2, Vec2) {
        let low = Vec2::new(
            -self.config.size * 0.5 + chunk.x as f32 * chunk_size,
            -self.config.size * 0.5 + chunk.y as f32 * chunk_size,
        );
        (low, low + Vec2::splat(chunk_size))
    }

    pub fn height_in_chunk(&self, chunk: IVec2, chunk_size: f32, steps: usize) -> Vec<(Vec2, f32)> {
        let (low, high) = self.chunk_bounds(chunk, chunk_size);
        let steps = steps.max(1);
        let step = chunk_size / steps as f32;
        let mut samples = Vec::with_capacity((steps + 1) * (steps + 1));
        for iz in 0..=steps {
            for ix in 0..=steps {
                let point = Vec2::new(
                    (low.x + ix as f32 * step).min(high.x),
                    (low.y + iz as f32 * step).min(high.y),
                );
                samples.push((point, self.height_at(point.x, point.y)));
            }
        }
        samples
    }
}
