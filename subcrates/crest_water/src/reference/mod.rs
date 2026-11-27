pub mod combine;
pub mod fft;
pub mod flow;
pub mod foam;
pub mod dynwaves;
pub mod gerstner;
pub mod query;
pub mod spectrum;

pub use combine::{AnimatedWaveSample, animated_wave_height};
pub use fft::{FftBaked, bake_frame, fft_2d};
pub use flow::{flow_from_spline, SplineSpec};
pub use foam::{FoamSettings, foam_decay_step, foam_source};
pub use dynwaves::DynWavesSim;
pub use gerstner::{GerstnerSurface, GerstnerWave, gerstner_surface};
pub use query::{fpi_surface_height, sample_bilinear};
pub use spectrum::{
    GerstnerSpectrumComponent, POWER_LOG, Rng, SpectrumFrame, deep_dispersion, gerstner_amplitudes,
    spectrum_initialize, spectrum_update, wang_hash,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seabed {
    Flat { height: f32 },
    Slope { base: f32, slope_x: f32, slope_z: f32, min: f32, max: f32 },
}

impl Seabed {
    pub fn height(&self, x: f32, z: f32) -> f32 {
        match self {
            Seabed::Flat { height } => *height,
            Seabed::Slope { base, slope_x, slope_z, min, max } => {
                (base + slope_x * x + slope_z * z).clamp(*min, *max)
            }
        }
    }
}
