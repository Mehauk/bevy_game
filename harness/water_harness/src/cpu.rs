use bevy::prelude::*;
use crest_water::lod::LodTransforms;
use crest_water::reference::{
    DynWavesSim, GerstnerWave, SplineSpec, Seabed, animated_wave_height, bake_frame,
    flow_from_spline, gerstner_surface, spectrum_initialize, spectrum_update,
};
use crest_water::settings::{OceanSettings, WaveSettings};

use crate::scenarios::{SIM_DT, Scenario, WakeSpec};

pub struct CpuSample {
    pub height: f32,
    pub disp_x: f32,
    pub disp_y: f32,
    pub disp_z: f32,
    pub jacobian: f32,
    pub flow: Vec2,
    pub depth: f32,
}

fn gerstner_waves(waves: &WaveSettings) -> Vec<GerstnerWave> {
    (0..waves.gerstner_count.min(16) as usize)
        .map(|index| GerstnerWave {
            wavelength: waves.gerstner_wavelengths[index],
            amplitude: waves.gerstner_amplitudes[index],
            direction: waves.gerstner_directions[index],
            speed: waves.gerstner_speeds[index],
            steepness: waves.gerstner_steepness,
            phase: waves.gerstner_phases[index],
        })
        .collect()
}

pub struct CpuOcean {
    settings: OceanSettings,
    seabed: Seabed,
    splines: Vec<SplineSpec>,
    wake: Option<WakeSpec>,
    total_frames: u64,
    size: usize,
    lod_origin: (f32, f32),
    fft_texel: f32,
    init: Vec<[f32; 4]>,
    sim: Option<DynWavesSim>,
    frame: u64,
}

impl CpuOcean {
    pub fn new(scenario: &Scenario, viewer: Vec3) -> Self {
        let settings = scenario.settings.clone();
        let size = settings.lod_resolution;
        let lods = LodTransforms::compute(&settings, viewer);
        let origin = lods.transforms[0].origin.xz();
        let waves = &settings.waves;
        let init = spectrum_initialize(
            size,
            0,
            settings.lod_count,
            settings.cascade_extent(0),
            waves.wind_direction,
            waves.wind_speed,
            waves.gravity,
            waves.loop_period,
            waves.turbulence,
            waves.chop,
            &crest_water::reference::POWER_LOG,
        );
        let sim = settings.dynamic_waves_enabled.then(|| {
            let mut sim = DynWavesSim::new(size);
            sim.texel = settings.texel_size(0);
            sim
        });
        let fft_texel = settings.texel_size(0);
        Self {
            settings,
            seabed: scenario.seabed,
            splines: scenario.splines.clone(),
            wake: scenario.wake.clone(),
            total_frames: scenario.frames,
            size,
            lod_origin: (origin.x, origin.y),
            fft_texel,
            init,
            sim,
            frame: 0,
        }
    }

    pub fn step(&mut self) {
        self.frame += 1;
        let Some(sim) = self.sim.as_mut() else {
            return;
        };
        sim.step(SIM_DT, self.settings.dyn_wave_speed, self.settings.dyn_damping);
        let Some(wake) = self.wake.as_ref() else {
            return;
        };
        if self.frame > wake.start_frame && self.frame <= self.total_frames {
            let elapsed = (self.frame - wake.start_frame) as f32 * SIM_DT;
            let position = wake.start + wake.direction * (wake.speed * elapsed);
            sim.splat(position, wake.radius, wake.strength, wake.velocity_strength);
        }
    }

    pub fn freeze(&mut self) {
        if let Some(sim) = self.sim.as_mut() {
            sim.step(0.0, self.settings.dyn_wave_speed, self.settings.dyn_damping);
        }
    }

    pub fn time(&self) -> f32 {
        self.frame.saturating_sub(1) as f32 * SIM_DT
    }

    pub fn sample(&self, x: f32, z: f32) -> CpuSample {
        let time = self.time();
        let waves = &self.settings.waves;
        let frame = spectrum_update(
            &self.init,
            self.size,
            0,
            self.settings.cascade_extent(0),
            time,
            waves.gravity,
            waves.loop_period,
            waves.chop,
        );
        let baked = bake_frame(
            &frame.height,
            &frame.disp_x,
            &frame.disp_z,
            self.size,
            self.fft_texel,
        );
        let gerstner = gerstner_surface(&gerstner_waves(waves), x, z, time);
        let animated = animated_wave_height(
            &baked,
            self.size,
            self.fft_texel,
            self.lod_origin,
            x,
            z,
            self.settings.water_level,
            &self.seabed,
            waves.attenuation_in_shallows,
            waves.shallows_max_depth,
            &gerstner,
        );
        let dynamic = self
            .sim
            .as_ref()
            .map(|sim| sim.sample(x, z, sim.texel))
            .unwrap_or(0.0);
        let flow = self
            .splines
            .iter()
            .map(|spline| flow_from_spline(spline, x, z))
            .fold(Vec2::ZERO, |sum, flow| sum + flow);
        let depth = (self.settings.water_level - self.seabed.height(x, z)).max(0.0);
        CpuSample {
            height: animated.height + dynamic,
            disp_x: animated.displacement.x,
            disp_y: animated.displacement.y + dynamic,
            disp_z: animated.displacement.z,
            jacobian: animated.jacobian * gerstner.jacobian,
            flow,
            depth,
        }
    }
}
