use bevy::prelude::Vec2;

use crate::terrain::field::ScalarField;
use crate::terrain::hydrology::{
    self, CraterConfig, ErosionConfig, Hydrology, Lake, PrecipitationInput, Waterfall, repose_tan,
};
use crate::terrain::macro_landform::{self, MacroField, SEA_LEVEL};
use crate::terrain::noise::{
    NoiseAlgorithm, NoiseContext, NoiseEngine, NoiseShape, blend_shapes, select_algorithm,
    shape_for,
};
use crate::terrain::params::{
    BaseGradient, BaseHeightBand, CellParameters, MacroContext, Substrate, relax_parameters,
    roll_cell_parameters,
};
use crate::terrain::region::{Region, resolve_region, sub_variant_index};
use crate::terrain::season::{FloodZone, SeaLevelSeason, Season, coastal_erosion};
use crate::terrain::seed::{WorldSeed, STAGE_BASE_GRADIENT, STAGE_NOISE};
use crate::terrain::topology::WorldTopology;
use crate::terrain::voronoi::{VertexBlendCache, VoronoiLayout};
use crate::terrain::water;

use super::config::{TerrainCell, TerrainConfig};

const MIN_WATERFALL_DROP_METRES: f32 = 4.0;
const REPOSE_RELAXATION_ITERATIONS: usize = 6;
const COASTAL_EROSION_STRENGTH: f32 = 0.55;
const RIVER_MARK_ACCUMULATION_FRACTION: f32 = 0.25;

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
    pub hardness: ScalarField,
    pub flow: hydrology::FlowField,
}

pub struct SeasonalFeatureStage {
    pub heights: ScalarField,
    pub flood_zone: FloodZone,
}

pub(crate) fn compute_ocean_mask(
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
            let mut algorithm_weights: Vec<(NoiseAlgorithm, f32)> =
                Vec::with_capacity(weights.len());
            let mut shape_weights: Vec<(NoiseShape, f32)> = Vec::with_capacity(weights.len());
            for &(id, weight) in weights {
                let (Some(algorithm), Some(shape)) = (algorithms.get(id), shapes.get(id)) else {
                    continue;
                };
                algorithm_weights.push((*algorithm, weight));
                shape_weights.push((*shape, weight));
            }
            let shape = blend_shapes(&shape_weights);
            let noise = engine.sample_blend_with(&algorithm_weights, point.x, point.y, &ctx, &shape);
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

    let precipitation = precipitation_field(
        &heights,
        &cache,
        &cell_of_vertex,
        macro_field,
        parameters,
        config.relief,
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

    let hardness = substrate_hardness_field(&cache, &heights, &parameters.cells);
    let (step, erode_radius, deposit_radius) = hydrology::erosion_geometry(heights.spacing());
    let droplets = hydrology::scaled_droplet_count(config.droplet_count, heights.dim);

    let mut carving = hydrology::priority_flood(&heights, SEA_LEVEL);
    hydrology::flow_accumulation(&mut carving, &precipitation);

    let rounds = config.erosion_iterations.max(1);
    for round in 0..rounds {
        hydrology::carve_rivers(
            &mut heights,
            &carving,
            config.river_threshold,
            config.erosion_strength,
            Some(&hardness),
        );
        hydrology::droplet_erosion_with_hardness(
            &mut heights,
            &precipitation,
            config.seed,
            &ErosionConfig {
                droplet_count: droplets,
                iterations: 1,
                erode_radius,
                deposit_radius,
                step,
                salt: round as u64,
                ..Default::default()
            },
            Some(&hardness),
        );
        if round + 1 < rounds {
            carving = hydrology::priority_flood(&heights, SEA_LEVEL);
            hydrology::flow_accumulation(&mut carving, &precipitation);
        }
    }

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
    hydrology::thermal_relax_repose(&mut heights, &repose, REPOSE_RELAXATION_ITERATIONS);

    let sea_top = SeaLevelSeason::default().level_at(Season::Summer);
    let cell_parameters: Vec<CellParameters> =
        parameters.cells.iter().map(|cell| cell.parameters).collect();
    let wind_gate = hydrology::wind_erodibility_field(&cache, &heights, &cell_parameters, sea_top);
    hydrology::apply_wind_erosion(
        &mut heights,
        macro_field.prevailing_wind,
        config.wind_erosion,
        config.wind_iterations,
        Some(&wind_gate),
    );

    hydrology::apply_craters(&mut heights, &eroded_craters, &|_| true);

    carving = hydrology::priority_flood(&heights, SEA_LEVEL);
    hydrology::flow_accumulation(&mut carving, &precipitation);
    let ocean = compute_ocean_mask(
        &heights,
        config.topology,
        &macro_field.bands,
        &cell_of_vertex,
        sea_top,
    );
    let mut waterfalls = hydrology::find_waterfalls(
        &heights,
        &carving,
        config.river_threshold,
        MIN_WATERFALL_DROP_METRES,
        &ocean,
    );
    hydrology::carve_plunge_pools(&mut heights, &carving, &mut waterfalls);

    let settled = {
        let mut settled = hydrology::priority_flood(&heights, SEA_LEVEL);
        hydrology::flow_accumulation(&mut settled, &precipitation);
        settled
    };
    let ocean = compute_ocean_mask(
        &heights,
        config.topology,
        &macro_field.bands,
        &cell_of_vertex,
        sea_top,
    );
    let lakes = hydrology::find_lakes(&heights, &settled, SEA_LEVEL, &ocean);
    let river_paths = hydrology::river_paths(&settled, config.river_threshold, &ocean);
    let flow = hydrology::bake_flow_vectors(&heights, &settled);

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
        hardness,
        flow,
    }
}

pub(crate) fn precipitation_field(
    heights: &ScalarField,
    cache: &VertexBlendCache,
    cell_of_vertex: &[usize],
    macro_field: &MacroField,
    parameters: &ParameterStage,
    relief: f32,
) -> ScalarField {
    let cell_precipitation: Vec<_> = parameters
        .cells
        .iter()
        .map(|cell| cell.parameters.precipitation)
        .collect();
    hydrology::generate_precipitation_blended(
        heights,
        &PrecipitationInput {
            cell_of_vertex,
            cell_precipitation: &cell_precipitation,
            cell_rain_shadow: &macro_field.rain_shadow,
            wind: macro_field.prevailing_wind,
            relief,
        },
        Some(cache),
    )
}

pub(crate) fn substrate_hardness_field(
    cache: &VertexBlendCache,
    heights: &ScalarField,
    cells: &[TerrainCell],
) -> ScalarField {
    let mut field = ScalarField::new(heights.dim, heights.size, heights.topology);
    for index in 0..heights.len() {
        let mut total = 0.0f32;
        let mut sum = 0.0f32;
        for &(cell_id, weight) in cache.weights_at(index) {
            let Some(cell) = cells.get(cell_id) else {
                continue;
            };
            sum += cell.parameters.substrate.hardness() * weight;
            total += weight;
        }
        let value = if total > 0.0 {
            sum / total
        } else {
            cache
                .cell_at(index)
                .and_then(|id| cells.get(id))
                .map(|cell| cell.parameters.substrate.hardness())
                .unwrap_or(Substrate::SiltClay.hardness())
        };
        field.set_at(index, value);
    }
    field
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

pub(crate) fn mark_hydrology(
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
            let threshold = settled.accumulation.max() * RIVER_MARK_ACCUMULATION_FRACTION;
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

pub fn run_seasonal_stage(config: &TerrainConfig, heights: ScalarField) -> SeasonalFeatureStage {
    run_seasonal_stage_with(config, heights, None)
}

pub fn run_seasonal_stage_with(
    config: &TerrainConfig,
    mut heights: ScalarField,
    hardness: Option<&ScalarField>,
) -> SeasonalFeatureStage {
    let levels = SeaLevelSeason::default();
    let all: Vec<f32> = Season::ALL.iter().map(|s| levels.level_at(*s)).collect();
    let flood_zone = FloodZone::around_sea_levels(&all, config.flood_margin);
    coastal_erosion(&mut heights, &flood_zone, COASTAL_EROSION_STRENGTH, hardness);
    SeasonalFeatureStage {
        heights,
        flood_zone,
    }
}
