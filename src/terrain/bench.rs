use std::time::Instant;

use bevy::prelude::{Assets, Image, IVec2};

use crate::terrain::field::ScalarField;
use crate::terrain::hydrology::{self, ErosionConfig};
use crate::terrain::macro_landform::{self, MacroConfig, SEA_LEVEL};
use crate::terrain::map::{
    TerrainConfig, TerrainMap, run_base_stage, run_evolving_stage, run_noise_stage,
    run_parameter_stage, run_seasonal_stage,
};
use crate::terrain::material::DebugMaterialMode;
use crate::terrain::seed::WorldSeed;
use crate::terrain::topology::WorldTopology;
use crate::terrain::voronoi::{VoronoiConfig, VoronoiLayout};
use crate::terrain::chunk::{
    DEFAULT_CHUNK_SIZE, bake_controls, build_chunk_mesh, chunk_heightfield, chunk_mesh_steps,
};
use crate::terrain::DebugView;

pub const BUDGETS: [(&str, f64); 13] = [
    ("voronoi", 200.0),
    ("macro", 200.0),
    ("parameters", 300.0),
    ("base", 600.0),
    ("noise", 1500.0),
    ("evolving", 4000.0),
    ("droplet_erosion", 6000.0),
    ("seasonal", 600.0),
    ("total_generation", 4500.0),
    ("chunk_mesh", 300.0),
    ("chunk_collider", 50.0),
    ("splat_bake", 600.0),
    ("chunk_upload_total", 50.0),
];

#[allow(dead_code)]
pub const CONTENTION_SLACK: f64 = 2.0;

pub struct StageTiming {
    pub name: &'static str,
    pub millis: f64,
}

pub struct TerrainBenchmark {
    pub timings: Vec<StageTiming>,
    pub footprint_bytes: usize,
}

impl TerrainBenchmark {
    pub fn total_ms(&self) -> f64 {
        self.timings.iter().map(|timing| timing.millis).sum()
    }

    pub fn footprint_mib(&self) -> f64 {
        self.footprint_bytes as f64 / (1024.0 * 1024.0)
    }

    #[cfg(test)]
    pub fn stage(&self, name: &str) -> Option<f64> {
        self.timings
            .iter()
            .find(|timing| timing.name == name)
            .map(|timing| timing.millis)
    }

    pub fn budget(name: &str) -> Option<f64> {
        BUDGETS
            .iter()
            .find(|(stage, _)| *stage == name)
            .map(|(_, budget)| *budget)
    }

    pub fn violations(&self) -> Vec<(&'static str, f64, f64)> {
        self.violations_under(1.0)
    }

    pub fn violations_under(&self, slack: f64) -> Vec<(&'static str, f64, f64)> {
        self.timings
            .iter()
            .filter_map(|timing| {
                Self::budget(timing.name)
                    .map(|budget| (timing.name, timing.millis, budget * slack))
            })
            .filter(|(_, millis, ceiling)| millis > ceiling)
            .collect()
    }

    pub fn report(&self) -> String {
        let lines: Vec<String> = self
            .timings
            .iter()
            .map(|timing| {
                let budget = Self::budget(timing.name).unwrap_or(0.0);
                let flag = if timing.millis > budget { "OVER" } else { "ok" };
                format!(
                    "  {:<18} {:>8.1} ms / {:>8.1} ms  {flag}",
                    timing.name, timing.millis, budget
                )
            })
            .collect();
        format!(
            "terrain benchmark (total {:.1} ms, footprint {:.1} MiB)\n{}",
            self.total_ms(),
            self.footprint_mib(),
            lines.join("\n")
        )
    }
}

fn timing(name: &'static str, start: Instant) -> StageTiming {
    StageTiming {
        name,
        millis: start.elapsed().as_secs_f64() * 1000.0,
    }
}

pub fn run(config: &TerrainConfig) -> TerrainBenchmark {
    let mut timings = Vec::new();

    let start = Instant::now();
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
    timings.push(timing("voronoi", start));

    let start = Instant::now();
    let macro_config = MacroConfig {
        size: config.size,
        topology: config.topology,
        sea_level: SEA_LEVEL,
        relief: config.relief,
    };
    let macro_field = macro_landform::generate_macro_field(
        config.seed,
        &layout,
        config.shape,
        &macro_config,
    );
    let layout = layout.with_cell_weights(&macro_landform::cell_power_weights(
        &macro_field,
        &layout,
        &macro_config,
    ));
    timings.push(timing("macro", start));

    let start = Instant::now();
    let parameters = run_parameter_stage(config, &layout, &macro_field);
    timings.push(timing("parameters", start));

    let start = Instant::now();
    let base = run_base_stage(config, &layout, &parameters);
    timings.push(timing("base", start));

    let start = Instant::now();
    let noise = run_noise_stage(config, &layout, &parameters, base);
    timings.push(timing("noise", start));

    let start = Instant::now();
    let evolving = run_evolving_stage(config, &layout, &macro_field, &parameters, noise);
    timings.push(timing("evolving", start));

    let start = Instant::now();
    let mut erosion_field = ScalarField::new(200, 400.0, WorldTopology::FiniteTorus);
    let mut rainfall = ScalarField::new(200, 400.0, WorldTopology::FiniteTorus);
    rainfall.fill(0.6);
    for z in 0..200 {
        for x in 0..200 {
            let dx = x as f32 - 100.0;
            let dz = z as f32 - 100.0;
            let radius = (dx * dx + dz * dz).sqrt();
            erosion_field.set(x, z, x as f32 * 0.3 + (14.0 - radius * 0.5).max(0.0));
        }
    }
    hydrology::droplet_erosion(
        &mut erosion_field,
        &rainfall,
        WorldSeed::new(20_261_120),
        &ErosionConfig::default(),
    );
    timings.push(timing("droplet_erosion", start));

    let start = Instant::now();
    let _seasonal = run_seasonal_stage(config, evolving.heights.clone());
    timings.push(timing("seasonal", start));

    let start = Instant::now();
    let map = TerrainMap::new(config.clone());
    timings.push(timing("total_generation", start));

    let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
    let start = Instant::now();
    let _mesh = build_chunk_mesh(&map, IVec2::ZERO, chunk_size, DebugView::None);
    timings.push(timing("chunk_mesh", start));

    let start = Instant::now();
    let _collider = chunk_heightfield(
        &map,
        IVec2::ZERO,
        chunk_size,
        chunk_mesh_steps(&map, IVec2::ZERO, chunk_size),
    );
    timings.push(timing("chunk_collider", start));

    let wanted = map.chunk_keys(chunk_size);

    let start = Instant::now();
    let mut images: Assets<Image> = Assets::default();
    let _controls = bake_controls(
        &map,
        DebugMaterialMode::Textured,
        DebugView::None,
        &wanted,
        &mut images,
    );
    timings.push(timing("splat_bake", start));

    let start = Instant::now();
    for key in &wanted {
        let _mesh = build_chunk_mesh(&map, *key, chunk_size, DebugView::None);
        let steps = chunk_mesh_steps(&map, *key, chunk_size);
        let _collider = chunk_heightfield(&map, *key, chunk_size, steps);
    }
    timings.push(timing("chunk_upload_total", start));

    let footprint_bytes = map.footprint_bytes();
    TerrainBenchmark {
        timings,
        footprint_bytes,
    }
}

pub fn run_default() -> TerrainBenchmark {
    run(&TerrainConfig::default())
}

pub fn large_config() -> TerrainConfig {
    TerrainConfig {
        subdivisions: 1024,
        ..TerrainConfig::default()
    }
}

pub fn run_large() -> TerrainBenchmark {
    run(&large_config())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::topology::WorldTopology;

    #[test]
    fn every_benchmarked_stage_has_a_budget() {
        let benchmark = run(&TerrainConfig::compact());
        let unbudgeted: Vec<&str> = benchmark
            .timings
            .iter()
            .filter(|timing| TerrainBenchmark::budget(timing.name).is_none())
            .map(|timing| timing.name)
            .collect();
        assert!(unbudgeted.is_empty(), "stages without budgets: {unbudgeted:?}");
    }

    #[test]
    fn compact_generation_stays_inside_its_budget() {
        let benchmark = run(&TerrainConfig::compact());
        assert!(
            benchmark.violations_under(CONTENTION_SLACK).is_empty(),
            "budget violations:\n{}",
            benchmark.report()
        );
    }

    #[test]
    fn torus_generation_stays_inside_its_budget() {
        let config = TerrainConfig::compact().with_topology(WorldTopology::FiniteTorus);
        let benchmark = run(&config);
        assert!(
            benchmark.violations_under(CONTENTION_SLACK).is_empty(),
            "budget violations:\n{}",
            benchmark.report()
        );
    }

    #[test]
    fn stage_timings_are_all_positive_and_summed() {
        let benchmark = run(&TerrainConfig::compact());
        assert_eq!(benchmark.timings.len(), BUDGETS.len());
        assert!(benchmark.timings.iter().all(|t| t.millis >= 0.0));
        assert!(benchmark.total_ms() > 0.0);
        assert!(benchmark.stage("chunk_mesh").is_some());
        assert!(benchmark.stage("splat_bake").is_some());
        assert!(benchmark.stage("chunk_upload_total").is_some());
        assert!(benchmark.footprint_bytes > 0);
        assert!(benchmark.report().contains("terrain benchmark"));
    }

    #[test]
    fn default_generation_stays_inside_its_budget() {
        let benchmark = run_default();
        assert!(
            benchmark.violations_under(CONTENTION_SLACK).is_empty(),
            "budget violations:\n{}",
            benchmark.report()
        );
    }

    #[test]
    #[ignore = "times a 1024x1024 world; run with --ignored to re-record budgets"]
    fn large_world_benchmark_reports() {
        let benchmark = run_large();
        assert!(benchmark.timings.iter().all(|t| t.millis >= 0.0));
        assert!(benchmark.footprint_bytes > 1024 * 1024);
        println!("{}", benchmark.report());
    }

    #[test]
    fn a_violation_is_reported_against_its_budget() {
        let benchmark = TerrainBenchmark {
            timings: vec![
                StageTiming { name: "noise", millis: 9_000.0 },
                StageTiming { name: "macro", millis: 1.0 },
            ],
            footprint_bytes: 0,
        };
        let violations = benchmark.violations();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].0, "noise");
    }

    #[test]
    fn contention_slack_absorbs_a_loaded_test_harness() {
        let benchmark = TerrainBenchmark {
            timings: vec![StageTiming {
                name: "noise",
                millis: 2_500.0,
            }],
            footprint_bytes: 0,
        };
        assert_eq!(benchmark.violations().len(), 1);
        assert!(
            benchmark.violations_under(CONTENTION_SLACK).is_empty(),
            "a stage inside budget * {CONTENTION_SLACK} must not fail the harness"
        );
    }
}
