#[allow(dead_code)]
mod cache;
mod config;
mod query;
mod stages;
mod surface;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use cache::{
    CachedWaterfall, CachedWorld, HeightmapError, StaticHeightmap, TerrainCache, TerrainSource,
};
pub use config::{TerrainCell, TerrainConfig};
pub use stages::{
    EvolvingFeatureStage, ParameterStage, run_base_stage, run_evolving_stage, run_noise_stage,
    run_parameter_stage, run_seasonal_stage, run_seasonal_stage_with,
};
#[allow(unused_imports)]
pub use surface::{SurfaceInfo, SurfaceWater};

use bevy::prelude::{Resource, Vec2};

use crate::terrain::field::ScalarField;
use crate::terrain::hydrology::{self, Lake, Waterfall};
use crate::terrain::macro_landform::{self, MacroConfig, MacroField, SEA_LEVEL};
use crate::terrain::params::{CellParameters, Geothermal, Salinity, Temperature};
use crate::terrain::region::{Region, resolve_region, sub_variant_index};
use crate::terrain::season::{FloodZone, SeaLevelSeason, Season, SeasonClock, blended_delta};
use crate::terrain::seed::WorldSeed;
use crate::terrain::voronoi::{VoronoiConfig, VoronoiLayout};
use crate::terrain::water::{self, WaterBody, WaterField};

use stages::{compute_ocean_mask, mark_hydrology, precipitation_field, substrate_hardness_field};

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
    hardness: ScalarField,
    season_clock: SeasonClock,
    flow: hydrology::FlowField,
}

impl Default for TerrainMap {
    fn default() -> Self {
        Self::new(TerrainConfig::default())
    }
}

impl TerrainMap {
    pub fn new(config: TerrainConfig) -> Self {
        let config = config.sanitized();
        let (layout, macro_field) = Self::stage_inputs(&config);

        let parameters = run_parameter_stage(&config, &layout, &macro_field);
        let base = run_base_stage(&config, &layout, &parameters);
        let noise = run_noise_stage(&config, &layout, &parameters, base);
        let evolving = run_evolving_stage(&config, &layout, &macro_field, &parameters, noise);
        Self::assemble(config, layout, macro_field, parameters, evolving)
    }

    pub fn stage_inputs(config: &TerrainConfig) -> (VoronoiLayout, MacroField) {
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
        let layout = layout.with_cell_weights(&macro_landform::cell_power_weights(
            &macro_field,
            &layout,
            &macro_config,
        ));
        (layout, macro_field)
    }

    #[allow(dead_code)]
    pub fn new_cached(config: TerrainConfig, cache: Option<&TerrainCache>) -> Self {
        let config = config.sanitized();
        let Some(cache) = cache else {
            return Self::new(config);
        };
        if let Some(cached) = cache.load(&config) {
            if let Some(map) = Self::from_cached(config.clone(), &cached) {
                return map;
            }
        }
        let map = Self::new(config.clone());
        cache.store(&config, &map);
        map
    }

    #[allow(dead_code)]
    pub fn compact() -> Self {
        Self::new(TerrainConfig::compact())
    }

    #[allow(dead_code)]
    pub fn from_source(source: &TerrainSource) -> Result<Self, HeightmapError> {
        match source {
            TerrainSource::Generated(config) => Ok(Self::new(config.clone())),
            TerrainSource::StaticAsset(asset) => Self::from_static_heightmap(asset),
        }
    }

    #[allow(dead_code)]
    pub fn from_static_heightmap(asset: &StaticHeightmap) -> Result<Self, HeightmapError> {
        let heights = asset.load_field()?;
        let config = TerrainConfig {
            size: asset.size,
            subdivisions: heights.dim,
            topology: asset.topology,
            seed: WorldSeed::new(asset.seed),
            ..TerrainConfig::default()
        }
        .sanitized();
        if config.subdivisions != heights.dim {
            return Err(HeightmapError::Unsupported {
                detail: format!(
                    "a {}x{} grid cannot be used as a terrain grid",
                    heights.dim, heights.dim
                ),
            });
        }
        let (layout, macro_field) = Self::stage_inputs(&config);
        let parameters = run_parameter_stage(&config, &layout, &macro_field);
        let cache = layout.vertex_cache(heights.dim, config.size, config.topology);
        let cell_of_vertex: Vec<usize> = (0..heights.len())
            .map(|index| cache.cell_at(index).unwrap_or(0))
            .collect();
        let precipitation =
            precipitation_field(&heights, &cache, &cell_of_vertex, &macro_field, &parameters);
        let cached = CachedWorld {
            dim: heights.dim as u32,
            size: config.size,
            topology: config.topology,
            heights: heights.values.clone(),
            precipitation: precipitation.values.clone(),
            waterfalls: Vec::new(),
        };
        Self::from_cached(config, &cached).ok_or(HeightmapError::Unsupported {
            detail: format!("{} does not match the terrain grid", asset.path.display()),
        })
    }

    fn assemble(
        config: TerrainConfig,
        layout: VoronoiLayout,
        macro_field: MacroField,
        parameters: ParameterStage,
        evolving: EvolvingFeatureStage,
    ) -> Self {
        let seasonal =
            run_seasonal_stage_with(&config, evolving.heights.clone(), Some(&evolving.hardness));
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
            hardness: evolving.hardness,
            season_clock: SeasonClock::default(),
            flow: evolving.flow,
        };
        let clock = map.season_clock.clone();
        map.apply_seasonal_parameters(&clock);
        map.refresh_water();
        map
    }

    #[allow(dead_code)]
    fn from_cached(config: TerrainConfig, cached: &CachedWorld) -> Option<Self> {
        let dim = config.subdivisions;
        if cached.dim as usize != dim
            || (cached.size - config.size).abs() > 1e-3
            || cached.topology != config.topology
            || cached.heights.len() != dim * dim
            || cached.precipitation.len() != dim * dim
        {
            return None;
        }
        let (layout, macro_field) = TerrainMap::stage_inputs(&config);
        let parameters = run_parameter_stage(&config, &layout, &macro_field);

        let mut heights = ScalarField::new(dim, config.size, config.topology);
        for (index, value) in cached.heights.iter().enumerate() {
            heights.set_at(index, *value);
        }
        let mut precipitation = ScalarField::new(dim, config.size, config.topology);
        for (index, value) in cached.precipitation.iter().enumerate() {
            precipitation.set_at(index, *value);
        }

        let cache = layout.vertex_cache(dim, config.size, config.topology);
        let cell_of_vertex: Vec<usize> = (0..heights.len())
            .map(|index| cache.cell_at(index).unwrap_or(0))
            .collect();
        let hardness = substrate_hardness_field(&cache, &heights, &parameters.cells);

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
        let waterfalls = cached.to_waterfalls(heights.len());
        let river_paths = hydrology::river_paths(&settled, config.river_threshold, &ocean);
        let cells = mark_hydrology(
            &parameters.cells, &layout, &settled, &lakes, &heights, config.seed,
        );
        let flow = hydrology::bake_flow_vectors(&heights, &settled);

        Some(Self::assemble(
            config,
            layout,
            macro_field,
            parameters,
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
                hardness,
                flow,
            },
        ))
    }

    pub fn footprint_bytes(&self) -> usize {
        let field_bytes = |field: &ScalarField| field.values.len() * size_of::<f32>();
        let mut bytes = field_bytes(&self.heights)
            + field_bytes(&self.base_heights)
            + field_bytes(&self.precipitation)
            + field_bytes(&self.accumulation)
            + field_bytes(&self.filled)
            + field_bytes(&self.hardness)
            + field_bytes(&self.flow.direction_x)
            + field_bytes(&self.flow.direction_y)
            + field_bytes(&self.flow.slope)
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
        self.season_clock.set_season(season);
        self.apply_seasonal_parameters(&self.season_clock.clone());
        let seasonal = run_seasonal_stage_with(
            &self.config,
            self.base_heights.clone(),
            Some(&self.hardness),
        );
        self.heights = seasonal.heights;
        self.flood_zone = seasonal.flood_zone;
        self.sea_level_now = self.sea_levels.level_at(season);
        self.refresh_water();
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn sync_season_state(&mut self, clock: &SeasonClock) -> bool {
        let changed = clock.season != self.season;
        if changed {
            self.season = clock.season;
            self.apply_seasonal_parameters(clock);
            let seasonal = run_seasonal_stage_with(
                &self.config,
                self.base_heights.clone(),
                Some(&self.hardness),
            );
            self.heights = seasonal.heights;
            self.flood_zone = seasonal.flood_zone;
            self.refresh_water();
            self.generation = self.generation.wrapping_add(1);
        }
        self.season_clock = clock.clone();
        self.apply_seasonal_parameters(clock);
        self.sea_level_now = self.sea_levels.continuous_level(clock.cycle_progress());
        changed
    }

    pub fn apply_seasonal_parameters(&mut self, clock: &SeasonClock) {
        for index in 0..self.cells.len() {
            let (Some(base), Some(base_region)) = (
                self.base_cell_parameters.get(index).copied(),
                self.base_cell_regions.get(index).copied(),
            ) else {
                continue;
            };
            let parameters = blended_delta(clock, base_region.definition().tier).apply(&base);
            let region = resolve_region(&parameters);
            if let Some(cell) = self.cells.get_mut(index) {
                cell.sub_variant =
                    sub_variant_index(region, &parameters, self.config.seed, cell.id as u64);
                cell.parameters = parameters;
                cell.region = region;
            }
        }
    }

    #[allow(dead_code)]
    pub fn regenerate(&mut self) {
        let config = self.config.clone();
        *self = Self::new(config);
    }

    #[allow(dead_code)]
    pub fn reseed(&mut self, seed: u64) {
        self.config.seed = WorldSeed::new(seed);
        self.regenerate();
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
        let seasonal =
            run_seasonal_stage_with(&self.config, evolving.heights.clone(), Some(&evolving.hardness));

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
        self.hardness = evolving.hardness;
        self.flow = evolving.flow;
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
        let cell_arid: Vec<bool> = self.cells.iter().map(|cell| cell.parameters.is_arid()).collect();
        let lake_salinity = water::classify_lake_salinity(
            &self.lakes,
            self.sea_level(),
            &self.cell_of_vertex,
            &cell_arid,
        );
        self.lake_salinity = lake_salinity;
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
}

fn map_base_parameters(cells: &[TerrainCell]) -> Vec<CellParameters> {
    cells.iter().map(|cell| cell.parameters).collect()
}

#[cfg(test)]
fn fnv_mix(hash: &mut u64, value: f32) {
    let bits = (value * 1000.0).round() as i64 as u64;
    for byte in bits.to_le_bytes() {
        *hash ^= byte as u64;
        *hash = hash.wrapping_mul(0x1000_0000_0000_01b3);
    }
}
