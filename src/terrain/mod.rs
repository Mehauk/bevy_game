use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;

use crate::utils::registry::commands::{CommandAppExt, command};
use crate::utils::scatter::{ScatterClass, fbm, poisson_scatter};
use crate::utils::{SpawnGroundHeight, TeleportRequest};

pub mod atmosphere;
mod bench;
pub mod chunk;
mod commands;
mod colour;
mod field;
pub mod gpu;
pub mod hydrology;
mod job;
mod macro_landform;
pub mod material;
mod map;
pub mod noise;
pub mod params;
mod preview;
pub mod region;
pub mod season;
pub mod seed;
mod setup;
pub mod sky;
pub mod splat;
pub mod stream;
mod surfaces;
pub mod topology;
pub mod voronoi;
pub mod water;

use atmosphere::{UnderwaterState, sync_underwater_view};
use chunk::{ChunkJobQueue, drain_chunk_builds, rebuild_terrain};
use commands::{
    benchmark_terrain, cell_info, debug_material, debug_view, export_preview, find_feature,
    inspect_hydrology, inspect_point, regenerate_terrain, set_cell_height, set_season,
    set_world_seed, set_world_shape, set_world_topology, toggle_season_cycle, world_summary,
};
use gpu::{SkyRenderMaterial, TerrainRenderMaterial, TileArrays, WaterRenderMaterial};
use job::{TerrainJob, queue_regeneration, run_terrain_job};
use material::DebugMaterialMode;
pub use map::{TerrainConfig, TerrainMap};
use season::SeasonClock;
use setup::{build_terrain_tile_arrays, load_surface_tiles, validate_terrain_textures};
pub use sky::SkyBox;
use sky::{
    TerrainAtmosphere, TerrainFog, setup_sky, sync_season_uniforms, sync_terrain_atmosphere,
    sync_terrain_fog, toggle_atmospheric_fog, toggle_fog_light_influence,
};
use splat::SurfaceTiles;
use stream::{ChunkFocus, StreamQueue, drain_stream_builds, stream_infinite_chunks};
use surfaces::{
    WaterJobQueue, animate_water_materials, drain_water_builds, rebuild_water_surfaces,
    sync_sea_tide,
};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        let mut placeholder = TerrainMap::new(TerrainConfig::compact());
        *placeholder.config_mut() = TerrainConfig::default();
        let origin_height = pick_spawn_height(&placeholder);

        app.insert_resource(placeholder)
            .init_resource::<TerrainFog>()
            .init_resource::<SeasonClock>()
            .init_resource::<TerrainAtmosphere>()
            .init_resource::<TerrainJob>()
            .init_resource::<TerrainDebugView>()
            .init_resource::<SurfaceTiles>()
            .init_resource::<TileArrays>()
            .init_resource::<UnderwaterState>()
            .init_resource::<ChunkJobQueue>()
            .init_resource::<WaterJobQueue>()
            .init_resource::<ChunkFocus>()
            .init_resource::<StreamQueue>()
            .init_resource::<TeleportRequest>()
            .insert_resource(DebugMaterialMode::Textured)
            .insert_resource(GlobalAmbientLight::default())
            .insert_resource(ClearColor(Color::srgb(0.4, 0.55, 0.75)));

        app.add_plugins((
            MaterialPlugin::<TerrainRenderMaterial>::default(),
            MaterialPlugin::<WaterRenderMaterial>::default(),
            MaterialPlugin::<SkyRenderMaterial>::default(),
        ));

        queue_regeneration(&mut app.world_mut().resource_mut::<TerrainJob>());

        app.insert_resource(SpawnGroundHeight(origin_height))
            .add_systems(
                Startup,
                (
                    setup_sky,
                    load_surface_tiles,
                    build_terrain_tile_arrays,
                ).chain(),
            )
            .add_systems(
                Update,
                (
                    tick_season_clock,
                    apply_season_to_map,
                    run_terrain_job,
                    rebuild_terrain,
                    drain_chunk_builds,
                    stream_infinite_chunks,
                    drain_stream_builds,
                    rebuild_water_surfaces,
                    drain_water_builds,
                    animate_water_materials,
                    sync_sea_tide,
                    sync_terrain_atmosphere,
                    sync_season_uniforms,
                    sync_underwater_view,
                    sync_terrain_fog,
                    validate_terrain_textures,
                )
                    .chain(),
            );

        command!(app, "Toggle atmospheric fog", toggle_atmospheric_fog);
        command!(
            app,
            "Toggle directional light fog influence",
            toggle_fog_light_influence
        );
        command!(
            app,
            "Set a Voronoi cell's base height (can be negative): <cell_id> <height>",
            set_cell_height
        );
        command!(
            app,
            "Inspect Voronoi terrain cells (optional: <x> <z>)",
            cell_info
        );
        command!(
            app,
            "Set the macro world shape (continent|archipelago|island_chain|inland_sea)",
            set_world_shape
        );
        command!(
            app,
            "Set the world topology (finite | torus | infinite)",
            set_world_topology
        );
        command!(app, "Set the world seed and regenerate: <u64>", set_world_seed);
        command!(
            app,
            "Regenerate the terrain from the current settings",
            regenerate_terrain
        );
        command!(app, "Inspect terrain at a point: <x> <z>", inspect_point);
        command!(
            app,
            "Inspect hydrology at a point: <x> <z>",
            inspect_hydrology
        );
        command!(
            app,
            "Set the season (spring | summer | fall | winter)",
            set_season
        );
        command!(app, "Toggle the seasonal cycle clock", toggle_season_cycle);
        command!(
            app,
            "Set the debug visualisation (none|cells|region|slope|precipitation|flow|flood)",
            debug_view
        );
        command!(
            app,
            "Set the terrain material mode (textured|flat_color|uv_grid)",
            debug_material
        );
        command!(app, "Write terrain preview maps to disk: [layer]", export_preview);
        command!(
            app,
            "Time every terrain generation stage against its budget",
            benchmark_terrain
        );
        command!(
            app,
            "Print a copy-friendly summary of the current world",
            world_summary
        );
        command!(
            app,
            "Locate a feature and teleport there: <lake|waterfall|river> [index] | region <label>",
            find_feature
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugView {
    None,
    CellBorders,
    Region,
    Slope,
    Precipitation,
    Flow,
    FloodZone,
}

impl DebugView {
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "none" => Some(DebugView::None),
            "cells" | "voronoi" => Some(DebugView::CellBorders),
            "region" | "biome" => Some(DebugView::Region),
            "slope" => Some(DebugView::Slope),
            "precipitation" | "rain" => Some(DebugView::Precipitation),
            "flow" | "rivers" => Some(DebugView::Flow),
            "flood" => Some(DebugView::FloodZone),
            _ => None,
        }
    }
}

#[derive(Resource)]
pub struct TerrainDebugView(pub DebugView);

impl Default for TerrainDebugView {
    fn default() -> Self {
        Self(DebugView::None)
    }
}

fn tick_season_clock(mut clock: ResMut<SeasonClock>, time: Res<Time>) {
    clock.tick(time.delta_secs());
}

fn apply_season_to_map(mut map: ResMut<TerrainMap>, clock: Res<SeasonClock>) {
    if clock.is_changed() && map.sync_season_state(&clock) {
        info!("Season advanced to {}", clock.season.label());
    }
}

pub fn pick_spawn_height(map: &TerrainMap) -> f32 {
    let origin = map.height_at(0.0, 0.0);
    let sea = map.sea_level();
    let limit = 50.0f32.to_radians().tan();
    let half = map.size() * 0.5;
    let step = (map.size() / 24.0).max(1.0);
    let mut best: Option<(f32, f32)> = None;
    let mut z = -half;
    while z <= half {
        let mut x = -half;
        while x <= half {
            let height = map.height_at(x, z);
            if height >= sea + 0.5 && map.slope_at(x, z) <= limit {
                let score = -(x * x + z * z);
                if best.is_none_or(|(previous, _)| score > previous) {
                    best = Some((score, height));
                }
            }
            x += step;
        }
        z += step;
    }
    best.map(|(_, height)| height).unwrap_or(origin)
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropScatter {
    pub trees: Vec<Vec2>,
    pub rocks: Vec<Vec2>,
    pub grass: Vec<Vec2>,
}

#[allow(dead_code)]
pub fn scatter_props(map: &TerrainMap, centre: Vec2, extent: f32) -> PropScatter {
    let seed = map.seed().0.rotate_left(17)
        ^ map.generation()
        ^ (centre.x.to_bits() as u64)
        ^ ((centre.y.to_bits() as u64) << 32);
    let sea = map.sea_level();
    let tree_slope = 35.0f32.to_radians().tan();
    let classes = [
        ScatterClass {
            min_spacing: 4.0,
            max_count: 48,
        },
        ScatterClass {
            min_spacing: 2.0,
            max_count: 32,
        },
        ScatterClass {
            min_spacing: 0.9,
            max_count: 240,
        },
    ];
    let density = |class: usize, point: Vec2| -> f32 {
        if map.is_underwater(point.x, point.y) {
            return 0.0;
        }
        let height = map.height_at(point.x, point.y);
        let slope = map.slope_at(point.x, point.y);
        let moisture = map.precipitation_at(point.x, point.y);
        match class {
            0 => {
                if height < sea + 0.5 || slope > tree_slope || moisture < 0.2 {
                    0.0
                } else {
                    fbm(seed, point.x, point.y, 18.0, 3)
                }
            }
            1 => 0.3 + 0.7 * fbm(seed ^ 0x9E, point.x, point.y, 11.0, 2),
            _ => {
                if height < sea + 0.3 || slope > 1.5 {
                    0.0
                } else {
                    fbm(seed ^ 0x5B, point.x, point.y, 9.0, 3) * moisture.min(1.0)
                }
            }
        }
    };
    let points = poisson_scatter(
        seed,
        centre - Vec2::splat(extent * 0.5),
        Vec2::splat(extent),
        &classes,
        &density,
    );
    let mut scatter = PropScatter::default();
    for (class, point) in points {
        match class {
            0 => scatter.trees.push(point),
            1 => scatter.rocks.push(point),
            _ => scatter.grass.push(point),
        }
    }
    scatter
}

#[cfg(test)]
mod tests {
    use super::*;
    use season::Season;

    #[test]
    fn debug_views_parse_and_cover_the_checklist() {
        for label in [
            "none",
            "cells",
            "region",
            "slope",
            "precipitation",
            "flow",
            "flood",
        ] {
            assert!(DebugView::parse(label).is_some(), "{label} must parse");
        }
        assert_eq!(DebugView::parse("bogus"), None);
    }

    #[test]
    fn material_modes_parse_and_cover_the_checklist() {
        for label in ["textured", "flat_color", "uv_grid"] {
            assert!(DebugMaterialMode::parse(label).is_some(), "{label} must parse");
        }
        assert_eq!(DebugMaterialMode::parse("bogus"), None);
    }

    #[test]
    fn the_spawn_origin_stays_on_solid_ground_through_the_seasons() {
        let mut map = TerrainMap::compact();
        let origin = map.height_at(0.0, 0.0);
        assert!(origin.is_finite());
        assert!(origin >= map.heights().min() - 1e-3);
        assert!(origin <= map.heights().max() + 1e-3);
        for season in Season::ALL {
            map.set_season(season);
            let height = map.height_at(0.0, 0.0);
            assert!(
                (height - origin).abs() < 1e-3,
                "{} must not move the spawn floor",
                season.label()
            );
        }
        assert_eq!(SpawnGroundHeight(origin).0, origin);
    }

    #[test]
    fn most_of_the_world_is_walkable_at_the_character_slope_limit() {
        let map = TerrainMap::compact();
        let limit = 50.0f32.to_radians().tan();
        let field = map.heights();
        let walkable = (0..field.len())
            .filter(|&index| {
                let (x, z) = field.coords(index);
                let point = field.world_of(x, z);
                map.slope_at(point.x, point.y) <= limit
            })
            .count();
        let fraction = walkable as f32 / field.len() as f32;
        assert!(
            fraction > 0.6,
            "only {fraction:.2} of the world is walkable at the slope limit"
        );
    }

    #[test]
    fn the_picked_spawn_height_is_dry_and_walkable() {
        let map = TerrainMap::compact();
        let height = pick_spawn_height(&map);
        assert!(height.is_finite());
        assert!(
            height >= map.sea_level(),
            "picked spawn {height} must sit at or above sea level {}",
            map.sea_level()
        );
    }

    #[test]
    fn scatter_is_deterministic_and_stays_on_dry_ground() {
        let map = TerrainMap::compact();
        let centre = Vec2::ZERO;
        let first = scatter_props(&map, centre, 40.0);
        let second = scatter_props(&map, centre, 40.0);
        assert_eq!(first, second, "the same patch must scatter identically");
        let all: Vec<&Vec2> = first
            .trees
            .iter()
            .chain(first.rocks.iter())
            .chain(first.grass.iter())
            .collect();
        assert!(
            !all.is_empty(),
            "the dry spawn neighbourhood must scatter something"
        );
        for point in all {
            assert!(
                !map.is_underwater(point.x, point.y),
                "prop at {point} must not scatter into water"
            );
            assert!((point.x - centre.x).abs() <= 20.0 + 1e-3);
            assert!((point.y - centre.y).abs() <= 20.0 + 1e-3);
        }
    }

    #[test]
    fn scatter_separates_prop_classes_by_their_radii() {
        let map = TerrainMap::compact();
        let scatter = scatter_props(&map, Vec2::ZERO, 40.0);
        let mut all: Vec<(f32, Vec2)> = Vec::new();
        all.extend(scatter.trees.iter().map(|p| (4.0, *p)));
        all.extend(scatter.rocks.iter().map(|p| (2.0, *p)));
        all.extend(scatter.grass.iter().map(|p| (0.9, *p)));
        for i in 0..all.len() {
            for j in i + 1..all.len() {
                let (radius_a, a) = all[i];
                let (radius_b, b) = all[j];
                assert!(
                    a.distance(b) >= radius_a.max(radius_b) - 1e-3,
                    "props at {a} and {b} overlap"
                );
            }
        }
    }

    #[test]
    fn scatter_uses_the_perlin_clump_gate_not_a_uniform_field() {
        let map = TerrainMap::compact();
        let near = scatter_props(&map, Vec2::ZERO, 40.0);
        let far = scatter_props(&map, Vec2::new(37.0, -23.0), 40.0);
        let total = near.trees.len() + near.rocks.len() + near.grass.len()
            + far.trees.len()
            + far.rocks.len()
            + far.grass.len();
        assert!(total > 0, "dry patches must scatter something");
        assert_ne!(near, far, "clumped scatter must differ between neighbouring patches");
    }
}
