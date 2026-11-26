use bevy::{pbr::MaterialPlugin, prelude::*};
use bevy_shader_kit::WetSandMaterial;

use crate::{
    utils::SpawnGroundHeight,
    utils::registry::{
        commands::{CommandAppExt, command},
        debug_info::{DebugInfoAppExt, debug_info},
        debug_views::{DebugViewAppExt, debug_view},
    },
};

mod atmosphere;
mod chunks;
mod erosion;
mod grid;
mod heightmap;
pub mod terrain;
mod water;

pub(crate) use atmosphere::SkyBox;
use atmosphere::{
    TerrainFog, setup_atmosphere, sync_sea_sun, sync_terrain_fog, toggle_atmospheric_fog,
    toggle_fog_light_influence,
};
use chunks::{
    attach_finished_chunk_meshes, chunk_debug_info, spawn_terrain_chunks, sync_chunk_colliders,
    update_chunk_lods,
};
pub use chunks::{TerrainChunk, TerrainSurfaceMaterial, TerrainViewer};
use heightmap::sync_height_map;
use terrain::{regenerate_terrain, set_terrain_generator, toggle_erosion};
pub use terrain::Terrain;
use water::{
    toggle_water_body_foam, toggle_water_coast, toggle_water_foam, toggle_water_optics,
    toggle_water_ripples, toggle_water_swells, toggle_water_tides, toggle_water_wetness,
};
pub use water::{Sea, SeaState, WaterSurface, terrain_gradient};

fn sync_spawn_ground_height(
    terrain: Res<Terrain>,
    mut spawn: ResMut<SpawnGroundHeight>,
    mut seen: Local<u64>,
) {
    if *seen == terrain.revision() {
        return;
    }
    *seen = terrain.revision();
    spawn.0 = terrain.sample_ground_height(0.0, 0.0);
}

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainFog>()
            .init_resource::<Terrain>()
            .init_resource::<TerrainViewer>()
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
                    update_chunk_lods,
                    sync_chunk_colliders,
                    attach_finished_chunk_meshes,
                    sync_height_map,
                    sync_spawn_ground_height,
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
        command!(app, "Toggle terrain erosion", toggle_erosion);
        command!(
            app,
            "Set the terrain generator",
            set_terrain_generator
        );
        command!(app, "Regenerate the terrain", regenerate_terrain);
        command!(
            app,
            "Toggle coastal shoaling on the water surface",
            toggle_water_coast
        );
        command!(app, "Toggle water ripples", toggle_water_ripples);
        command!(app, "Toggle water foam", toggle_water_foam);
        command!(app, "Toggle underwater light optics", toggle_water_optics);
        command!(app, "Toggle body foam", toggle_water_body_foam);
        command!(app, "Toggle the tides", toggle_water_tides);
        command!(app, "Toggle ocean swells", toggle_water_swells);
        command!(app, "Toggle wet sand", toggle_water_wetness);
        debug_info!(app, "Chunks", chunk_debug_info);
        debug_view!(app, "chunks", "Chunk grid painted on the terrain surface");
        debug_view!(app, "lod", "Chunk LOD regions painted on the terrain surface");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::terrain::Generator;

    #[test]
    fn spawn_height_follows_regeneration() {
        let terrain = Terrain::default();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(terrain.clone())
            .insert_resource(SpawnGroundHeight(terrain.sample_ground_height(0.0, 0.0)))
            .add_systems(Update, sync_spawn_ground_height);
        app.update();

        {
            let mut terrain = app.world_mut().resource_mut::<Terrain>();
            terrain.set_generator(Generator::Noise {
                seed: 6,
                octaves: 4,
                scale: 0.01,
            });
            terrain.regenerate();
        }
        app.update();

        let expected = app
            .world()
            .resource::<Terrain>()
            .sample_ground_height(0.0, 0.0);
        assert_eq!(app.world().resource::<SpawnGroundHeight>().0, expected);
    }
}
