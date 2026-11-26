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
use terrain::{regenerate_terrain, set_terrain_generator, toggle_erosion};
pub use terrain::Terrain;
use water::{
    set_chop_strength, set_swell_strength, set_water_weather, toggle_water_body_foam,
    toggle_water_caustics, toggle_water_detail, toggle_water_dye, toggle_water_fft,
    toggle_water_foam, toggle_water_optics, toggle_water_refraction, toggle_water_ripples,
    toggle_water_swells, toggle_water_tides, toggle_water_wetness,
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
                    sync_spawn_ground_height,
                    (
                        sync_sea_sun,
                        water::drive_sea_level,
                        water::poll_fft_bake,
                        water::sync_water_clipmap,
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
        command!(app, "Set the water dye color", water::set_water_color);
        command!(app, "Set the water refraction", water::set_water_refraction);
        command!(app, "Set the caustic strength", water::set_caustic_strength);
        command!(app, "Toggle terrain erosion", toggle_erosion);
        command!(
            app,
            "Set the terrain generator",
            set_terrain_generator
        );
        command!(app, "Regenerate the terrain", regenerate_terrain);
        command!(app, "Toggle water ripples", toggle_water_ripples);
        command!(app, "Toggle water foam", toggle_water_foam);
        command!(app, "Toggle underwater light optics", toggle_water_optics);
        command!(app, "Toggle body foam", toggle_water_body_foam);
        command!(app, "Toggle surface detail", toggle_water_detail);
        command!(app, "Toggle water dye", toggle_water_dye);
        command!(app, "Toggle water caustics", toggle_water_caustics);
        command!(app, "Toggle water refraction", toggle_water_refraction);
        command!(app, "Toggle the tides", toggle_water_tides);
        command!(app, "Toggle ocean swells", toggle_water_swells);
        command!(app, "Toggle FFT water", toggle_water_fft);
        command!(app, "Set the water weather", set_water_weather);
        command!(app, "Set the FFT swell strength", set_swell_strength);
        command!(app, "Set the FFT chop strength", set_chop_strength);
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
