use bevy::{pbr::MaterialPlugin, prelude::*};
use bevy_shader_kit::WetSandMaterial;

use crate::{
    utils::SpawnGroundHeight,
    utils::registry::{
        commands::{CommandAppExt, command},
        debug_info::{DebugInfoAppExt, debug_info},
    },
};

mod atmosphere;
mod chunks;
mod heightmap;
pub mod terrain;
mod water;

pub(crate) use atmosphere::SkyBox;
use atmosphere::{
    TerrainFog, setup_atmosphere, sync_sea_sun, sync_terrain_fog, toggle_atmospheric_fog,
    toggle_fog_light_influence,
};
use chunks::{attach_finished_chunk_meshes, chunk_debug_info, spawn_terrain_chunks};
pub use terrain::Terrain;
pub use water::{Sea, SeaState, WaterSurface, terrain_gradient};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainFog>()
            .init_resource::<Terrain>()
            .init_resource::<SeaState>()
            .add_plugins(MaterialPlugin::<WetSandMaterial>::default());
        let spawn_ground = app
            .world()
            .resource::<Terrain>()
            .sample_ground_height(0.0, 0.0);
        app.insert_resource(SpawnGroundHeight(spawn_ground));
        app.add_systems(
            Startup,
            (
                spawn_terrain_chunks,
                setup_atmosphere,
                water::setup_water_surface,
            )
                .chain(),
        )
            .add_systems(
                Update,
                (
                    sync_terrain_fog,
                    attach_finished_chunk_meshes,
                    (
                        sync_sea_sun,
                        water::drive_sea_level,
                        water::sync_water_params,
                        water::sync_wetness,
                    )
                        .chain(),
                ),
            );

        command!(app, "Toggle atmospheric fog", toggle_atmospheric_fog);
        command!(
            app,
            "Toggle directional light fog influence",
            toggle_fog_light_influence
        );
        command!(app, "Set the water level", water::set_water_level);
        command!(app, "Set the wave energy", water::set_wave_energy);
        debug_info!(app, "Chunks", chunk_debug_info);
    }
}
