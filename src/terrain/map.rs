use bevy::prelude::{IVec2, Resource, Vec2};

use crate::terrain::field::ScalarField;
use crate::terrain::hydrology::{
    self, CraterConfig, ErosionConfig, Hydrology, Lake, PrecipitationInput, Waterfall, repose_tan,
};
use crate::terrain::macro_landform::{self, MacroConfig, MacroField, WorldShape, SEA_LEVEL};
use crate::terrain::noise::{
    NoiseAlgorithm, NoiseContext, NoiseEngine, NoiseShape, blend_shapes, select_algorithm,
    shape_for,
};
use crate::terrain::params::{
    BaseGradient, BaseHeightBand, CellParameters, Geothermal, MacroContext, Salinity, Substrate,
    Temperature, roll_cell_parameters, relax_parameters,
};
use crate::terrain::region::{self, Region, resolve_region, sub_variant_index};
use crate::terrain::season::{
    FloodZone, SeaLevelSeason, Season, SeasonClock, blended_delta, coastal_erosion,
};
use crate::terrain::seed::{WorldSeed, STAGE_BASE_GRADIENT, STAGE_NOISE};
use crate::terrain::topology::WorldTopology;
use crate::terrain::voronoi::{VoronoiConfig, VoronoiLayout};
use crate::terrain::water::{self, WaterBody, WaterField};

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
}

pub struct ParameterStage {
    pub cells: Vec<TerrainCell>,
    pub regions: Vec<Region>,
    pub neighbours: Vec<Vec<usize>>,
}

pub struct BaseFeatureStage {
    pub heights: ScalarField,
}

pub struct NoiseFeatureStage {
    pub heights: ScalarField,
}

pub struct EvolvingFeatureStage {
    pub heights: ScalarField,
    pub precipitation: ScalarField,
    pub accumulation: ScalarField,
    pub filled: ScalarField,
    pub lakes: Vec<Lake>,
    pub waterfalls: Vec<Waterfall>,
    pub river_paths: Vec<Vec<usize>>,
    pub cells: Vec<TerrainCell>,
    pub cell_of_vertex: Vec<usize>,
}

pub struct SeasonalFeatureStage {
    pub heights: ScalarField,
    pub flood_zone: FloodZone,
}

#[derive(Resource, Clone)]
pub struct TerrainMap {
    config: TerrainConfig,
    layout: VoronoiLayout,
    macro_field: MacroField,
    cells: Vec<TerrainCell>,
    regions: Vec<Region>,
    cell_neighbours: Vec<Vec<usize>>,
    base_heights: ScalarField,
    heights: ScalarField,
    precipitation: ScalarField,
    accumulation: ScalarField,
    filled: ScalarField,
    lakes: Vec<Lake>,
    waterfalls: Vec<Waterfall>,
    river_paths: Vec<Vec<usize>>,
    lake_planes: Vec<WaterBody>,
    lake_salinity: Vec<Salinity>,
    ocean_mask: Vec<bool>,
    cell_of_vertex: Vec<usize>,
    water: WaterField,
    flood_zone: FloodZone,
    sea_levels: SeaLevelSeason,
    sea_level_now: f32,
    base_cell_parameters: Vec<CellParameters>,
    base_cell_regions: Vec<Region>,
    season: Season,
    generation: u64,
}

impl Default for TerrainMap {
    fn default() -> Self {
        Self::new(TerrainConfig::default())
    }
}

impl TerrainMap {
    pub fn new(config: TerrainConfig) -> Self {
        let layout = VoronoiLayout::generate(
            config.seed,
            VoronoiConfig {
                size: config.size,
                grid_dim: config.cell_grid_dim as u32,
                topology: config.topology,
                default_blend_width: config.blend_width,
                ..Default::default()
            },
        );
        let macro_config = MacroConfig {
            size: config.size,
            topology: config.topology,
            sea_level: SEA_LEVEL,
            relief: config.relief,
        };
        let macro_field =
            macro_landform::generate_macro_field(config.seed, &layout, config.shape, &macro_config);

        let parameters = run_parameter_stage(&config, &layout, &macro_field);
        let base = run_base_stage(&config, &layout, &parameters);
        let noise = run_noise_stage(&config, &layout, &parameters, base);
        let evolving = run_evolving_stage(&config, &layout, &macro_field, &parameters, noise);
        let seasonal = run_seasonal_stage(&config, evolving.heights.clone());
        let base_cell_parameters = map_base_parameters(&evolving.cells);
        let base_cell_regions: Vec<Region> = evolving.cells.iter().map(|cell| cell.region).collect();

        let mut map = Self {
            cells: evolving.cells,
            regions: parameters.regions,
            cell_neighbours: parameters.neighbours,
            layout,
            macro_field,
            base_heights: evolving.heights,
            heights: seasonal.heights,
            precipitation: evolving.precipitation,
            accumulation: evolving.accumulation,
            filled: evolving.filled,
            lakes: evolving.lakes,
            waterfalls: evolving.waterfalls,
            river_paths: evolving.river_paths,
            lake_planes: Vec::new(),
            lake_salinity: Vec::new(),
            ocean_mask: Vec::new(),
            cell_of_vertex: evolving.cell_of_vertex,
            water: WaterField::empty(config.subdivisions, config.size, config.topology),
            flood_zone: seasonal.flood_zone,
            config,
            sea_levels: SeaLevelSeason::default(),
            sea_level_now: SeaLevelSeason::default().level_at(Season::Summer),
            base_cell_parameters,
            base_cell_regions,
            season: Season::Summer,
            generation: 1,
        };
        map.apply_seasonal_parameters(&SeasonClock {
            season: Season::Summer,
            ..Default::default()
        });
        map.refresh_water();
        map
    }

    pub fn compact() -> Self {
        Self::new(TerrainConfig::compact())
    }

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

    pub fn water_surface_at(&self, x: f32, z: f32) -> Option<water::WaterSample> {
        self.water.sample_at(&self.heights, Vec2::new(x, z))
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
        let (gx, gz) = self.heights.grid_of(Vec2::new(x, z));
        self.heights.gradient(gx, gz).length()
    }

    pub fn sea_level(&self) -> f32 {
        self.sea_level_now
    }

    pub fn prevailing_wind(&self) -> Vec2 {
        self.macro_field.prevailing_wind
    }

    pub fn sea_levels(&self) -> &SeaLevelSeason {
        &self.sea_levels
    }

    pub fn flood_zone(&self) -> &FloodZone {
        &self.flood_zone
    }

    pub fn is_underwater(&self, x: f32, z: f32) -> bool {
        self.height_at(x, z) < self.sea_level()
    }

    pub fn depth_below_sea(&self, x: f32, z: f32) -> f32 {
        (self.sea_level() - self.height_at(x, z)).max(0.0)
    }

    pub fn max_height_extent(&self) -> f32 {
        ((self.heights.max() - self.heights.min()) * 0.5).max(1.0)
    }

    pub fn footprint_bytes(&self) -> usize {
        let field_bytes = |field: &ScalarField| field.values.len() * size_of::<f32>();
        let mut bytes = field_bytes(&self.heights)
            + field_bytes(&self.base_heights)
            + field_bytes(&self.precipitation)
            + field_bytes(&self.accumulation)
            + field_bytes(&self.filled)
            + field_bytes(&self.water.surface);
        bytes += self.ocean_mask.len() + self.water.frozen.len();
        bytes += self.cell_of_vertex.len() * size_of::<usize>();
        bytes += self.cells.len() * size_of::<TerrainCell>();
        bytes += self.regions.len() * size_of::<Region>();
        bytes += self.base_cell_parameters.len() * size_of::<CellParameters>();
        bytes += self
            .cell_neighbours
            .iter()
            .map(|list| list.len() * size_of::<usize>())
            .sum::<usize>();
        bytes += self
            .river_paths
            .iter()
            .map(|path| path.len() * size_of::<usize>())
            .sum::<usize>();
        bytes += self.lakes.len() * size_of::<Lake>();
        bytes += self.waterfalls.len() * size_of::<Waterfall>();
        bytes += self.lake_planes.len() * size_of::<WaterBody>();
        bytes += self.lake_salinity.len() * size_of::<Salinity>();
        bytes += self.water.kind.len() * size_of::<water::WaterKind>();
        bytes += self.water.flow.len() * size_of::<Vec2>();
        bytes += self.water.salinity.len() * size_of::<Salinity>();
        bytes += size_of_val(self.layout.cells());
        bytes
    }

    #[cfg(test)]
    pub fn checksum(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for value in self.heights.values.iter() {
            fnv_mix(&mut hash, *value);
        }
        for value in self.base_heights.values.iter() {
            fnv_mix(&mut hash, *value);
        }
        for value in self.precipitation.values.iter() {
            fnv_mix(&mut hash, *value);
        }
        for value in self.accumulation.values.iter() {
            fnv_mix(&mut hash, *value);
        }
        for region in self.regions.iter() {
            fnv_mix(&mut hash, region.index() as f32);
        }
        for path in self.river_paths.iter() {
            fnv_mix(&mut hash, path.len() as f32);
        }
        fnv_mix(&mut hash, self.lakes.len() as f32);
        fnv_mix(&mut hash, self.waterfalls.len() as f32);
        hash
    }

    pub fn set_cell_base_height(&mut self, cell_id: usize, base_height: f32) -> bool {
        let Some(cell) = self.cells.iter_mut().find(|c| c.id == cell_id) else {
            return false;
        };
        cell.base_height = base_height;
        self.rebuild_surface();
        true
    }

    pub fn set_season(&mut self, season: Season) {
        if season == self.season {
            return;
        }
        self.season = season;
        self.apply_seasonal_parameters(&SeasonClock {
            season,
            ..Default::default()
        });
        let seasonal = run_seasonal_stage(&self.config, self.base_heights.clone());
        self.heights = seasonal.heights;
        self.flood_zone = seasonal.flood_zone;
        self.sea_level_now = self.sea_levels.level_at(season);
        self.refresh_water();
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn sync_season_state(&mut self, clock: &SeasonClock) -> bool {
        let changed = clock.season != self.season;
        if changed {
            self.set_season(clock.season);
        }
        self.apply_seasonal_parameters(clock);
        self.sea_level_now = self.sea_levels.continuous_level(clock.cycle_progress());
        changed
    }

    pub fn apply_seasonal_parameters(&mut self, clock: &SeasonClock) {
        for index in 0..self.cells.len() {
            let (Some(base), Some(tier)) = (
                self.base_cell_parameters.get(index).copied(),
                self.base_cell_regions.get(index).copied(),
            ) else {
                continue;
            };
            let parameters = blended_delta(clock, tier.definition().tier).apply(&base);
            let region = resolve_region(&parameters);
            if let Some(cell) = self.cells.get_mut(index) {
                cell.sub_variant =
                    sub_variant_index(region, &parameters, self.config.seed, cell.id as u64);
                cell.parameters = parameters;
                cell.region = region;
            }
        }
    }

    pub fn regenerate(&mut self) {
        let config = self.config.clone();
        *self = Self::new(config);
    }

    pub fn reseed(&mut self, seed: u64) {
        self.config.seed = WorldSeed::new(seed);
        self.regenerate();
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

    fn rebuild_surface(&mut self) {
        let parameters = ParameterStage {
            cells: self.cells.clone(),
            regions: self.regions.clone(),
            neighbours: self.cell_neighbours.clone(),
        };
        let base = run_base_stage(&self.config, &self.layout, &parameters);
        let noise = run_noise_stage(&self.config, &self.layout, &parameters, base);
        let evolving =
            run_evolving_stage(&self.config, &self.layout, &self.macro_field, &parameters, noise);
        let seasonal = run_seasonal_stage(&self.config, evolving.heights.clone());

        self.cells = evolving.cells;
        self.base_cell_parameters = map_base_parameters(&self.cells);
        self.base_cell_regions = self.cells.iter().map(|cell| cell.region).collect();
        self.apply_seasonal_parameters(&SeasonClock {
            season: self.season,
            ..Default::default()
        });
        self.base_heights = evolving.heights;
        self.heights = seasonal.heights;
        self.precipitation = evolving.precipitation;
        self.accumulation = evolving.accumulation;
        self.filled = evolving.filled;
        self.lakes = evolving.lakes;
        self.waterfalls = evolving.waterfalls;
        self.river_paths = evolving.river_paths;
        self.cell_of_vertex = evolving.cell_of_vertex;
        self.flood_zone = seasonal.flood_zone;
        self.refresh_water();
        self.generation = self.generation.wrapping_add(1);
    }

    fn refresh_water(&mut self) {
        let sea_top = self.sea_levels.level_at(Season::Summer);
        self.ocean_mask = compute_ocean_mask(
            &self.heights,
            self.config.topology,
            &self.macro_field.bands,
            &self.cell_of_vertex,
            sea_top,
        );
        self.lake_salinity = self.compute_lake_salinity();
        self.lake_planes = water::lake_planes(&self.lakes, &self.heights, 6, &self.lake_salinity);
        let temperatures: Vec<Temperature> =
            self.cells.iter().map(|cell| cell.parameters.temperature).collect();
        let geothermal: Vec<Geothermal> =
            self.cells.iter().map(|cell| cell.parameters.geothermal).collect();
        let ctx = water::WaterCellContext {
            cell_of_vertex: &self.cell_of_vertex,
            temperature: &temperatures,
            geothermal: &geothermal,
        };
        let sea_level = self.sea_level();
        let season = self.season;
        self.water = water::WaterField::build(
            &self.heights,
            sea_level,
            &self.ocean_mask,
            &self.lakes,
            &self.lake_salinity,
            &self.river_paths,
            &self.waterfalls,
            &self.accumulation,
            season,
            &ctx,
        );
    }

    fn compute_lake_salinity(&self) -> Vec<Salinity> {
        let sea_level = self.sea_level();
        self.lakes
            .iter()
            .map(|lake| {
                if lake.drains_to_sea {
                    return Salinity::Freshwater;
                }
                if lake.surface <= sea_level + 1.0 {
                    return Salinity::Brackish;
                }
                let mut arid = 0usize;
                let mut sampled = 0usize;
                for &vertex in lake.cells.iter().step_by(4) {
                    let cell = self.cell_of_vertex.get(vertex).copied().unwrap_or(0);
                    let Some(source) = self.cells.get(cell) else {
                        continue;
                    };
                    sampled += 1;
                    if source.parameters.is_arid() {
                        arid += 1;
                    }
                }
                if sampled > 0 && arid * 2 > sampled {
                    Salinity::Hypersaline
                } else {
                    Salinity::Freshwater
                }
            })
            .collect()
    }
}

fn map_base_parameters(cells: &[TerrainCell]) -> Vec<CellParameters> {
    cells.iter().map(|cell| cell.parameters).collect()
}

fn compute_ocean_mask(
    heights: &ScalarField,
    topology: WorldTopology,
    bands: &[BaseHeightBand],
    cell_of_vertex: &[usize],
    sea_top: f32,
) -> Vec<bool> {
    let mut seeds: Vec<usize> = Vec::new();
    if topology.wraps() {
        for index in 0..heights.len() {
            let cell = cell_of_vertex.get(index).copied().unwrap_or(0);
            if matches!(
                bands.get(cell).copied(),
                Some(BaseHeightBand::Abyssal) | Some(BaseHeightBand::DeepOcean)
            ) {
                seeds.push(index);
            }
        }
    } else {
        for z in 0..heights.dim {
            for x in 0..heights.dim {
                if x == 0 || z == 0 || x == heights.dim - 1 || z == heights.dim - 1 {
                    let index = heights.index(x, z);
                    if heights.get_at(index) <= sea_top {
                        seeds.push(index);
                    }
                }
            }
        }
    }
    if seeds.is_empty() {
        let mut lowest = 0usize;
        for index in 1..heights.len() {
            if heights.get_at(index) < heights.get_at(lowest) {
                lowest = index;
            }
        }
        seeds.push(lowest);
    }
    water::ocean_connectivity(heights, sea_top, &seeds)
}

pub fn run_parameter_stage(
    config: &TerrainConfig,
    layout: &VoronoiLayout,
    macro_field: &MacroField,
) -> ParameterStage {
    let neighbours: Vec<Vec<usize>> = layout
        .cells()
        .iter()
        .map(|cell| cell.neighbours.clone())
        .collect();

    let mut rolled: Vec<CellParameters> = layout
        .cells()
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            roll_cell_parameters(config.seed, cell.id as u64, &macro_context(macro_field, index))
        })
        .collect();

    relax_parameters(config.seed, &mut rolled, &neighbours, 3);
    let regions: Vec<Region> = rolled.iter().map(resolve_region).collect();

    let cells = layout
        .cells()
        .iter()
        .enumerate()
        .map(|(index, cell)| TerrainCell {
            id: cell.id,
            center: cell.center,
            base_height: config.seed.range(
                STAGE_BASE_GRADIENT,
                cell.id as u64,
                3,
                -config.cell_base_height_range,
                config.cell_base_height_range,
            ),
            parameters: rolled[index],
            region: regions[index],
            macro_elevation: macro_field
                .elevation
                .get(index)
                .copied()
                .unwrap_or(SEA_LEVEL),
            sub_variant: sub_variant_index(
                regions[index],
                &rolled[index],
                config.seed,
                cell.id as u64,
            ),
        })
        .collect();

    ParameterStage {
        cells,
        regions,
        neighbours,
    }
}

fn macro_context(macro_field: &MacroField, index: usize) -> MacroContext {
    MacroContext {
        band: macro_field
            .bands
            .get(index)
            .copied()
            .unwrap_or(crate::terrain::params::BaseHeightBand::SeaLevel),
        latitude: macro_field.latitude.get(index).copied().unwrap_or(0.5),
        prevailing_wind: macro_field.prevailing_wind,
        rain_shadow: macro_field.rain_shadow.get(index).copied().unwrap_or(0.0),
        geothermal_intensity: macro_field
            .geothermal_intensity
            .get(index)
            .copied()
            .unwrap_or(0.0),
        sea_proximity: macro_field.sea_proximity.get(index).copied().unwrap_or(0.5),
        downslope: macro_field
            .downslope
            .get(index)
            .copied()
            .unwrap_or(Vec2::X),
        slope_magnitude: macro_field
            .slope_magnitude
            .get(index)
            .copied()
            .unwrap_or(0.3),
    }
}

pub fn run_base_stage(
    config: &TerrainConfig,
    layout: &VoronoiLayout,
    parameters: &ParameterStage,
) -> BaseFeatureStage {
    let mut field = ScalarField::new(config.subdivisions, config.size, config.topology);
    let cache = layout.vertex_cache(field.dim, config.size, config.topology);
    for z in 0..field.dim {
        for x in 0..field.dim {
            let index = field.index(x, z);
            let point = field.world_of(x, z);
            let mut sum = 0.0f32;
            let mut total = 0.0f32;
            for &(id, weight) in cache.weights_at(index) {
                let Some(cell) = parameters.cells.get(id) else {
                    continue;
                };
                let tilt = layout
                    .delta(cell.center, point)
                    .dot(cell.parameters.base_gradient.aspect)
                    * BaseGradient::slope_tan(cell.parameters.base_gradient.slope);
                sum += (cell.macro_elevation + cell.base_height + tilt) * weight;
                total += weight;
            }
            field.set(x, z, if total > 0.0 { sum / total } else { SEA_LEVEL });
        }
    }
    BaseFeatureStage { heights: field }
}

pub fn run_noise_stage(
    config: &TerrainConfig,
    layout: &VoronoiLayout,
    parameters: &ParameterStage,
    base: BaseFeatureStage,
) -> NoiseFeatureStage {
    let engine = NoiseEngine::new(config.seed.stage(STAGE_NOISE) as u32);
    let ctx = NoiseContext {
        topology: config.topology,
        size: config.size,
        wind: macro_landform::prevailing_wind_for(config.seed),
    };

    let algorithms: Vec<NoiseAlgorithm> = parameters
        .cells
        .iter()
        .map(|cell| select_algorithm(&cell.parameters, cell.sub_variant_label().unwrap_or("")))
        .collect();
    let shapes: Vec<NoiseShape> = parameters
        .cells
        .iter()
        .map(|cell| shape_for(&cell.parameters, cell.region, cell.sub_variant_label().unwrap_or("")))
        .collect();
    let global_scale = config.amplitude / TerrainConfig::default().amplitude;

    let mut field = base.heights;
    let cache = layout.vertex_cache(field.dim, config.size, config.topology);
    for z in 0..field.dim {
        for x in 0..field.dim {
            let point = field.world_of(x, z);
            let weights = cache.weights_at(field.index(x, z));
            let mut contributions: Vec<(NoiseAlgorithm, f32)> = Vec::with_capacity(weights.len());
            let mut shape_items: Vec<(NoiseShape, f32)> = Vec::with_capacity(weights.len());
            for &(id, weight) in weights {
                let (Some(algorithm), Some(shape)) = (algorithms.get(id), shapes.get(id)) else {
                    continue;
                };
                contributions.push((*algorithm, weight));
                shape_items.push((*shape, weight));
            }
            let shape = blend_shapes(&shape_items);
            let noise = engine.sample_blend_with(&contributions, point.x, point.y, &ctx, &shape);
            field.add(x, z, noise * shape.amplitude * global_scale);
        }
    }

    NoiseFeatureStage { heights: field }
}

pub fn run_evolving_stage(
    config: &TerrainConfig,
    layout: &VoronoiLayout,
    macro_field: &MacroField,
    parameters: &ParameterStage,
    noise: NoiseFeatureStage,
) -> EvolvingFeatureStage {
    let mut heights = noise.heights;

    let cache = layout.vertex_cache(heights.dim, config.size, config.topology);
    let cell_of_vertex: Vec<usize> = (0..heights.len())
        .map(|index| cache.cell_at(index).unwrap_or(0))
        .collect();

    let cell_precipitation: Vec<_> = parameters
        .cells
        .iter()
        .map(|cell| cell.parameters.precipitation)
        .collect();

    let precipitation = hydrology::generate_precipitation_blended(
        &heights,
        &PrecipitationInput {
            cell_of_vertex: &cell_of_vertex,
            cell_precipitation: &cell_precipitation,
            cell_rain_shadow: &macro_field.rain_shadow,
            wind: macro_field.prevailing_wind,
            relief: config.relief,
        },
        Some(&cache),
    );

    let craters = hydrology::place_craters(
        config.seed,
        &CraterConfig {
            count: config.crater_count,
            degradation: config.crater_degradation,
            ..Default::default()
        },
        config.size,
    );
    let mut fresh_craters = Vec::new();
    let mut eroded_craters = Vec::new();
    for mut crater in craters {
        if crater_allowed(layout, &parameters.cells, crater.center) {
            crater.degradation = 0.0;
            fresh_craters.push(crater);
        } else {
            eroded_craters.push(crater);
        }
    }
    hydrology::apply_craters(&mut heights, &fresh_craters, &|_| true);

    let mut carving = hydrology::priority_flood(&heights, SEA_LEVEL);
    hydrology::flow_accumulation(&mut carving, &precipitation);
    hydrology::carve_rivers(&mut heights, &carving, config.river_threshold, config.erosion_strength);
    hydrology::droplet_erosion(
        &mut heights,
        &precipitation,
        config.seed,
        &ErosionConfig {
            droplet_count: config.droplet_count,
            iterations: config.erosion_iterations,
            ..Default::default()
        },
    );

    let cell_substrate: Vec<Substrate> = parameters
        .cells
        .iter()
        .map(|cell| cell.parameters.substrate)
        .collect();
    let mut repose = ScalarField::new(heights.dim, config.size, config.topology);
    for index in 0..heights.len() {
        let substrate = cell_substrate
            .get(cell_of_vertex[index])
            .copied()
            .unwrap_or(Substrate::SiltClay);
        repose.set_at(index, repose_tan(substrate) * config.repose_scale);
    }
    hydrology::thermal_relax_repose(&mut heights, &repose, 6);
    hydrology::apply_wind_erosion(
        &mut heights,
        macro_field.prevailing_wind,
        config.wind_erosion,
        config.wind_iterations,
    );

    hydrology::apply_craters(&mut heights, &eroded_craters, &|_| true);

    let mut settled = hydrology::priority_flood(&heights, SEA_LEVEL);
    hydrology::flow_accumulation(&mut settled, &precipitation);
    let sea_top = SeaLevelSeason::default().level_at(Season::Summer);
    let ocean = compute_ocean_mask(
        &heights,
        config.topology,
        &macro_field.bands,
        &cell_of_vertex,
        sea_top,
    );
    let lakes = hydrology::find_lakes(&heights, &settled, SEA_LEVEL, &ocean);
    let mut waterfalls =
        hydrology::find_waterfalls(&heights, &settled, config.river_threshold, 4.0, &ocean);
    hydrology::carve_plunge_pools(&mut heights, &settled, &mut waterfalls);
    let river_paths = hydrology::river_paths(&settled, config.river_threshold, &ocean);

    let cells = mark_hydrology(
        &parameters.cells, layout, &settled, &lakes, &heights, config.seed,
    );

    EvolvingFeatureStage {
        heights,
        precipitation,
        accumulation: settled.accumulation.clone(),
        filled: settled.filled,
        lakes,
        waterfalls,
        river_paths,
        cells,
        cell_of_vertex,
    }
}

fn crater_allowed(layout: &VoronoiLayout, cells: &[TerrainCell], point: Vec2) -> bool {
    let Some(id) = layout.cell_at(point) else {
        return false;
    };
    matches!(
        cells.get(id).map(|cell| cell.region),
        Some(
            Region::VolcanicCaldera
                | Region::HydrothermalVentField
                | Region::ScreeTalus
                | Region::DesertBarren
                | Region::HighlandSteppeDesert
        )
    )
}

fn mark_hydrology(
    cells: &[TerrainCell],
    layout: &VoronoiLayout,
    settled: &Hydrology,
    lakes: &[Lake],
    heights: &ScalarField,
    seed: WorldSeed,
) -> Vec<TerrainCell> {
    let mut updated = cells.to_vec();
    let mut lake_cells = std::collections::HashSet::new();
    for lake in lakes {
        for &vertex in &lake.cells {
            let (gx, gz) = heights.coords(vertex);
            if let Some(id) = layout.cell_at(heights.world_of(gx, gz)) {
                lake_cells.insert(id);
            }
        }
    }

    for (index, cell) in updated.iter_mut().enumerate() {
        let centre = cell.center;
        let (gx, gz) = heights.grid_of(centre);
        let accumulation = settled.accumulation.get(gx, gz);
        let mut parameters = cell.parameters;
        if lake_cells.contains(&index) {
            parameters.hydrology = crate::terrain::params::Hydrology::FloodedBasin;
        } else if accumulation > 0.0 {
            let threshold = settled.accumulation.max() * 0.25;
            if accumulation >= threshold.max(1.0) {
                parameters.hydrology = crate::terrain::params::Hydrology::FluvialChannel;
            }
        }
        cell.region = resolve_region(&parameters);
        cell.sub_variant = sub_variant_index(cell.region, &parameters, seed, cell.id as u64);
        cell.parameters = parameters;
    }
    updated
}

pub fn run_seasonal_stage(config: &TerrainConfig, mut heights: ScalarField) -> SeasonalFeatureStage {
    let levels = SeaLevelSeason::default();
    let all: Vec<f32> = Season::ALL.iter().map(|s| levels.level_at(*s)).collect();
    let flood_zone = FloodZone::around_sea_levels(&all, config.flood_margin);
    coastal_erosion(&mut heights, &flood_zone, 0.55);
    SeasonalFeatureStage {
        heights,
        flood_zone,
    }
}

#[cfg(test)]
pub fn height_span(field: &ScalarField) -> f32 {
    field.max() - field.min()
}

#[cfg(test)]
fn fnv_mix(hash: &mut u64, value: f32) {
    let bits = (value * 1000.0).round() as i64 as u64;
    for byte in bits.to_le_bytes() {
        *hash ^= byte as u64;
        *hash = hash.wrapping_mul(0x1000_0000_0000_01b3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN_SEED: u64 = 20_261_120;
    const GOLDEN_FINITE: u64 = 0x2A55_4426_7310_3317;
    const GOLDEN_TORUS: u64 = 0x3690_4CC8_5156_300D;

    struct Stages {
        parameters: ParameterStage,
        base: ScalarField,
        noise: ScalarField,
        evolving_heights: ScalarField,
        seasonal_heights: ScalarField,
        flood_zone: FloodZone,
    }

    fn run_all_stages(config: &TerrainConfig) -> Stages {
        let layout = VoronoiLayout::generate(
            config.seed,
            VoronoiConfig {
                size: config.size,
                grid_dim: config.cell_grid_dim as u32,
                topology: config.topology,
                default_blend_width: config.blend_width,
                ..Default::default()
            },
        );
        let macro_field = macro_landform::generate_macro_field(
            config.seed,
            &layout,
            config.shape,
            &MacroConfig {
                size: config.size,
                topology: config.topology,
                sea_level: SEA_LEVEL,
                relief: config.relief,
            },
        );
        let parameters = run_parameter_stage(config, &layout, &macro_field);
        let base = run_base_stage(config, &layout, &parameters);
        let base_field = base.heights.clone();
        let noise = run_noise_stage(config, &layout, &parameters, base);
        let noise_field = noise.heights.clone();
        let evolving = run_evolving_stage(config, &layout, &macro_field, &parameters, noise);
        let evolving_heights = evolving.heights.clone();
        let seasonal = run_seasonal_stage(config, evolving.heights);
        Stages {
            parameters,
            base: base_field,
            noise: noise_field,
            evolving_heights,
            seasonal_heights: seasonal.heights,
            flood_zone: seasonal.flood_zone,
        }
    }

    fn golden_config(topology: WorldTopology) -> TerrainConfig {
        TerrainConfig::compact()
            .with_seed(GOLDEN_SEED)
            .with_topology(topology)
    }

    #[test]
    fn a_seed_reproduces_the_same_world_for_every_topology() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let config = golden_config(topology);
            let first = TerrainMap::new(config.clone());
            let second = TerrainMap::new(config);
            assert_eq!(
                first.heights().values,
                second.heights().values,
                "{topology:?} heights drifted between runs"
            );
            assert_eq!(first.checksum(), second.checksum());
            assert_eq!(first.cells().len(), second.cells().len());
            assert_eq!(first.lakes().len(), second.lakes().len());
            assert_eq!(first.waterfalls().len(), second.waterfalls().len());
            assert_eq!(
                first.river_paths().iter().map(|p| p.len()).sum::<usize>(),
                second.river_paths().iter().map(|p| p.len()).sum::<usize>()
            );
        }
    }

    #[test]
    fn every_pipeline_stage_is_deterministic_on_its_own() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let config = golden_config(topology);
            let first = run_all_stages(&config);
            let second = run_all_stages(&config);
            assert_eq!(first.base.values, second.base.values, "base stage");
            assert_eq!(first.noise.values, second.noise.values, "noise stage");
            assert_eq!(
                first.evolving_heights.values, second.evolving_heights.values,
                "evolving stage"
            );
            assert_eq!(
                first.seasonal_heights.values, second.seasonal_heights.values,
                "seasonal stage"
            );
            assert_eq!(first.flood_zone, second.flood_zone);
            let regions: Vec<usize> = first.parameters.regions.iter().map(|r| r.index()).collect();
            let repeated: Vec<usize> = second.parameters.regions.iter().map(|r| r.index()).collect();
            assert_eq!(regions, repeated, "parameter stage");
        }
    }

    #[test]
    fn stages_actually_change_the_heightfield() {
        let config = golden_config(WorldTopology::Finite);
        let stages = run_all_stages(&config);
        let mean_delta = stages
            .noise
            .values
            .iter()
            .zip(stages.base.values.iter())
            .map(|(n, b)| (n - b).abs())
            .sum::<f32>()
            / stages.base.values.len() as f32;
        assert!(
            mean_delta > 0.5,
            "the noise stage must add relief on top of the base features, mean |delta| {mean_delta}"
        );
        assert_ne!(
            stages.evolving_heights.values, stages.noise.values,
            "the evolving stage must carve the heightfield"
        );
    }

    #[test]
    fn continent_and_archipelago_hold_target_land_fraction_on_full_worlds() {
        for (shape, min, max) in [
            (WorldShape::Continent, 0.30f32, 0.60),
            (WorldShape::Archipelago, 0.30, 0.60),
        ] {
            let mut fractions: Vec<f32> = Vec::new();
            for seed_index in 0..3u64 {
                let map = TerrainMap::new(
                    TerrainConfig::default()
                        .with_seed(GOLDEN_SEED + seed_index * 101)
                        .with_shape(shape),
                );
                let above = map
                    .heights()
                    .values
                    .iter()
                    .filter(|&&h| h > map.sea_level())
                    .count();
                fractions.push(above as f32 / map.heights().values.len() as f32);
            }
            let mean = fractions.iter().sum::<f32>() / fractions.len() as f32;
            eprintln!("{shape:?} full-world land fraction mean {mean:.3}, per-seed {fractions:?}");
            assert!(
                (min..=max).contains(&mean),
                "{shape:?} mean land fraction {mean:.3} outside {min}..{max}: {fractions:?}"
            );
        }
    }

    #[test]
    fn a_different_seed_produces_a_different_world() {
        let base = TerrainMap::new(golden_config(WorldTopology::Finite));
        let other = TerrainMap::new(
            TerrainConfig::compact()
                .with_seed(GOLDEN_SEED + 1)
                .with_topology(WorldTopology::Finite),
        );
        assert_ne!(base.checksum(), other.checksum());
        assert_ne!(base.heights().values, other.heights().values);
    }

    #[test]
    fn every_world_shape_carves_its_own_land() {
        let worlds: Vec<(WorldShape, u64)> = WorldShape::all()
            .iter()
            .map(|shape| {
                let config = TerrainConfig::compact()
                    .with_seed(GOLDEN_SEED)
                    .with_shape(*shape);
                (*shape, TerrainMap::new(config).checksum())
            })
            .collect();
        for (index, (shape, checksum)) in worlds.iter().enumerate() {
            for (other_shape, other) in worlds[index + 1..].iter() {
                assert_ne!(
                    checksum, other,
                    "{shape:?} and {other_shape:?} generated the same world"
                );
            }
        }
    }

    #[test]
    fn topology_changes_the_world() {
        let finite = TerrainMap::new(golden_config(WorldTopology::Finite));
        let torus = TerrainMap::new(golden_config(WorldTopology::FiniteTorus));
        assert_ne!(finite.checksum(), torus.checksum());
    }

    #[test]
    fn noise_relief_follows_the_per_cell_shape() {
        let config = golden_config(WorldTopology::Finite);
        let layout = VoronoiLayout::generate(
            config.seed,
            VoronoiConfig {
                size: config.size,
                grid_dim: config.cell_grid_dim as u32,
                topology: config.topology,
                default_blend_width: config.blend_width,
                ..Default::default()
            },
        );
        let macro_field = macro_landform::generate_macro_field(
            config.seed,
            &layout,
            config.shape,
            &MacroConfig {
                size: config.size,
                topology: config.topology,
                sea_level: SEA_LEVEL,
                relief: config.relief,
            },
        );
        let parameters = run_parameter_stage(&config, &layout, &macro_field);
        let base = run_base_stage(&config, &layout, &parameters);
        let base_field = base.heights.clone();
        let noise = run_noise_stage(&config, &layout, &parameters, base);
        let shapes: Vec<NoiseShape> = parameters
            .cells
            .iter()
            .map(|cell| {
                shape_for(
                    &cell.parameters,
                    cell.region,
                    cell.sub_variant_label().unwrap_or(""),
                )
            })
            .collect();

        let mut sums = vec![0.0f32; parameters.cells.len()];
        let mut counts = vec![0usize; parameters.cells.len()];
        for z in 0..base_field.dim {
            for x in 0..base_field.dim {
                let point = base_field.world_of(x, z);
                let Some(id) = layout.cell_at(point) else {
                    continue;
                };
                sums[id] += (noise.heights.get(x, z) - base_field.get(x, z)).abs();
                counts[id] += 1;
            }
        }

        let means: Vec<(f32, f32)> = shapes
            .iter()
            .zip(sums.iter())
            .zip(counts.iter())
            .filter(|((_, _), count)| **count > 0)
            .map(|((shape, sum), count)| (shape.amplitude, sum / *count as f32))
            .collect();
        let amplitudes: Vec<f32> = means.iter().map(|(amplitude, _)| *amplitude).collect();
        let max_amplitude = amplitudes.iter().cloned().fold(f32::MIN, f32::max);
        let min_amplitude = amplitudes.iter().cloned().fold(f32::MAX, f32::min);
        assert!(
            max_amplitude > min_amplitude * 2.0,
            "cells must not all share one amplitude: {amplitudes:?}"
        );
        let richest = means
            .iter()
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap();
        let poorest = means
            .iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap();
        assert!(
            richest.1 > poorest.1 * 1.5,
            "the loudest cell must carve more relief than the quietest: {:?} vs {:?}",
            richest,
            poorest
        );
    }

    #[test]
    fn golden_seed_checksums_match_the_recorded_worlds() {
        let finite = TerrainMap::new(golden_config(WorldTopology::Finite));
        let torus = TerrainMap::new(golden_config(WorldTopology::FiniteTorus));
        let worlds = [
            ("finite", finite.checksum(), GOLDEN_FINITE),
            ("torus", torus.checksum(), GOLDEN_TORUS),
        ];
        let drifted: Vec<String> = worlds
            .iter()
            .filter(|(_, computed, recorded)| computed != recorded)
            .map(|(name, computed, recorded)| {
                format!("{name}: computed {computed:#018x}, recorded {recorded:#018x}")
            })
            .collect();
        assert!(drifted.is_empty(), "golden checksums drifted: {drifted:?}");
    }

    #[test]
    fn live_cell_parameters_track_the_season_clock() {
        use crate::terrain::params::ParamLevel;
        let mut map = TerrainMap::new(golden_config(WorldTopology::Finite));
        let centre = map.cells()[0].center;
        let summer_clock = SeasonClock {
            season: Season::Summer,
            progress: 0.0,
            running: false,
            ..Default::default()
        };
        let winter_clock = SeasonClock {
            season: Season::Winter,
            progress: 0.0,
            running: false,
            ..Default::default()
        };
        map.sync_season_state(&summer_clock);
        let summer = map
            .parameters_at(centre.x, centre.y)
            .copied()
            .expect("the cell centre must resolve to its own cell");
        assert!(map.sync_season_state(&winter_clock), "the season must advance");
        let winter = map
            .parameters_at(centre.x, centre.y)
            .copied()
            .expect("the cell centre must resolve to its own cell");
        assert_ne!(
            summer.temperature, winter.temperature,
            "the live cell parameters must move with the clock"
        );
        assert!(
            winter.temperature.index() > summer.temperature.index(),
            "winter must shift the live cell toward freezing through the real path"
        );
        map.sync_season_state(&summer_clock);
        let restored = map.parameters_at(centre.x, centre.y).copied().unwrap();
        assert_eq!(
            restored.temperature, summer.temperature,
            "returning to summer must restore the same live parameters"
        );
    }

    #[test]
    fn sea_level_is_continuous_across_a_season_boundary() {
        let mut map = TerrainMap::new(golden_config(WorldTopology::Finite));
        let mut clock = SeasonClock {
            cycle_seconds: 400.0,
            running: true,
            ..Default::default()
        };
        clock.set_season(Season::Winter);
        map.sync_season_state(&clock);
        let start = map.sea_level();
        let mut previous = start;
        let mut worst_step = 0.0f32;
        for _ in 0..800 {
            clock.tick(0.5);
            map.sync_season_state(&clock);
            let level = map.sea_level();
            worst_step = worst_step.max((level - previous).abs());
            previous = level;
        }
        assert!(
            worst_step < 0.06,
            "sea level stepped by {worst_step} instead of flowing"
        );
        assert!(
            (previous - start).abs() < 1e-3,
            "a full cycle must return the tide to its start"
        );
        assert!(
            map.sea_levels().max() - map.sea_levels().min() > 1.0,
            "the band must actually move"
        );
    }

    #[test]
    fn regenerating_from_a_config_reproduces_the_original_map() {
        let mut map = TerrainMap::new(golden_config(WorldTopology::FiniteTorus));
        let before = map.checksum();
        map.reseed(GOLDEN_SEED + 9);
        assert_ne!(map.checksum(), before, "reseeding must change the world");
        map.reseed(GOLDEN_SEED);
        assert_eq!(map.checksum(), before, "reseeding back must restore it");
    }

    fn full_world(topology: WorldTopology) -> TerrainMap {
        TerrainMap::new(
            TerrainConfig::default()
                .with_seed(GOLDEN_SEED)
                .with_topology(topology),
        )
    }

    #[test]
    fn water_surface_at_covers_sea_lakes_and_rivers() {
        use crate::terrain::water::WaterKind;
        let map = full_world(WorldTopology::Finite);

        let sea_vertex = (0..map.heights().len())
            .find(|&i| map.ocean_mask()[i] && map.heights().get_at(i) < map.sea_level())
            .expect("the world has sea");
        let (sx, sz) = map.heights().coords(sea_vertex);
        let sea_point = map.heights().world_of(sx, sz);
        let sample = map
            .water_surface_at(sea_point.x, sea_point.y)
            .expect("the sea must sample as water");
        assert_eq!(sample.kind, WaterKind::Sea);
        assert_eq!(sample.salinity, Salinity::Marine);
        assert!(sample.surface_y.is_finite() && sample.depth > 0.0);

        let lake = map
            .lakes()
            .iter()
            .find(|l| !l.cells.is_empty())
            .expect("the world has lakes");
        let (lx, lz) = map.heights().coords(lake.cells[0]);
        let lake_point = map.heights().world_of(lx, lz);
        let sample = map
            .water_surface_at(lake_point.x, lake_point.y)
            .expect("a lake cell must sample as water");
        assert_eq!(sample.kind, WaterKind::Lake);
        assert!(sample.surface_y > 0.0);

        let river_vertex = map
            .river_paths()
            .iter()
            .flat_map(|p| p.iter().copied().next())
            .find(|&v| !map.ocean_mask()[v])
            .expect("the world has rivers");
        let (rx, rz) = map.heights().coords(river_vertex);
        let river_point = map.heights().world_of(rx, rz);
        let sample = map
            .water_surface_at(river_point.x, river_point.y)
            .expect("a river cell must sample as water");
        assert_eq!(sample.kind, WaterKind::River);
        assert_eq!(sample.salinity, Salinity::Freshwater);

        let dry = (0..map.heights().len())
            .filter(|&i| {
                let (x, z) = map.heights().coords(i);
                let p = map.heights().world_of(x, z);
                map.water_surface_at(p.x, p.y).is_none()
            })
            .count();
        assert!(dry > 0, "dry land must report no water");
    }

    #[test]
    fn water_surface_at_is_wrap_aware_on_the_torus() {
        let map = TerrainMap::new(golden_config(WorldTopology::FiniteTorus));
        let size = map.size();
        for x in [-55.0, 0.0, 55.0] {
            for z in [-40.0, 30.0] {
                let here = map.water_surface_at(x, z);
                let wrapped = map.water_surface_at(x + size, z - size);
                match (here, wrapped) {
                    (Some(a), Some(b)) => {
                        assert!((a.surface_y - b.surface_y).abs() < 1e-4, "wrap mismatch at {x},{z}")
                    }
                    (None, None) => {}
                    _ => panic!("wrap disagreement at {x},{z}: {here:?} vs {wrapped:?}"),
                }
            }
        }
    }

    #[test]
    fn landlocked_lakes_are_never_marked_as_sea() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let map = full_world(topology);
            assert!(!map.lakes().is_empty(), "{topology:?} world must have lakes");
            for lake in map.lakes() {
                for &vertex in &lake.cells {
                    assert!(
                        !map.ocean_mask()[vertex],
                        "lake vertex {vertex} must not be ocean-connected ({topology:?})"
                    );
                }
            }
        }
    }
}
