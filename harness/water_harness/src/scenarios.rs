use bevy::prelude::*;
use crest_water::reference::{SplineSpec, Seabed};
use crest_water::settings::OceanSettings;

pub const SIM_DT: f32 = 1.0 / 60.0;
pub const FREEZE_FRAMES: u64 = 10;
pub const VIEWER: Vec3 = Vec3::new(16.0, 0.0, 16.0);
pub const QUERY: Vec3 = Vec3::new(16.0, 0.0, 16.0);

#[derive(Clone, Debug)]
pub struct WakeSpec {
    pub start: Vec2,
    pub direction: Vec2,
    pub speed: f32,
    pub radius: f32,
    pub strength: f32,
    pub velocity_strength: f32,
    pub start_frame: u64,
}

#[derive(Clone, Debug)]
pub struct Scenario {
    pub name: &'static str,
    pub settings: OceanSettings,
    pub seabed: Seabed,
    pub splines: Vec<SplineSpec>,
    pub wake: Option<WakeSpec>,
    pub frames: u64,
    pub probes: Vec<Vec2>,
}

#[derive(Resource, Clone)]
pub struct ScenarioResource(pub Scenario);

fn celerity(wavelength: f32, gravity: f32) -> f32 {
    (wavelength * gravity / std::f32::consts::TAU).sqrt()
}

fn with_gerstner(
    settings: &mut OceanSettings,
    waves: &[(f32, f32, Vec2, f32)],
    steepness: f32,
) {
    let gravity = settings.waves.gravity;
    settings.waves.gerstner_steepness = steepness;
    settings.waves.gerstner_count = waves.len() as u32;
    for (index, (wavelength, amplitude, direction, phase)) in waves.iter().enumerate() {
        settings.waves.gerstner_wavelengths[index] = *wavelength;
        settings.waves.gerstner_amplitudes[index] = *amplitude;
        settings.waves.gerstner_directions[index] = *direction;
        settings.waves.gerstner_speeds[index] = celerity(*wavelength, gravity);
        settings.waves.gerstner_phases[index] = *phase;
    }
}

fn probes() -> Vec<Vec2> {
    (-2..2)
        .flat_map(|k| {
            (-2..2).map(move |m| {
                QUERY.xz()
                    + Vec2::new(0.25 + 0.5 * k as f32, 0.25 + 0.5 * m as f32)
            })
        })
        .collect()
}

pub fn scenarios() -> Vec<Scenario> {
    let mut calm = OceanSettings::default();
    calm.waves.wind_speed = 4.0;

    let mut storm = OceanSettings::default();
    storm.waves.wind_speed = 12.0;
    storm.waves.chop = 2.2;
    storm.waves.turbulence = 0.6;
    with_gerstner(
        &mut storm,
        &[
            (12.0, 0.6, Vec2::new(0.9, 0.3), 0.3),
            (8.0, 0.35, Vec2::new(0.7, 0.7), 1.1),
            (5.0, 0.2, Vec2::new(0.4, 0.9), 2.0),
        ],
        0.6,
    );

    let mut shore = OceanSettings::default();
    shore.waves.wind_speed = 5.0;
    shore.waves.attenuation_in_shallows = 0.95;
    shore.waves.shallows_max_depth = 4.0;

    let mut river = OceanSettings::default();
    river.waves.wind_speed = 3.0;

    let mut wakes = OceanSettings::default();
    wakes.dyn_wave_speed = 2.0;
    wakes.dyn_damping = 1.0;

    let mut curtain = OceanSettings::default();
    curtain.waves.wind_speed = 6.0;

    let mut tempest = OceanSettings::default();
    tempest.waves.wind_speed = 24.0;
    tempest.waves.chop = 3.0;
    tempest.waves.turbulence = 1.0;
    tempest.waves.amplitude_multiplier = 1.6;
    with_gerstner(
        &mut tempest,
        &[
            (20.0, 1.4, Vec2::new(0.9, 0.2), 0.0),
            (14.0, 0.9, Vec2::new(0.8, 0.5), 0.7),
            (9.0, 0.5, Vec2::new(0.6, 0.8), 1.6),
            (6.0, 0.3, Vec2::new(0.3, 0.95), 2.4),
        ],
        0.9,
    );

    vec![
        Scenario {
            name: "calm_ocean",
            settings: calm,
            seabed: Seabed::Flat { height: -50.0 },
            splines: vec![],
            wake: None,
            frames: 120,
            probes: probes(),
        },
        Scenario {
            name: "storm_swell",
            settings: storm,
            seabed: Seabed::Flat { height: -50.0 },
            splines: vec![],
            wake: None,
            frames: 120,
            probes: probes(),
        },
        Scenario {
            name: "shallow_shore",
            settings: shore,
            seabed: Seabed::Slope {
                base: -2.0,
                slope_x: 0.0,
                slope_z: 0.08,
                min: -1000.0,
                max: 1000.0,
            },
            splines: vec![],
            wake: None,
            frames: 120,
            probes: probes(),
        },
        Scenario {
            name: "spline_river",
            settings: river,
            seabed: Seabed::Flat { height: -2.0 },
            splines: vec![SplineSpec {
                points: vec![[8.0, 0.0, 16.0], [16.0, 0.0, 16.0], [24.0, 0.0, 16.0]],
                width: 3.0,
                flow_speed: 2.0,
                wave_multiplier: 1.0,
                foam_amount: 0.0,
            }],
            wake: None,
            frames: 60,
            probes: probes(),
        },
        Scenario {
            name: "boat_wakes",
            settings: wakes,
            seabed: Seabed::Flat { height: -50.0 },
            splines: vec![],
            wake: Some(WakeSpec {
                start: Vec2::new(12.0, 16.0),
                direction: Vec2::X,
                speed: 2.0,
                radius: 1.5,
                strength: 0.01,
                velocity_strength: 0.02,
                start_frame: 30,
            }),
            frames: 180,
            probes: probes(),
        },
        Scenario {
            name: "underwater_curtain",
            settings: curtain,
            seabed: Seabed::Slope {
                base: -6.0,
                slope_x: 0.0,
                slope_z: 0.05,
                min: -1000.0,
                max: 1000.0,
            },
            splines: vec![],
            wake: None,
            frames: 90,
            probes: probes(),
        },
        Scenario {
            name: "stormy",
            settings: tempest,
            seabed: Seabed::Flat { height: -50.0 },
            splines: vec![],
            wake: None,
            frames: 120,
            probes: probes(),
        },
    ]
}
