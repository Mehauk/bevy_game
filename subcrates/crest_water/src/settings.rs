use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub struct WaveSettings {
    pub wind_speed: f32,
    pub wind_direction: Vec2,
    pub gravity: f32,
    pub chop: f32,
    pub turbulence: f32,
    pub amplitude_multiplier: f32,
    pub loop_period: f32,
    pub attenuation_in_shallows: f32,
    pub shallows_max_depth: f32,
    pub gerstner_chop: f32,
    pub gerstner_steepness: f32,
    pub gerstner_count: u32,
    pub gerstner_wavelengths: [f32; 16],
    pub gerstner_amplitudes: [f32; 16],
    pub gerstner_directions: [Vec2; 16],
    pub gerstner_speeds: [f32; 16],
    pub gerstner_phases: [f32; 16],
}

impl Default for WaveSettings {
    fn default() -> Self {
        Self {
            wind_speed: 4.0,
            wind_direction: Vec2::X,
            gravity: 9.81,
            chop: 1.6,
            turbulence: 0.4,
            amplitude_multiplier: 1.0,
            loop_period: 32.0,
            attenuation_in_shallows: 0.95,
            shallows_max_depth: 1000.0,
            gerstner_chop: 0.0,
            gerstner_steepness: 0.0,
            gerstner_count: 0,
            gerstner_wavelengths: [1.0; 16],
            gerstner_amplitudes: [0.0; 16],
            gerstner_directions: [Vec2::X; 16],
            gerstner_speeds: [0.0; 16],
            gerstner_phases: [0.0; 16],
        }
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct OceanSettings {
    pub water_level: f32,
    pub waves: WaveSettings,
    pub lod_count: usize,
    pub lod_resolution: usize,
    pub base_texel_size: f32,
    pub dynamic_waves_enabled: bool,
    pub foam_enabled: bool,
    pub flow_enabled: bool,
    pub dyn_wave_speed: f32,
    pub dyn_damping: f32,
}

impl Default for OceanSettings {
    fn default() -> Self {
        Self {
            water_level: 0.0,
            waves: WaveSettings::default(),
            lod_count: 7,
            lod_resolution: 64,
            base_texel_size: 0.5,
            dynamic_waves_enabled: true,
            foam_enabled: true,
            flow_enabled: true,
            dyn_wave_speed: 2.0,
            dyn_damping: 1.0,
        }
    }
}

impl OceanSettings {
    pub fn texel_size(&self, lod: usize) -> f32 {
        self.base_texel_size * (1u32 << lod.min(30)) as f32
    }

    pub fn cascade_extent(&self, lod: usize) -> f32 {
        self.texel_size(lod) * self.lod_resolution as f32
    }
}

pub fn average_wavelength(settings: &OceanSettings) -> f32 {
    let g = settings.waves.gravity.max(0.1);
    let wm = 0.87 * g / settings.waves.wind_speed.max(0.1);
    let mut sum = 0.0f32;
    let mut weight = 0.0f32;
    for octave in 0..14 {
        let wavelength = 2.0f32.powi(octave - 4);
        let k = std::f32::consts::TAU / wavelength;
        let w = (g * k).sqrt();
        let pm = (-1.291f32 * (wm / w).powi(4)).exp();
        let s = pm * k.powi(-3);
        sum += wavelength * s;
        weight += s;
    }
    if weight > 0.0 {
        sum / weight
    } else {
        1.0
    }
}
