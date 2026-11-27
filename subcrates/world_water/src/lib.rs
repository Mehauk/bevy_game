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
    FftBakeReceiver, FftSwellField, FftWaterTextures, ShoreContext, SdfVolume, SwellSet, SwellWave,
    WaterCaustics, WaterDetail, WaterDye, WaterMaterial, WaterMaterialExt, WaterOptics, WaterParams,
    WetSandMaterial, fft_derivatives_image, fft_displacement_image, fft_placeholder_image,
    shore_factor, shore_vert_damp, spawn_fft_bake,
};

pub trait SeaBed {
    fn ground_height(&self, x: f32, z: f32) -> f32;
    fn spill_height(&self, x: f32, z: f32) -> f32;
    fn texel(&self) -> Vec2;
}

#[derive(Component)]
pub struct Sea;

#[derive(Resource, Default)]
pub struct SeaViewer {
    pub position: Vec3,
}

#[derive(Resource)]
pub struct WaterSurface {
    pub material: Handle<WaterMaterial>,
}

#[derive(Resource)]
pub struct WetSandSurface {
    pub material: Handle<WetSandMaterial>,
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
    pub detail: WaterDetail,
    pub dye: WaterDye,
    pub caustics: WaterCaustics,
    pub refraction: f32,
    pub tides_enabled: bool,
    pub swells_enabled: bool,
    pub wetness_enabled: bool,
    pub fft_enabled: bool,
    pub swell_strength: f32,
    pub chop_strength: f32,
    pub fft_swell: Option<FftSwellField>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherPreset {
    Calm,
    Moderate,
    Storm,
}

impl WeatherPreset {
    pub fn energy(self) -> f32 {
        match self {
            WeatherPreset::Calm => 0.0,
            WeatherPreset::Moderate => 0.5,
            WeatherPreset::Storm => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DyeBand {
    Shallow,
    Deep,
}

impl DyeBand {
    pub fn name(self) -> &'static str {
        match self {
            DyeBand::Shallow => "shallow",
            DyeBand::Deep => "deep",
        }
    }
}

pub fn weather_strengths(energy: f32) -> (f32, f32) {
    let energy = energy.clamp(0.0, 1.0);
    if energy <= 0.5 {
        let t = energy / 0.5;
        (mix(0.10, 0.50, t), mix(0.25, 0.70, t))
    } else {
        let t = (energy - 0.5) / 0.5;
        (mix(0.50, 1.20, t), mix(0.70, 1.00, t))
    }
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
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
            detail: WaterDetail::default(),
            dye: WaterDye::default(),
            caustics: WaterCaustics::default(),
            refraction: 0.35,
            tides_enabled: true,
            swells_enabled: true,
            wetness_enabled: true,
            fft_enabled: true,
            swell_strength: 0.5,
            chop_strength: 0.7,
            fft_swell: None,
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

    pub fn effective_fft_strengths(&self) -> (f32, f32) {
        if self.fft_enabled && self.swells_enabled {
            (self.swell_strength, self.chop_strength)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn fft_swell_height_at(&self, x: f32, z: f32, time: f32) -> Option<f32> {
        if !self.fft_enabled || !self.swells_enabled {
            return None;
        }
        let field = self.fft_swell.as_ref()?;
        Some(field.height_at(x, z, time) * self.swell_strength)
    }

    pub fn shore_surface_height_at(
        &self,
        bed: &dyn SeaBed,
        x: f32,
        z: f32,
        time: f32,
    ) -> f32 {
        let shore = shore_context(bed, self, x, z);
        let swell = match self.fft_swell_height_at(x, z, time) {
            Some(height) => height * shore_vert_damp(shore.rest_depth),
            None => self.effective_swells().height_at_shored(x, z, time, shore),
        };
        let trough_floor = -(shore.rest_depth - TROUGH_CLEARANCE).max(0.0);
        self.sea_level + swell.max(trough_floor)
    }

    pub fn flooded_surface_height_at(
        &self,
        bed: &dyn SeaBed,
        x: f32,
        z: f32,
        time: f32,
    ) -> Option<f32> {
        if self.sea_level + self.max_crest_height() < bed.spill_height(x, z) {
            None
        } else {
            Some(self.shore_surface_height_at(bed, x, z, time))
        }
    }

    fn max_crest_height(&self) -> f32 {
        if let Some(field) = self.fft_swell.as_ref() {
            if self.fft_enabled && self.swells_enabled {
                return field.crest_span * self.swell_strength.max(0.0) + COAST_SPAN_FLOOR;
            }
        }
        let span = self
            .effective_swells()
            .amplitude_span()
            .max(COAST_SPAN_FLOOR);
        span * COAST_SHOAL_MAX * (0.5 + self.wave_energy)
    }

    pub fn set_base_level(&mut self, height: f32) -> String {
        self.base_level = height;
        format!("Water level: {height:.2}")
    }

    pub fn set_wave_energy(&mut self, energy: f32) -> String {
        self.wave_energy = energy.clamp(0.0, 1.0);
        format!("Wave energy: {:.2}", self.wave_energy)
    }

    pub fn set_refraction(&mut self, strength: f32) -> String {
        self.refraction = strength.max(0.0);
        format!("Water refraction: {:.2}", self.refraction)
    }

    pub fn set_caustic_strength(&mut self, strength: f32) -> String {
        self.caustics.strength = strength.max(0.0);
        format!("Caustic strength: {:.2}", self.caustics.strength)
    }

    pub fn set_dye_color(&mut self, band: DyeBand, color: Vec3) -> String {
        let color = color.clamp(Vec3::ZERO, Vec3::splat(1.0));
        match band {
            DyeBand::Shallow => self.dye.shallow = color,
            DyeBand::Deep => self.dye.deep = color,
        }
        format!("Water {}: {color:?}", band.name())
    }

    pub fn apply_weather(&mut self, preset: WeatherPreset) -> String {
        self.wave_energy = preset.energy();
        let (swell, chop) = weather_strengths(preset.energy());
        self.swell_strength = swell;
        self.chop_strength = chop;
        format!("Weather: {preset:?} (swell {swell:.2}, chop {chop:.2})")
    }

    pub fn set_swell_strength(&mut self, strength: f32) -> String {
        self.swell_strength = strength.clamp(0.0, 2.0);
        format!("Swell strength: {:.2}", self.swell_strength)
    }

    pub fn set_chop_strength(&mut self, strength: f32) -> String {
        self.chop_strength = strength.clamp(0.0, 2.0);
        format!("Chop strength: {:.2}", self.chop_strength)
    }

    pub fn toggle_tides(&mut self) -> String {
        self.tides_enabled = !self.tides_enabled;
        format!("Tides: {}", if self.tides_enabled { "on" } else { "off" })
    }

    pub fn toggle_swells(&mut self) -> String {
        self.swells_enabled = !self.swells_enabled;
        format!("Swells: {}", if self.swells_enabled { "on" } else { "off" })
    }

    pub fn toggle_fft(&mut self) -> String {
        self.fft_enabled = !self.fft_enabled;
        format!("FFT water: {}", if self.fft_enabled { "on" } else { "off" })
    }

    pub fn toggle_wetness(&mut self) -> String {
        self.wetness_enabled = !self.wetness_enabled;
        format!("Wet sand: {}", if self.wetness_enabled { "on" } else { "off" })
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
const WHITECAP_BASE: f32 = 0.2;
const WHITECAP_ENERGY: f32 = 0.6;
const SUN_RADIANCE_PER_LUX: f32 = 3.0 / bevy::light::light_consts::lux::AMBIENT_DAYLIGHT;

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

pub fn terrain_gradient(bed: &dyn SeaBed, x: f32, z: f32) -> Vec2 {
    let step = bed.texel().max_element().max(1.0);
    Vec2::new(
        (bed.ground_height(x + step, z) - bed.ground_height(x - step, z)) / (2.0 * step),
        (bed.ground_height(x, z + step) - bed.ground_height(x, z - step)) / (2.0 * step),
    )
}

fn shore_context(bed: &dyn SeaBed, sea: &SeaState, x: f32, z: f32) -> ShoreContext {
    let rest_depth = sea.sea_level - bed.ground_height(x, z);
    let gradient = terrain_gradient(bed, x, z);
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

#[cfg(test)]
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

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SeaState>()
            .init_resource::<SeaViewer>()
            .add_systems(Startup, setup_water_surface)
            .add_systems(
                Update,
                (
                    sync_sea_sun,
                    drive_sea_level,
                    poll_fft_bake,
                    sync_water_clipmap,
                    sync_water_params,
                    sync_wetness,
                )
                    .chain(),
            );
    }
}

fn setup_water_surface(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    sea_state: Res<SeaState>,
) {
    let placeholder = images.add(fft_placeholder_image());
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
            params: WaterParams {
                refraction: sea_state.refraction,
                ..default()
            },
            swells: sea_state.effective_swells(),
            detail_settings: sea_state.detail,
            dye_settings: sea_state.dye,
            caustics_settings: sea_state.caustics,
            body_volume: SdfVolume::from_bounds(
                Vec3::splat(-0.5),
                Vec3::splat(0.5),
                Vec3::splat(2.0),
            ),
            body_sdf: empty_sdf_image(&mut images),
            swell_displacement: placeholder.clone(),
            swell_derivatives: placeholder.clone(),
            chop_displacement: placeholder.clone(),
            chop_derivatives: placeholder.clone(),
            ..default()
        },
    });
    commands.insert_resource(WaterSurface {
        material: material.clone(),
    });
    commands.insert_resource(FftWaterTextures {
        ready: false,
        swell_displacement: placeholder.clone(),
        swell_derivatives: placeholder.clone(),
        chop_displacement: placeholder.clone(),
        chop_derivatives: placeholder,
    });
    commands.insert_resource(spawn_fft_bake());
    commands.spawn((
        Name::new("Sea"),
        Sea,
        Transform::from_xyz(0.0, sea_state.sea_level, 0.0),
        Mesh3d(meshes.add(build_water_clipmap_mesh())),
        MeshMaterial3d(material),
        NotShadowCaster,
    ));
}

fn poll_fft_bake(
    mut commands: Commands,
    receiver: Option<Res<FftBakeReceiver>>,
    mut images: ResMut<Assets<Image>>,
    textures: Option<ResMut<FftWaterTextures>>,
    mut sea_state: ResMut<SeaState>,
) {
    let Some(receiver) = receiver else {
        return;
    };
    let baked = match receiver.receiver.lock() {
        Ok(guard) => match guard.try_recv() {
            Ok(baked) => Some(baked),
            Err(_) => None,
        },
        Err(_) => None,
    };
    let Some((swell, chop)) = baked else {
        return;
    };
    let swell_displacement_image = fft_displacement_image(&swell);
    let swell_derivatives_image = fft_derivatives_image(&swell);
    let chop_displacement_image = fft_displacement_image(&chop);
    let chop_derivatives_image = fft_derivatives_image(&chop);
    let swell_displacement = images.add(swell_displacement_image);
    let swell_derivatives = images.add(swell_derivatives_image);
    let chop_displacement = images.add(chop_displacement_image);
    let chop_derivatives = images.add(chop_derivatives_image);
    if let Some(mut textures) = textures {
        textures.ready = true;
        textures.swell_displacement = swell_displacement;
        textures.swell_derivatives = swell_derivatives;
        textures.chop_displacement = chop_displacement;
        textures.chop_derivatives = chop_derivatives;
    }
    sea_state.fft_swell = Some(swell.swell_field);
    commands.remove_resource::<FftBakeReceiver>();
}

fn drive_sea_level(time: Res<Time>, mut sea_state: ResMut<SeaState>) {
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

fn sync_sea_sun(
    lights: Query<(Ref<'_, DirectionalLight>, Ref<'_, Transform>)>,
    mut sea_state: ResMut<SeaState>,
) {
    let Some((light, transform)) = lights.iter().next() else {
        return;
    };
    if !light.is_changed() && !transform.is_changed() {
        return;
    }
    sea_state.optics.sun_direction = -*transform.forward();
    sea_state.optics.sun_illuminance =
        light.color.to_linear().to_vec3() * (light.illuminance * SUN_RADIANCE_PER_LUX);
}

fn sync_water_clipmap(
    viewer: Res<SeaViewer>,
    sea_state: Res<SeaState>,
    mut sea: Query<&mut Transform, With<Sea>>,
) {
    let snapped = snap_clipmap_center(Vec2::new(viewer.position.x, viewer.position.z));
    for mut transform in &mut sea {
        transform.translation = Vec3::new(snapped.x, sea_state.sea_level, snapped.y);
    }
}

fn sync_water_params(
    time: Res<Time>,
    sea_state: Res<SeaState>,
    surface: Option<Res<WaterSurface>>,
    fft_textures: Option<Res<FftWaterTextures>>,
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
    params.refraction = sea_state.refraction;
    params.optics = sea_state.optics;
    material.extension.params = params;
    material.extension.swells = sea_state.effective_swells();
    material.extension.detail_settings = sea_state.detail;
    material.extension.dye_settings = sea_state.dye;
    material.extension.caustics_settings = sea_state.caustics;

    let (swell_strength, chop_strength) = sea_state.effective_fft_strengths();
    let mut fft_params = material.extension.fft_params;
    fft_params.swell_strength = swell_strength;
    fft_params.chop_strength = chop_strength;
    fft_params.whitecap_gain = WHITECAP_BASE + WHITECAP_ENERGY * sea_state.wave_energy;
    material.extension.fft_params = fft_params;
    if let Some(textures) = fft_textures {
        if textures.ready {
            material.extension.swell_displacement = textures.swell_displacement.clone();
            material.extension.swell_derivatives = textures.swell_derivatives.clone();
            material.extension.chop_displacement = textures.chop_displacement.clone();
            material.extension.chop_derivatives = textures.chop_derivatives.clone();
        }
        material.extension.fft =
            textures.ready && sea_state.fft_enabled && sea_state.swells_enabled;
    }
}

fn sync_wetness(
    sea_state: Res<SeaState>,
    surface: Option<Res<WetSandSurface>>,
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

pub fn flip_material_flag(
    surface: Option<&WaterSurface>,
    materials: &mut Assets<WaterMaterial>,
    label: &str,
    flag: impl FnOnce(&mut WaterMaterialExt) -> &mut bool,
) -> Option<String> {
    let surface = surface?;
    let mut material = materials.get_mut(&surface.material)?;
    let value = flag(&mut material.extension);
    *value = !*value;
    Some(format!("{label}: {}", if *value { "on" } else { "off" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use bevy_shader_kit::{WetSandExt, bake_band, FftBandConfig};

    struct FlatShore {
        ground: f32,
        spill: f32,
    }

    impl FlatShore {
        fn new(ground: f32, spill: f32) -> Self {
            Self { ground, spill }
        }
    }

    impl SeaBed for FlatShore {
        fn ground_height(&self, _x: f32, _z: f32) -> f32 {
            self.ground
        }

        fn spill_height(&self, _x: f32, _z: f32) -> f32 {
            self.spill
        }

        fn texel(&self) -> Vec2 {
            Vec2::ONE
        }
    }

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
            .init_resource::<SeaViewer>()
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
            .insert_resource(SeaViewer {
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
        let shore = FlatShore::new(0.0, 0.0);
        let sea = SeaState {
            sea_level: 0.2,
            wave_energy: 1.0,
            swells_enabled: false,
            ..default()
        };
        for step in 0..12 {
            assert_eq!(
                sea.shore_surface_height_at(&shore, 0.0, 0.0, step as f32 * 0.35),
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
            .insert_resource(WetSandSurface {
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
            extension: WaterMaterialExt::default(),
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
            .add_systems(Update, (drive_sea_level, sync_water_params));
        app.world_mut().resource_mut::<SeaState>().toggle_swells();
        app.world_mut().resource_mut::<SeaState>().toggle_tides();
        for _ in 0..4 {
            app.update();
        }

        let material = app
            .world()
            .resource::<Assets<WaterMaterial>>()
            .get(&handle)
            .unwrap();
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
        let shore = FlatShore::new(0.0, 0.0);
        let sea = SeaState {
            sea_level: -1.0,
            ..default()
        };
        for step in 0..24 {
            let time = step as f32 * 0.35;
            assert_eq!(
                sea.shore_surface_height_at(&shore, 0.0, 0.0, time),
                sea.sea_level,
                "shoaling damps the swell to nothing once the ground breaks the surface (t={time})"
            );
        }
    }

    #[test]
    fn a_trough_never_opens_below_the_seabed() {
        let shore = FlatShore::new(0.0, 0.0);
        let sea = SeaState {
            sea_level: 0.2,
            wave_energy: 1.0,
            ..default()
        };
        for step in 0..24 {
            let time = step as f32 * 0.35;
            assert!(
                sea.shore_surface_height_at(&shore, 0.0, 0.0, time) >= shore.ground,
                "a trough at t={time} dropped below the seabed"
            );
        }
    }

    #[test]
    fn ground_the_sea_never_reaches_stays_dry() {
        let shore = FlatShore::new(0.0, 0.0);
        let sea = SeaState {
            sea_level: -1.0,
            ..default()
        };
        assert_eq!(sea.flooded_surface_height_at(&shore, 0.0, 0.0, 1.0), None);
    }

    #[test]
    fn the_sea_floods_the_plain_once_it_spills_in() {
        let shore = FlatShore::new(0.0, 0.0);
        let sea = SeaState {
            sea_level: 0.0,
            ..default()
        };
        assert!(sea.flooded_surface_height_at(&shore, 0.0, 0.0, 1.0).is_some());
    }

    #[test]
    fn the_shader_settings_reach_the_material() {
        let mut materials = Assets::<WaterMaterial>::default();
        let handle = materials.add(WaterMaterial {
            base: StandardMaterial::default(),
            extension: WaterMaterialExt::default(),
        });
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(20),
            ))
            .insert_resource(SeaState {
                refraction: 0.9,
                detail: WaterDetail {
                    normal_gain: 1.25,
                    ..default()
                },
                dye: WaterDye {
                    beers_law: 3.5,
                    ..default()
                },
                caustics: WaterCaustics {
                    strength: 0.0,
                    ..default()
                },
                ..default()
            })
            .insert_resource(WaterSurface {
                material: handle.clone(),
            })
            .insert_resource(materials)
            .add_systems(Update, sync_water_params);
        app.update();

        let material = app
            .world()
            .resource::<Assets<WaterMaterial>>()
            .get(&handle)
            .unwrap();
        assert_eq!(material.extension.params.refraction, 0.9);
        assert_eq!(material.extension.detail_settings.normal_gain, 1.25);
        assert_eq!(material.extension.dye_settings.beers_law, 3.5);
        assert_eq!(material.extension.caustics_settings.strength, 0.0);
    }

    #[test]
    fn the_surface_toggles_stick_across_sync() {
        let mut materials = Assets::<WaterMaterial>::default();
        let handle = materials.add(WaterMaterial {
            base: StandardMaterial::default(),
            extension: WaterMaterialExt::default(),
        });
        let surface = WaterSurface {
            material: handle.clone(),
        };
        flip_material_flag(Some(&surface), &mut materials, "Water detail", |ext| {
            &mut ext.detail
        });
        flip_material_flag(Some(&surface), &mut materials, "Water dye", |ext| {
            &mut ext.dye
        });
        flip_material_flag(Some(&surface), &mut materials, "Water caustics", |ext| {
            &mut ext.caustics
        });
        flip_material_flag(Some(&surface), &mut materials, "Water refraction", |ext| {
            &mut ext.refract
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
            .add_systems(Update, sync_water_params);
        for _ in 0..4 {
            app.update();
        }

        let material = app
            .world()
            .resource::<Assets<WaterMaterial>>()
            .get(&handle)
            .unwrap();
        assert!(!material.extension.detail);
        assert!(!material.extension.dye);
        assert!(!material.extension.caustics);
        assert!(!material.extension.refract);
    }

    #[test]
    fn weather_presets_land_on_their_marks() {
        let calm = weather_strengths(WeatherPreset::Calm.energy());
        let moderate = weather_strengths(WeatherPreset::Moderate.energy());
        let storm = weather_strengths(WeatherPreset::Storm.energy());
        assert!((calm.0 - 0.10).abs() < 1e-5 && (calm.1 - 0.25).abs() < 1e-5);
        assert!((moderate.0 - 0.50).abs() < 1e-5 && (moderate.1 - 0.70).abs() < 1e-5);
        assert!((storm.0 - 1.20).abs() < 1e-5 && (storm.1 - 1.00).abs() < 1e-5);
        for step in 0..10 {
            let lower = weather_strengths(step as f32 * 0.1);
            let upper = weather_strengths(step as f32 * 0.1 + 0.1);
            assert!(upper.0 >= lower.0 && upper.1 >= lower.1, "weather must ramp up");
        }
    }

    #[test]
    fn the_weather_preset_blends_the_bands() {
        let mut sea = SeaState::default();
        sea.apply_weather(WeatherPreset::Storm);
        assert_eq!(sea.wave_energy, 1.0);
        assert!((sea.swell_strength - 1.2).abs() < 1e-5);
        assert!((sea.chop_strength - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_fft_toggle_sticks() {
        let mut sea = SeaState::default();
        sea.toggle_fft();
        assert!(!sea.fft_enabled);
        sea.toggle_fft();
        assert!(sea.fft_enabled);
    }

    #[test]
    fn swells_off_zero_the_fft_strengths() {
        let sea = SeaState {
            swells_enabled: false,
            ..default()
        };
        assert_eq!(sea.effective_fft_strengths(), (0.0, 0.0));
        assert!(sea.fft_swell_height_at(2.0, 3.0, 1.0).is_none());
    }

    #[test]
    fn the_fft_swell_drives_the_shore_height() {
        let band = bake_band(&FftBandConfig {
            tile_size: 8.0,
            resolution: 16,
            frames: 4,
            loop_period: 2.0,
            wind_speed: 10.0,
            wind_direction: Vec2::X,
            fetch: 100.0,
            directional_spread: 4.0,
            min_wavelength: 1.0,
            max_wavelength: 8.0,
            choppiness: 1.0,
            amplitude: 1.0,
            seed: 7,
        });
        let shore = FlatShore::new(0.0, 0.0);
        let sea = SeaState {
            sea_level: 5.0,
            swell_strength: 1.0,
            fft_swell: Some(band.swell_field.clone()),
            ..default()
        };
        let mut heights = Vec::new();
        for step in 0..8 {
            let time = step as f32 * 0.3;
            let height = sea.shore_surface_height_at(&shore, 3.0, -4.0, time);
            assert!(height.is_finite());
            heights.push(height);
        }
        assert!(
            heights.windows(2).any(|pair| (pair[1] - pair[0]).abs() > 1e-4),
            "the fft swell must move the shore surface"
        );
        let start = sea.shore_surface_height_at(&shore, 3.0, -4.0, 0.0);
        let looped = sea.shore_surface_height_at(&shore, 3.0, -4.0, band.loop_period);
        assert!((start - looped).abs() < 1e-4, "the loop must close at the shore too");
    }

    #[test]
    fn fft_strengths_reach_the_material() {
        let mut materials = Assets::<WaterMaterial>::default();
        let handle = materials.add(WaterMaterial {
            base: StandardMaterial::default(),
            extension: WaterMaterialExt::default(),
        });
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(20),
            ))
            .insert_resource(SeaState {
                swell_strength: 0.9,
                chop_strength: 0.4,
                ..default()
            })
            .insert_resource(WaterSurface {
                material: handle.clone(),
            })
            .insert_resource(FftWaterTextures {
                ready: false,
                ..default()
            })
            .insert_resource(materials)
            .add_systems(Update, sync_water_params);
        app.update();

        let material = app
            .world()
            .resource::<Assets<WaterMaterial>>()
            .get(&handle)
            .unwrap();
        assert!((material.extension.fft_params.swell_strength - 0.9).abs() < 1e-6);
        assert!((material.extension.fft_params.chop_strength - 0.4).abs() < 1e-6);
        assert!(!material.extension.fft, "fft stays off until the bake lands");
    }

    #[test]
    fn the_sea_faces_the_way_the_light_arrives_from() {
        use bevy::light::light_consts::lux;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SeaState>()
            .world_mut()
            .spawn((
                DirectionalLight {
                    illuminance: lux::AMBIENT_DAYLIGHT,
                    ..default()
                },
                Transform::from_xyz(0.0, 5.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
            ));
        app.add_systems(Update, sync_sea_sun);
        app.update();

        let optics = &app.world().resource::<SeaState>().optics;
        assert!(
            optics.sun_direction.dot(Vec3::Y) > 0.99,
            "a light shining straight down means the sun is overhead"
        );
        assert!(
            (optics.sun_illuminance.x - 3.0).abs() < 1.0e-3,
            "the default daylight lux maps back onto the water's in-scatter range"
        );
    }
}
