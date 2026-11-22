use std::path::PathBuf;

use super::*;

use crate::terrain::macro_landform::WorldShape;
use crate::terrain::noise::{NoiseShape, shape_for};
use crate::terrain::topology::WorldTopology;

    const GOLDEN_SEED: u64 = 20_261_120;
    const GOLDEN_FINITE: u64 = 0x496B_ED77_DADE_79E1;
    const GOLDEN_TORUS: u64 = 0x1B5B_C536_BAEE_65E5;

    struct Stages {
        parameters: ParameterStage,
        base: ScalarField,
        noise: ScalarField,
        evolving_heights: ScalarField,
        seasonal_heights: ScalarField,
        flood_zone: FloodZone,
    }

    fn run_all_stages(config: &TerrainConfig) -> Stages {
        let (layout, macro_field) = TerrainMap::stage_inputs(config);
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
        let (layout, macro_field) = TerrainMap::stage_inputs(&config);
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

    #[test]
    fn sanitized_repairs_impossible_configs() {
        let default = TerrainConfig::default();
        let broken = TerrainConfig {
            size: f32::NAN,
            subdivisions: 0,
            cell_grid_dim: 0,
            amplitude: f32::INFINITY,
            cell_base_height_range: f32::NAN,
            relief: f32::NAN,
            crater_degradation: f32::NAN,
            erosion_strength: f32::NAN,
            repose_scale: f32::NAN,
            wind_erosion: f32::NAN,
            river_threshold: f32::NAN,
            blend_width: f32::NAN,
            flood_margin: f32::NAN,
            ..TerrainConfig::compact()
        };
        let fixed = broken.sanitized();
        assert_eq!(fixed.size, default.size);
        assert_eq!(fixed.subdivisions, default.subdivisions);
        assert_eq!(fixed.cell_grid_dim, default.cell_grid_dim);
        assert_eq!(fixed.amplitude, default.amplitude);
        assert_eq!(fixed.cell_base_height_range, default.cell_base_height_range);
        assert_eq!(fixed.relief, default.relief);
        assert_eq!(fixed.crater_degradation, default.crater_degradation);
        assert_eq!(fixed.erosion_strength, default.erosion_strength);
        assert_eq!(fixed.repose_scale, default.repose_scale);
        assert_eq!(fixed.wind_erosion, default.wind_erosion);
        assert_eq!(fixed.river_threshold, default.river_threshold);
        assert_eq!(fixed.blend_width, default.blend_width);
        assert_eq!(fixed.flood_margin, default.flood_margin);
        assert_eq!(fixed.seed, default.seed);

        let healthy = TerrainConfig::compact();
        assert_eq!(healthy.sanitized().subdivisions, healthy.subdivisions);
        assert_eq!(healthy.sanitized().size, healthy.size);
        assert_eq!(healthy.sanitized().droplet_count, healthy.droplet_count);
        assert_eq!(
            fixed.sanitized().subdivisions,
            fixed.subdivisions,
            "sanitizing twice must change nothing"
        );
    }

    #[test]
    fn a_config_with_non_finite_parameters_still_generates() {
        let broken = TerrainConfig {
            relief: f32::NAN,
            erosion_strength: f32::NAN,
            wind_erosion: f32::NAN,
            river_threshold: f32::NAN,
            ..TerrainConfig::compact()
        };
        let map = TerrainMap::new(broken);
        assert!(map.heights().values.iter().all(|value| value.is_finite()));
        assert_eq!(map.heights().dim, TerrainConfig::compact().subdivisions);
        assert!(map.max_height_extent() > 0.0);
    }

    fn pearson(xs: &[f32], ys: &[f32]) -> f32 {
        let count = xs.len() as f32;
        if count < 2.0 {
            return 0.0;
        }
        let mean_x = xs.iter().sum::<f32>() / count;
        let mean_y = ys.iter().sum::<f32>() / count;
        let mut cross = 0.0f32;
        let mut spread_x = 0.0f32;
        let mut spread_y = 0.0f32;
        for index in 0..xs.len() {
            let dx = xs[index] - mean_x;
            let dy = ys[index] - mean_y;
            cross += dx * dy;
            spread_x += dx * dx;
            spread_y += dy * dy;
        }
        if spread_x <= 0.0 || spread_y <= 0.0 {
            return 0.0;
        }
        cross / (spread_x * spread_y).sqrt()
    }

    #[test]
    fn the_hardness_field_follows_the_per_cell_substrate() {
        let map = TerrainMap::new(golden_config(WorldTopology::Finite));
        let mut sampled: Vec<f32> = Vec::new();
        let mut expected: Vec<f32> = Vec::new();
        for cell in map.cells() {
            let value = map.hardness_at(cell.center.x, cell.center.y);
            assert!(
                value.is_finite() && (0.0..=1.0).contains(&value),
                "hardness must stay normalised: {value}"
            );
            sampled.push(value);
            expected.push(cell.parameters.substrate.hardness());
        }
        assert!(sampled.len() > 4);
        let low = sampled.iter().cloned().fold(f32::INFINITY, f32::min);
        let high = sampled.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!(high - low > 0.1, "hardness must vary across the world: {low}..{high}");
        let correlation = pearson(&sampled, &expected);
        assert!(
            correlation > 0.4,
            "sampled hardness must track the substrate it was blended from: r={correlation}"
        );
    }

    #[test]
    fn slope_at_interpolates_between_lattice_nodes() {
        let map = TerrainMap::new(golden_config(WorldTopology::Finite));
        let spacing = map.heights().spacing();
        let mut differing = 0usize;
        let mut samples = 0usize;
        for z in 1..map.heights().dim - 1 {
            for x in 1..map.heights().dim - 1 {
                let node = map.heights().world_of(x, z);
                let at_node = map.slope_at(node.x, node.y);
                let inside_cell = map.slope_at(node.x + spacing * 0.5, node.y);
                assert!(at_node.is_finite() && inside_cell.is_finite());
                assert!(at_node >= 0.0 && inside_cell >= 0.0);
                if (at_node - inside_cell).abs() > 1e-6 {
                    differing += 1;
                }
                samples += 1;
            }
        }
        assert!(
            differing * 4 > samples,
            "slope must vary inside a cell, not only at lattice nodes: {differing}/{samples}"
        );
    }

    #[test]
    fn smooth_height_reproduces_the_field_at_lattice_nodes() {
        let map = TerrainMap::new(golden_config(WorldTopology::FiniteTorus));
        for z in (0..map.heights().dim).step_by(7) {
            for x in (0..map.heights().dim).step_by(7) {
                let node = map.heights().world_of(x, z);
                let exact = map.heights().get(x, z);
                let smooth = map.height_at_smooth(node.x, node.y);
                assert!(
                    (smooth - exact).abs() < 1e-3,
                    "catmull-rom must pass through the node ({x},{z}): {smooth} vs {exact}"
                );
            }
        }
    }

    #[test]
    fn snow_coverage_tracks_the_season_and_the_snowline() {
        let mut map = TerrainMap::new(golden_config(WorldTopology::Finite));
        let peak = (0..map.base_heights().len())
            .max_by(|a, b| {
                map.base_heights()
                    .get_at(*a)
                    .total_cmp(&map.base_heights().get_at(*b))
            })
            .unwrap();
        let (px, pz) = map.base_heights().coords(peak);
        let point = map.base_heights().world_of(px, pz);

        map.set_season(Season::Summer);
        let summer = map.snow_coverage_at(point.x, point.y);
        let summer_line = map.snowline();
        map.set_season(Season::Winter);
        let winter = map.snow_coverage_at(point.x, point.y);
        let winter_line = map.snowline();

        assert!((0.0..=1.0).contains(&summer) && (0.0..=1.0).contains(&winter));
        assert!(
            winter >= summer,
            "the summit must hold at least as much snow in winter: {summer} -> {winter}"
        );
        assert!(winter_line < summer_line, "the snowline must drop in winter");
        assert!(winter_line <= map.snowline_base());
    }

    #[test]
    fn snow_coverage_is_thinner_on_steep_ground() {
        let mut map = TerrainMap::new(golden_config(WorldTopology::Finite));
        map.set_season(Season::Winter);
        let mut flat = 0.0f32;
        let mut steep = 0.0f32;
        for z in 1..map.heights().dim - 1 {
            for x in 1..map.heights().dim - 1 {
                let node = map.heights().world_of(x, z);
                let slope = map.slope_at(node.x, node.y);
                let coverage = map.snow_coverage_at(node.x, node.y);
                if map.height_at(node.x, node.y) < map.sea_level() + 2.0 {
                    continue;
                }
                if slope < 0.02 {
                    flat = flat.max(coverage);
                } else if slope > 0.6 {
                    steep = steep.max(coverage);
                }
            }
        }
        assert!(flat > 0.0, "a winter world must have snow somewhere");
        assert!(
            steep <= flat,
            "steep ground cannot hold more snow than flat ground: {steep} vs {flat}"
        );
    }

    #[test]
    fn shore_overrides_only_apply_inside_the_flood_zone() {
        let mut map = TerrainMap::new(golden_config(WorldTopology::FiniteTorus));
        map.set_season(Season::Winter);
        let mut overridden = 0usize;
        for z in 0..map.heights().dim {
            for x in 0..map.heights().dim {
                let node = map.heights().world_of(x, z);
                let Some(layer) = map.shore_override_at(node.x, node.y) else {
                    continue;
                };
                assert!(
                    map.flood_zone().contains(map.height_at(node.x, node.y)),
                    "shore layer {layer:?} returned outside the flood zone"
                );
                overridden += 1;
            }
        }
        assert!(overridden > 0, "a winter world must have shore cells to override");
    }

    #[test]
    fn the_map_exposes_a_downstream_flow_field() {
        let map = TerrainMap::new(golden_config(WorldTopology::FiniteTorus));
        let mut draining = 0usize;
        for z in 0..map.heights().dim {
            for x in 0..map.heights().dim {
                let node = map.heights().world_of(x, z);
                let point = Vec2::new(node.x, node.y);
                let direction = map.flow_direction_at(point.x, point.y);
                let slope = map.flow_slope_at(point.x, point.y);
                assert!(slope.is_finite() && slope >= 0.0);
                assert!(direction.length() <= 1.05);
                if map.accumulation_at(point.x, point.y) > map.config().river_threshold
                    && direction.length_squared() > 1e-4
                {
                    draining += 1;
                }
            }
        }
        assert!(
            draining > 0,
            "river-carrying cells must have a baked flow direction"
        );
        assert!(!map.flow_field().slope.values.is_empty());
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("bevy_game_terrain_cache_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_disk_cache_returns_a_byte_identical_world() {
        let config = golden_config(WorldTopology::Finite);
        let dir = scratch_dir("round_trip");
        let cache = TerrainCache::new(&dir);

        let fresh = TerrainMap::new(config.clone());
        let cold = TerrainMap::new_cached(config.clone(), Some(&cache));
        assert!(cache.path_for(&config).exists(), "the miss must be written back");
        assert_eq!(fresh.checksum(), cold.checksum());
        assert_eq!(fresh.heights().values, cold.heights().values);
        assert_eq!(fresh.base_heights().values, cold.base_heights().values);
        assert_eq!(fresh.precipitation.values, cold.precipitation.values);

        let warm = TerrainMap::new_cached(config.clone(), Some(&cache));
        assert_eq!(fresh.checksum(), warm.checksum());
        assert_eq!(fresh.heights().values, warm.heights().values);
        assert_eq!(fresh.waterfalls().len(), warm.waterfalls().len());
        assert_eq!(fresh.lakes().len(), warm.lakes().len());
        assert_eq!(
            fresh.hardness_field().values,
            warm.hardness_field().values,
            "the cached world must rebuild the same hardness field"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unusable_cache_entry_falls_back_to_generation() {
        let config = golden_config(WorldTopology::Finite);
        let dir = scratch_dir("corrupt");
        let cache = TerrainCache::new(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(cache.path_for(&config), b"definitely not a terrain cache").unwrap();

        let fresh = TerrainMap::new(config.clone());
        let recovered = TerrainMap::new_cached(config.clone(), Some(&cache));
        assert_eq!(fresh.checksum(), recovered.checksum());
        assert_eq!(fresh.heights().values, recovered.heights().values);

        cache.store(&config, &fresh);
        let mut stale = cache.load(&config).expect("entry written");
        stale.dim = (config.subdivisions + 4) as u32;
        assert!(TerrainMap::from_cached(config.clone(), &stale).is_none());
        stale.dim = config.subdivisions as u32;
        stale.size += 1.0;
        assert!(TerrainMap::from_cached(config.clone(), &stale).is_none());
        stale.size = config.size;
        stale.topology = WorldTopology::Infinite;
        assert!(TerrainMap::from_cached(config.clone(), &stale).is_none());
        stale.topology = config.topology;
        assert!(TerrainMap::from_cached(config.clone(), &stale).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn surface_info_agrees_with_the_individual_queries() {
        let map = TerrainMap::new(golden_config(WorldTopology::FiniteTorus));
        let mut dry = 0usize;
        let mut wet = 0usize;
        for z in (0..map.heights().dim).step_by(3) {
            for x in (0..map.heights().dim).step_by(3) {
                let point = map.heights().world_of(x, z);
                let info = map.surface_info_at(point.x, point.y);
                assert_eq!(info.point, point);
                assert_eq!(info.height, map.height_at(point.x, point.y));
                assert_eq!(info.slope, map.slope_at(point.x, point.y));
                assert_eq!(info.moisture, map.precipitation_at(point.x, point.y));
                assert_eq!(info.hardness, map.hardness_at(point.x, point.y));
                assert_eq!(info.region, map.region_at(point.x, point.y));
                assert_eq!(info.snow, map.snow_coverage_at(point.x, point.y));
                assert_eq!(
                    info.substrate.is_some(),
                    map.parameters_at(point.x, point.y).is_some()
                );
                assert!((0.0..=1.0).contains(&info.snow));
                assert!((0.0..=1.0).contains(&info.hardness));
                assert_eq!(info.is_submerged(), map.water_surface_at(point.x, point.y).is_some());
                if info.is_dry() {
                    assert_eq!(info.depth, 0.0, "dry ground carries no water depth");
                    assert!(info.salinity.is_none());
                    dry += 1;
                } else {
                    assert!(info.salinity.is_some(), "flooded ground carries a salinity");
                    assert!(info.depth >= 0.0);
                    wet += 1;
                }
            }
        }
        assert!(
            dry > 0 && wet > 0,
            "the torus world must show both dry and flooded ground: {dry} dry, {wet} wet"
        );
    }

    #[test]
    fn surface_info_follows_the_season() {
        use crate::terrain::params::ParamLevel;
        let mut map = TerrainMap::new(golden_config(WorldTopology::Finite));
        let peak = (0..map.base_heights().len())
            .max_by(|a, b| {
                map.base_heights()
                    .get_at(*a)
                    .total_cmp(&map.base_heights().get_at(*b))
            })
            .unwrap();
        let (px, pz) = map.base_heights().coords(peak);
        let point = map.base_heights().world_of(px, pz);
        let summer = map.surface_info_at(point.x, point.y);
        map.set_season(Season::Winter);
        let winter = map.surface_info_at(point.x, point.y);
        assert_eq!(summer.substrate, winter.substrate, "bedrock does not melt");
        assert_eq!(summer.hardness, winter.hardness);
        assert!(winter.snow >= summer.snow);
        let winter_cold = winter.temperature.expect("a summit has a temperature").index();
        let summer_cold = summer.temperature.expect("a summit has a temperature").index();
        assert!(
            winter_cold >= summer_cold,
            "winter must be at least as cold as summer on the summit"
        );
    }

    #[test]
    fn a_generated_source_matches_the_plain_constructor() {
        let config = golden_config(WorldTopology::Finite);
        let from_source = TerrainMap::from_source(&TerrainSource::Generated(config.clone()))
            .expect("generated sources always load");
        assert_eq!(from_source.checksum(), TerrainMap::new(config).checksum());
    }

    #[test]
    fn the_cache_key_follows_everything_that_shapes_the_world() {
        let base = TerrainConfig::compact();
        assert_eq!(TerrainCache::key_for(&base), TerrainCache::key_for(&base.clone()));
        let mut reseeded = base.clone();
        reseeded.seed = WorldSeed::new(7);
        assert_ne!(TerrainCache::key_for(&base), TerrainCache::key_for(&reseeded));
        let mut resized = base.clone();
        resized.size += 1.0;
        assert_ne!(TerrainCache::key_for(&base), TerrainCache::key_for(&resized));
        let mut reshaped = base.clone();
        reshaped.shape = WorldShape::Archipelago;
        assert_ne!(TerrainCache::key_for(&base), TerrainCache::key_for(&reshaped));
        let mut refined = base.clone();
        refined.subdivisions += 8;
        assert_ne!(TerrainCache::key_for(&base), TerrainCache::key_for(&refined));
    }
