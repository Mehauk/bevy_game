use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;

use crate::utils::registry::commands::{CommandAppExt, command};
use crate::utils::SpawnGroundHeight;

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
    benchmark_terrain, cell_info, debug_material, debug_view, export_preview, inspect_hydrology,
    inspect_point, regenerate_terrain, set_cell_height, set_season, set_world_seed,
    set_world_shape, set_world_topology, toggle_season_cycle,
};
use gpu::{SkyRenderMaterial, TerrainRenderMaterial, TileArrays, WaterRenderMaterial};
use job::{TerrainJob, run_terrain_job};
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
        app.init_resource::<TerrainMap>()
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
            .insert_resource(DebugMaterialMode::Textured)
            .insert_resource(GlobalAmbientLight::default())
            .insert_resource(ClearColor(Color::srgb(0.4, 0.55, 0.75)));

        app.add_plugins((
            MaterialPlugin::<TerrainRenderMaterial>::default(),
            MaterialPlugin::<WaterRenderMaterial>::default(),
            MaterialPlugin::<SkyRenderMaterial>::default(),
        ));

        let origin_height = app.world().resource::<TerrainMap>().height_at(0.0, 0.0);
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
}
