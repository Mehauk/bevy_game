use std::f32::consts::TAU;

use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    pbr::StandardMaterial,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use bevy_shader_kit::{
    ShoreContext, SdfVolume, SwellSet, SwellWave, WaterMaterial, WaterMaterialExt, WaterOptics,
    WetSandMaterial, shore_factor,
};

use super::chunks::{TerrainSurfaceMaterial, TerrainViewer};
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

    pub fn shore_surface_height_at(&self, terrain: &Terrain, x: f32, z: f32, time: f32) -> f32 {
        let shore = shore_context(terrain, self, x, z);
        let swell = self
            .effective_swells()
            .height_at_shored(x, z, time, shore);
        let trough_floor = -(shore.rest_depth - TROUGH_CLEARANCE).max(0.0);
        self.sea_level + swell.max(trough_floor)
    }

    pub fn flooded_surface_height_at(
        &self,
        terrain: &Terrain,
        x: f32,
        z: f32,
        time: f32,
    ) -> Option<f32> {
        if self.sea_level + self.max_crest_height() < terrain.grid().sample_spill(x, z) {
            None
        } else {
            Some(self.shore_surface_height_at(terrain, x, z, time))
        }
    }

    fn max_crest_height(&self) -> f32 {
        let span = self
            .effective_swells()
            .amplitude_span()
            .max(COAST_SPAN_FLOOR);
        span * COAST_SHOAL_MAX * (0.5 + self.wave_energy)
    }
}

const CLIPMAP_BASE_STEP: f32 = 0.5;
const CLIPMAP_RING_CELLS: u32 = 64;
const CLIPMAP_RINGS: u32 = 9;
const CLIPMAP_SNAP: f32 = 2.0;
const CLIPMAP_HOLE: u32 = CLIPMAP_RING_CELLS / 4;
const EMPTY_SDF: f32 = 1.0e6;
const SWASH_RUNUP: f32 = 0.15;
const TROUGH_CLEARANCE: f32 = 0.03;
const WETNESS_DECAY: f32 = 0.01;
const COAST_SPAN_FLOOR: f32 = 0.05;
const COAST_SHOAL_MAX: f32 = 2.5;

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

fn ring_step(ring: u32) -> f32 {
    CLIPMAP_BASE_STEP * (1u32 << ring) as f32
}

fn clipmap_half_extent() -> f32 {
    ring_step(CLIPMAP_RINGS - 1) * CLIPMAP_RING_CELLS as f32 * 0.5
}

fn snap_clipmap_center(translation: Vec2) -> Vec2 {
    (translation / CLIPMAP_SNAP).round() * CLIPMAP_SNAP
}

fn build_water_clipmap_mesh() -> Mesh {
    let side = CLIPMAP_RING_CELLS + 1;
    let half_cells = CLIPMAP_RING_CELLS / 2;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for ring in 0..CLIPMAP_RINGS {
        let step = ring_step(ring);
        let base = positions.len() as u32;
        let vertex = |i: u32, j: u32| -> u32 { base + j * side + i };
        let quad = |indices: &mut Vec<u32>, i: u32, j: u32| {
            indices.extend_from_slice(&[
                vertex(i, j),
                vertex(i, j + 1),
                vertex(i + 1, j + 1),
                vertex(i, j),
                vertex(i + 1, j + 1),
                vertex(i + 1, j),
            ]);
        };
        for j in 0..side {
            for i in 0..side {
                positions.push([
                    (i as f32 - half_cells as f32) * step,
                    0.0,
                    (j as f32 - half_cells as f32) * step,
                ]);
                normals.push([0.0, 1.0, 0.0]);
                uvs.push([
                    i as f32 / CLIPMAP_RING_CELLS as f32,
                    j as f32 / CLIPMAP_RING_CELLS as f32,
                ]);
            }
        }
        let stitched = ring + 1 < CLIPMAP_RINGS;
        let last = CLIPMAP_RING_CELLS - 1;
        let in_hole = |i: u32, j: u32| {
            ring > 0
                && i >= CLIPMAP_HOLE
                && i < CLIPMAP_RING_CELLS - CLIPMAP_HOLE
                && j >= CLIPMAP_HOLE
                && j < CLIPMAP_RING_CELLS - CLIPMAP_HOLE
        };
        for j in 0..CLIPMAP_RING_CELLS {
            for i in 0..CLIPMAP_RING_CELLS {
                if in_hole(i, j) {
                    continue;
                }
                let on_border = i == 0 || i == last || j == 0 || j == last;
                if !stitched || !on_border {
                    quad(&mut indices, i, j);
                }
            }
        }
        if !stitched {
            continue;
        }
        for t in (0..CLIPMAP_RING_CELLS).step_by(2) {
            let near = t == 0;
            let far = t == last - 1;
            let (a, b, c) = (vertex(0, t), vertex(0, t + 2), vertex(1, t + 1));
            indices.extend_from_slice(&[a, b, c]);
            if !near {
                indices.extend_from_slice(&[a, c, vertex(1, t)]);
            }
            if !far {
                indices.extend_from_slice(&[b, vertex(1, t + 2), c]);
            }
            let (a, b, c) = (vertex(last + 1, t), vertex(last + 1, t + 2), vertex(last, t + 1));
            indices.extend_from_slice(&[a, c, b]);
            if !near {
                indices.extend_from_slice(&[a, vertex(last, t), c]);
            }
            if !far {
                indices.extend_from_slice(&[b, c, vertex(last, t + 2)]);
            }
            let (a, b, c) = (vertex(t, 0), vertex(t + 2, 0), vertex(t + 1, 1));
            indices.extend_from_slice(&[a, c, b]);
            if !near {
                indices.extend_from_slice(&[a, vertex(t, 1), c]);
            }
            if !far {
                indices.extend_from_slice(&[b, c, vertex(t + 2, 1)]);
            }
            let (a, b, c) = (vertex(t, last + 1), vertex(t + 2, last + 1), vertex(t + 1, last));
            indices.extend_from_slice(&[a, b, c]);
            if !near {
                indices.extend_from_slice(&[a, c, vertex(t, last)]);
            }
            if !far {
                indices.extend_from_slice(&[b, vertex(t + 2, last), c]);
            }
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
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
    commands.spawn((
        Name::new("Sea"),
        Sea,
        Transform::from_xyz(0.0, sea_state.sea_level, 0.0),
        Mesh3d(meshes.add(build_water_clipmap_mesh())),
        MeshMaterial3d(material),
        NotShadowCaster,
    ));
}

pub(super) fn drive_sea_level(time: Res<Time>, mut sea_state: ResMut<SeaState>) {
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
}

pub(super) fn sync_water_clipmap(
    viewer: Res<TerrainViewer>,
    sea_state: Res<SeaState>,
    mut sea: Query<&mut Transform, With<Sea>>,
) {
    let snapped = snap_clipmap_center(Vec2::new(viewer.position.x, viewer.position.z));
    for mut transform in &mut sea {
        transform.translation = Vec3::new(snapped.x, sea_state.sea_level, snapped.y);
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
    use crate::terrain::terrain::{Generator, TerrainConfig};
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
            .init_resource::<TerrainViewer>()
            .add_systems(Update, (drive_sea_level, sync_water_clipmap).chain());
        let sea = app.world_mut().spawn((Sea, Transform::default())).id();
        for _ in 0..8 {
            app.update();
        }
        let state = app.world().resource::<SeaState>();
        assert_eq!(state.sea_level, 0.5);
        assert_eq!(state.level_rate, 0.0);
        let transform = app.world().get::<Transform>(sea).unwrap();
        assert_eq!(transform.translation, Vec3::new(0.0, 0.5, 0.0));
    }

    #[test]
    fn the_clipmap_follows_the_viewer() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(SeaState {
                sea_level: -0.25,
                ..default()
            })
            .insert_resource(TerrainViewer {
                position: Vec3::new(13.0, 4.0, -7.0),
            })
            .add_systems(Update, sync_water_clipmap);
        let sea = app.world_mut().spawn((Sea, Transform::default())).id();
        app.update();
        let transform = app.world().get::<Transform>(sea).unwrap();
        assert_eq!(transform.translation, Vec3::new(14.0, -0.25, -8.0));
    }

    #[test]
    fn snap_clipmap_center_locks_to_the_grid() {
        for step in 0..64 {
            let raw = Vec2::new(step as f32 * 0.37 - 11.8, step as f32 * -0.53 + 7.4);
            let snapped = snap_clipmap_center(raw);
            assert_eq!(snapped.x % CLIPMAP_SNAP, 0.0);
            assert_eq!(snapped.y % CLIPMAP_SNAP, 0.0);
            assert!((snapped.x - raw.x).abs() <= CLIPMAP_SNAP * 0.5 + 1.0e-4);
            assert!((snapped.y - raw.y).abs() <= CLIPMAP_SNAP * 0.5 + 1.0e-4);
        }
    }

    #[test]
    fn the_clipmap_tiles_the_world_without_gaps() {
        let mesh = build_water_clipmap_mesh();
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let indices: Vec<usize> = mesh.indices().unwrap().iter().collect();
        let half = clipmap_half_extent();
        let mut area = 0.0f64;
        for tri in indices.chunks(3) {
            let a = Vec3::from(positions[tri[0] as usize]);
            let b = Vec3::from(positions[tri[1] as usize]);
            let c = Vec3::from(positions[tri[2] as usize]);
            let cross = (b - a).cross(c - a);
            assert!(cross.y > 0.0, "every triangle must face up");
            area += cross.length() as f64 * 0.5;
        }
        let covered = (2.0 * half) as f64 * (2.0 * half) as f64;
        assert!(
            (area - covered).abs() < covered * 1.0e-4,
            "triangles cover {area}, expected {covered}"
        );
    }

    #[test]
    fn clipmap_seams_share_exact_vertices() {
        let mesh = build_water_clipmap_mesh();
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let used: std::collections::BTreeSet<usize> = mesh.indices().unwrap().iter().collect();
        let side = CLIPMAP_RING_CELLS + 1;
        let boundary = |ring: u32, half: f32| -> std::collections::BTreeSet<(u32, u32)> {
            let base = (ring * side * side) as usize;
            (0..(side * side) as usize)
                .filter(|k| used.contains(&(base + k)))
                .map(|k| positions[base + k])
                .filter(|p| {
                    (p[0].abs() == half && p[2].abs() <= half)
                        || (p[2].abs() == half && p[0].abs() <= half)
                })
                .map(|p| (p[0].to_bits(), p[2].to_bits()))
                .collect()
        };
        for ring in 1..CLIPMAP_RINGS {
            let fine = boundary(
                ring - 1,
                ring_step(ring - 1) * CLIPMAP_RING_CELLS as f32 * 0.5,
            );
            let coarse = boundary(
                ring,
                ring_step(ring) * (CLIPMAP_RING_CELLS as f32 * 0.5 - CLIPMAP_HOLE as f32),
            );
            assert!(!fine.is_empty() && fine == coarse, "seam {ring} is not watertight");
        }
    }

    #[test]
    fn swells_off_flatten_the_surface() {
        let sea = SeaState {
            sea_level: 1.0,
            swells_enabled: false,
            ..default()
        };
        for step in 0..12 {
            let time = step as f32 * 0.7;
            assert_eq!(sea.sea_level + sea.effective_swells().height_at(4.0, -9.0, time), 1.0);
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
        assert_eq!(sea.sea_level + sea.effective_swells().height_at(3.0, -8.0, 4.0), 1.5);
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
            let height = sea.sea_level + sea.effective_swells().height_at(12.0, -30.0, time);
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

    #[test]
    fn ground_the_sea_never_reaches_stays_dry() {
        let terrain = Terrain::default();
        let sea = SeaState {
            sea_level: -1.0,
            ..default()
        };
        assert_eq!(sea.flooded_surface_height_at(&terrain, 0.0, 0.0, 1.0), None);
    }

    #[test]
    fn the_sea_floods_the_plain_once_it_spills_in() {
        let terrain = Terrain::default();
        let sea = SeaState {
            sea_level: 0.0,
            ..default()
        };
        assert!(sea.flooded_surface_height_at(&terrain, 0.0, 0.0, 1.0).is_some());
    }

    #[test]
    fn an_enclosed_basin_below_sea_level_stays_dry() {
        let terrain = Terrain::new(TerrainConfig {
            generator: Generator::Noise {
                seed: 3,
                octaves: 4,
                scale: 0.008,
            },
            ..default()
        });
        let grid = terrain.grid();
        let mut basin = (0u32, 0u32, 0.0f32);
        for iz in (0..grid.resolution().y).step_by(4) {
            for ix in (0..grid.resolution().x).step_by(4) {
                let x = grid.bounds_min().x + (ix as f32 + 0.5) * grid.texel().x;
                let z = grid.bounds_min().y + (iz as f32 + 0.5) * grid.texel().y;
                let gap = grid.sample_spill(x, z) - grid.sample(x, z);
                if gap > basin.2 {
                    basin = (ix, iz, gap);
                }
            }
        }
        let (ix, iz, gap) = basin;
        assert!(gap > 0.5, "the noise world must contain an enclosed basin");
        let x = grid.bounds_min().x + (ix as f32 + 0.5) * grid.texel().x;
        let z = grid.bounds_min().y + (iz as f32 + 0.5) * grid.texel().y;
        let sea = SeaState {
            sea_level: (grid.sample(x, z) + grid.sample_spill(x, z)) * 0.5,
            swells_enabled: false,
            ..default()
        };
        assert!(
            terrain.sample_ground_height(x, z) < sea.sea_level,
            "the basin floor must sit below the sea"
        );
        assert_eq!(sea.flooded_surface_height_at(&terrain, x, z, 0.0), None);
    }
}
