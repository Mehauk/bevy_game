use std::f32::consts::TAU;

use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    pbr::StandardMaterial,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use bevy_shader_kit::{
    ShoreContext, SdfVolume, SwellSet, SwellWave, WaterMaterial, WaterMaterialExt, WaterOptics,
    WetSandMaterial, shore_factor,
};

use super::chunks::TerrainSurfaceMaterial;
use super::heightmap::bake_height_map;
use super::terrain::Terrain;
use crate::utils::registry::commands::{CommandArgs, CommandOutput};

#[derive(Component)]
pub struct Sea;

#[derive(Resource)]
pub struct WaterSurface {
    pub material: Handle<WaterMaterial>,
}

#[derive(Resource)]
pub struct SeaState {
    pub base_level: f32,
    pub tide_amplitude: f32,
    pub tide_period: f32,
    pub wave_energy: f32,
    pub sea_level: f32,
    pub level_rate: f32,
    pub wetness_height: f32,
    pub swells: SwellSet,
    pub optics: WaterOptics,
    pub tides_enabled: bool,
    pub swells_enabled: bool,
    pub wetness_enabled: bool,
}

impl Default for SeaState {
    fn default() -> Self {
        Self {
            base_level: -0.1,
            tide_amplitude: 0.6,
            tide_period: 240.0,
            wave_energy: 0.35,
            sea_level: -0.1,
            level_rate: 0.0,
            wetness_height: f32::MIN,
            swells: SWELLS,
            optics: WaterOptics::default(),
            tides_enabled: true,
            swells_enabled: true,
            wetness_enabled: true,
        }
    }
}

impl SeaState {
    pub fn effective_swells(&self) -> SwellSet {
        if self.swells_enabled {
            self.swells
        } else {
            SwellSet::default()
        }
    }

    pub fn surface_height_at(&self, x: f32, z: f32, time: f32) -> f32 {
        self.sea_level + self.effective_swells().height_at(x, z, time)
    }

    pub fn shore_surface_height_at(&self, terrain: &Terrain, x: f32, z: f32, time: f32) -> f32 {
        let shore = shore_context(terrain, self, x, z);
        let swell = self
            .effective_swells()
            .height_at_shored(x, z, time, shore);
        let trough_floor = -(shore.rest_depth - TROUGH_CLEARANCE).max(0.0);
        self.sea_level + swell.max(trough_floor)
    }
}

const SEA_SUBDIVISIONS: u32 = 256;
const EMPTY_SDF: f32 = 1.0e6;
const SWASH_RUNUP: f32 = 0.15;
const TROUGH_CLEARANCE: f32 = 0.03;
const WETNESS_DECAY: f32 = 0.01;

const SWELLS: SwellSet = SwellSet {
    count: 3,
    pad0: 0,
    pad1: 0,
    pad2: 0,
    waves: [
        SwellWave {
            direction: Vec2::new(0.943, 0.330),
            amplitude: 0.11,
            wavelength: 17.0,
            speed: 2.6,
            steepness: 0.22,
            pad0: 0.0,
            pad1: 0.0,
        },
        SwellWave {
            direction: Vec2::new(-0.371, 0.928),
            amplitude: 0.07,
            wavelength: 11.0,
            speed: 2.0,
            steepness: 0.18,
            pad0: 0.0,
            pad1: 0.0,
        },
        SwellWave {
            direction: Vec2::new(0.658, -0.752),
            amplitude: 0.045,
            wavelength: 6.5,
            speed: 1.5,
            steepness: 0.12,
            pad0: 0.0,
            pad1: 0.0,
        },
        SwellWave {
            direction: Vec2::ZERO,
            amplitude: 0.0,
            wavelength: 1.0,
            speed: 0.0,
            steepness: 0.0,
            pad0: 0.0,
            pad1: 0.0,
        },
    ],
};

pub fn terrain_gradient(terrain: &Terrain, x: f32, z: f32) -> Vec2 {
    let step = terrain.grid().texel().max_element().max(1.0);
    Vec2::new(
        (terrain.sample_ground_height(x + step, z) - terrain.sample_ground_height(x - step, z))
            / (2.0 * step),
        (terrain.sample_ground_height(x, z + step) - terrain.sample_ground_height(x, z - step))
            / (2.0 * step),
    )
}

fn shore_context(terrain: &Terrain, sea: &SeaState, x: f32, z: f32) -> ShoreContext {
    let rest_depth = sea.sea_level - terrain.sample_ground_height(x, z);
    let gradient = terrain_gradient(terrain, x, z);
    let length = gradient.length();
    ShoreContext {
        rest_depth,
        shore_dir: if length > 1.0e-4 {
            gradient / length
        } else {
            Vec2::ZERO
        },
        shore_factor: shore_factor(rest_depth),
        energy: sea.wave_energy,
    }
}

fn empty_sdf_image(images: &mut Assets<Image>) -> Handle<Image> {
    let bytes: Vec<u8> = [EMPTY_SDF; 8]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    images.add(Image::new(
        Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 2,
        },
        TextureDimension::D3,
        bytes,
        TextureFormat::R32Float,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ))
}

pub fn setup_water_surface(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    terrain: Res<Terrain>,
    sea_state: Res<SeaState>,
) {
    let coast = bake_height_map(&terrain, &mut images);
    let material = water_materials.add(WaterMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.6, 0.75, 0.72),
            alpha_mode: AlphaMode::Opaque,
            cull_mode: None,
            specular_transmission: 1.0,
            ior: 1.333,
            perceptual_roughness: 0.04,
            reflectance: 0.35,
            thickness: 0.1,
            attenuation_distance: 2.5,
            attenuation_color: Color::srgb(0.2, 0.7, 0.5),
            ..default()
        },
        extension: WaterMaterialExt {
            swells: sea_state.effective_swells(),
            body_volume: SdfVolume::from_bounds(
                Vec3::splat(-0.5),
                Vec3::splat(0.5),
                Vec3::splat(2.0),
            ),
            body_sdf: empty_sdf_image(&mut images),
            coast: coast.field,
            coast_map: coast.image.clone(),
            coastal: true,
            ..default()
        },
    });
    commands.insert_resource(WaterSurface {
        material: material.clone(),
    });
    commands.insert_resource(coast);
    let world = terrain.config().world;
    commands.spawn((
        Name::new("Sea"),
        Sea,
        Transform::from_xyz(0.0, sea_state.sea_level, 0.0),
        Mesh3d(
            meshes.add(
                Plane3d::default()
                    .mesh()
                    .size(world.x, world.y)
                    .subdivisions(SEA_SUBDIVISIONS),
            ),
        ),
        MeshMaterial3d(material),
        NotShadowCaster,
    ));
}

pub(super) fn drive_sea_level(
    time: Res<Time>,
    mut sea_state: ResMut<SeaState>,
    mut sea: Query<&mut Transform, With<Sea>>,
) {
    if sea_state.tides_enabled {
        let phase = TAU * time.elapsed_secs() / sea_state.tide_period;
        sea_state.sea_level = sea_state.base_level + sea_state.tide_amplitude * phase.sin();
        sea_state.level_rate =
            sea_state.tide_amplitude * (TAU / sea_state.tide_period) * phase.cos();
    } else {
        sea_state.sea_level = sea_state.base_level;
        sea_state.level_rate = 0.0;
    }
    let wet_target = sea_state.sea_level + SWASH_RUNUP;
    sea_state.wetness_height =
        wet_target.max(sea_state.wetness_height - WETNESS_DECAY * time.delta_secs());
    for mut transform in &mut sea {
        transform.translation.y = sea_state.sea_level;
    }
}

pub(super) fn sync_water_params(
    time: Res<Time>,
    sea_state: Res<SeaState>,
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
) {
    let Some(surface) = surface else {
        return;
    };
    let Some(mut material) = materials.get_mut(&surface.material) else {
        return;
    };
    let mut params = material.extension.params;
    params.time = time.elapsed_secs();
    params.water_height = sea_state.sea_level;
    params.wave_energy = sea_state.wave_energy;
    params.optics = sea_state.optics;
    material.extension.params = params;
    material.extension.swells = sea_state.effective_swells();
}

pub(super) fn sync_wetness(
    sea_state: Res<SeaState>,
    surface: Option<Res<TerrainSurfaceMaterial>>,
    mut materials: ResMut<Assets<WetSandMaterial>>,
) {
    let Some(surface) = surface else {
        return;
    };
    let Some(mut material) = materials.get_mut(&surface.material) else {
        return;
    };
    material.extension.wetness.height = if sea_state.wetness_enabled {
        sea_state.wetness_height
    } else {
        f32::MIN
    };
}

pub(super) fn set_water_level(
    args: Res<CommandArgs>,
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(height) = args.get_f32(0) else {
        output.set("Usage: set_water_level <height>");
        return;
    };
    sea_state.base_level = height;
    output.set(format!("Water level: {height:.2}"));
}

pub(super) fn set_wave_energy(
    args: Res<CommandArgs>,
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(energy) = args.get_f32(0) else {
        output.set("Usage: set_wave_energy <0..1>");
        return;
    };
    sea_state.wave_energy = energy.clamp(0.0, 1.0);
    output.set(format!("Wave energy: {:.2}", sea_state.wave_energy));
}

fn flip_extension_flag(
    surface: Option<Res<WaterSurface>>,
    materials: &mut Assets<WaterMaterial>,
    output: &mut CommandOutput,
    label: &str,
    flag: impl FnOnce(&mut WaterMaterialExt) -> &mut bool,
) {
    let Some(surface) = surface else {
        return;
    };
    let Some(mut material) = materials.get_mut(&surface.material) else {
        return;
    };
    let value = flag(&mut material.extension);
    *value = !*value;
    output.set(format!("{label}: {}", if *value { "on" } else { "off" }));
}

pub(super) fn toggle_water_coast(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    flip_extension_flag(surface, &mut materials, &mut output, "Water coast", |ext| {
        &mut ext.coastal
    });
}

pub(super) fn toggle_water_ripples(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    flip_extension_flag(surface, &mut materials, &mut output, "Water ripples", |ext| {
        &mut ext.ripples
    });
}

pub(super) fn toggle_water_foam(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    flip_extension_flag(surface, &mut materials, &mut output, "Water foam", |ext| {
        &mut ext.foam
    });
}

pub(super) fn toggle_water_optics(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    flip_extension_flag(surface, &mut materials, &mut output, "Water optics", |ext| {
        &mut ext.optics
    });
}

pub(super) fn toggle_water_body_foam(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    flip_extension_flag(surface, &mut materials, &mut output, "Body foam", |ext| {
        &mut ext.body_foam
    });
}

pub(super) fn toggle_water_tides(mut sea_state: ResMut<SeaState>, mut output: ResMut<CommandOutput>) {
    sea_state.tides_enabled = !sea_state.tides_enabled;
    output.set(if sea_state.tides_enabled {
        "Tides: on"
    } else {
        "Tides: off"
    });
}

pub(super) fn toggle_water_swells(
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    sea_state.swells_enabled = !sea_state.swells_enabled;
    output.set(if sea_state.swells_enabled {
        "Swells: on"
    } else {
        "Swells: off"
    });
}

pub(super) fn toggle_water_wetness(
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    sea_state.wetness_enabled = !sea_state.wetness_enabled;
    output.set(if sea_state.wetness_enabled {
        "Wet sand: on"
    } else {
        "Wet sand: off"
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use bevy_shader_kit::WetSandExt;

    #[test]
    fn tides_off_hold_the_sea_level() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(20),
            ))
            .insert_resource(SeaState {
                base_level: 0.5,
                sea_level: 0.5,
                tides_enabled: false,
                ..default()
            })
            .add_systems(Update, drive_sea_level);
        for _ in 0..8 {
            app.update();
        }
        let sea = app.world().resource::<SeaState>();
        assert_eq!(sea.sea_level, 0.5);
        assert_eq!(sea.level_rate, 0.0);
    }

    #[test]
    fn swells_off_flatten_the_surface() {
        let sea = SeaState {
            sea_level: 1.0,
            swells_enabled: false,
            ..default()
        };
        for step in 0..12 {
            assert_eq!(sea.surface_height_at(4.0, -9.0, step as f32 * 0.7), 1.0);
        }
    }

    #[test]
    fn swells_off_hold_the_shore_flat_too() {
        let terrain = Terrain::default();
        let sea = SeaState {
            sea_level: 0.2,
            wave_energy: 1.0,
            swells_enabled: false,
            ..default()
        };
        for step in 0..12 {
            assert_eq!(
                sea.shore_surface_height_at(&terrain, 0.0, 0.0, step as f32 * 0.35),
                0.2
            );
        }
    }

    #[test]
    fn wetness_off_writes_no_wetness() {
        let mut materials = Assets::<WetSandMaterial>::default();
        let handle = materials.add(WetSandMaterial {
            base: StandardMaterial::default(),
            extension: WetSandExt::default(),
        });
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(SeaState {
                wetness_height: 3.0,
                wetness_enabled: false,
                ..default()
            })
            .insert_resource(TerrainSurfaceMaterial {
                material: handle.clone(),
            })
            .insert_resource(materials)
            .add_systems(Update, sync_wetness);
        app.update();
        let material = app
            .world()
            .resource::<Assets<WetSandMaterial>>()
            .get(&handle)
            .unwrap();
        assert_eq!(material.extension.wetness.height, f32::MIN);
    }

    #[test]
    fn water_toggles_stick_across_sync() {
        let mut materials = Assets::<WaterMaterial>::default();
        let handle = materials.add(WaterMaterial {
            base: StandardMaterial::default(),
            extension: WaterMaterialExt {
                coastal: true,
                ..default()
            },
        });
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(20),
            ))
            .insert_resource(SeaState::default())
            .insert_resource(WaterSurface {
                material: handle.clone(),
            })
            .insert_resource(materials)
            .init_resource::<CommandOutput>()
            .add_systems(Update, (drive_sea_level, sync_water_params));
        let coast = app.world_mut().register_system(toggle_water_coast);
        let swells = app.world_mut().register_system(toggle_water_swells);
        let tides = app.world_mut().register_system(toggle_water_tides);
        app.update();
        app.world_mut().run_system(coast).unwrap();
        app.world_mut().run_system(swells).unwrap();
        app.world_mut().run_system(tides).unwrap();
        for _ in 0..4 {
            app.update();
        }

        let material = app
            .world()
            .resource::<Assets<WaterMaterial>>()
            .get(&handle)
            .unwrap();
        assert!(
            !material.extension.coastal,
            "sync_water_params must not restore the coast flag"
        );
        assert_eq!(material.extension.swells, SwellSet::default());
        let sea = app.world().resource::<SeaState>();
        assert!(!sea.tides_enabled);
        assert_eq!(sea.sea_level, sea.base_level);
    }

    #[test]
    fn a_flat_swell_set_rests_on_the_sea_level() {
        let sea = SeaState {
            sea_level: 1.5,
            swells: SwellSet::default(),
            ..default()
        };
        assert_eq!(sea.surface_height_at(3.0, -8.0, 4.0), 1.5);
    }

    #[test]
    fn the_swell_stays_inside_its_amplitudes() {
        let sea = SeaState {
            sea_level: 2.0,
            ..default()
        };
        let span = sea.swells.amplitude_span();
        for step in 0..12 {
            let time = step as f32 * 0.7;
            let height = sea.surface_height_at(12.0, -30.0, time);
            assert!(
                (height - sea.sea_level).abs() <= span + 1.0e-3,
                "the surface at t={time} left the swell envelope"
            );
        }
    }

    #[test]
    fn dry_ground_feels_no_swell() {
        let terrain = Terrain::default();
        let ground = terrain.sample_ground_height(0.0, 0.0);
        let sea = SeaState {
            sea_level: ground - 1.0,
            ..default()
        };
        for step in 0..24 {
            let time = step as f32 * 0.35;
            assert_eq!(
                sea.shore_surface_height_at(&terrain, 0.0, 0.0, time),
                sea.sea_level,
                "shoaling damps the swell to nothing once the ground breaks the surface (t={time})"
            );
        }
    }

    #[test]
    fn a_trough_never_opens_below_the_seabed() {
        let terrain = Terrain::default();
        let ground = terrain.sample_ground_height(0.0, 0.0);
        let sea = SeaState {
            sea_level: ground + 0.2,
            wave_energy: 1.0,
            ..default()
        };
        for step in 0..24 {
            let time = step as f32 * 0.35;
            assert!(
                sea.shore_surface_height_at(&terrain, 0.0, 0.0, time) >= ground,
                "a trough at t={time} dropped below the seabed"
            );
        }
    }
}
