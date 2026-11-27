use bevy::prelude::*;

pub const POWER_LOG: [f32; 14] = [
    -7.10794, -6.42794, -5.93794, -5.27794, -4.67794, -3.71794, -3.17794, -2.60794, -1.93794,
    -1.11794, -0.85794, -0.36794, 0.04206, -9.39794,
];

const INV_PI2: f32 = std::f32::consts::FRAC_2_PI;
const PI4: f32 = 0.33661977;
const PI2: f32 = std::f32::consts::TAU;
const SPECTRUM_SMALLEST_WL_POW_2: f32 = -4.0;
const MIN_POWER_LOG: f32 = -8.0;
const WAVE_SAMPLE_FACTOR: u32 = 8;

pub struct Rng {
    pub state: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self {
            state: wang_hash(seed),
        }
    }

    pub fn rand_u32(&mut self) -> u32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state
    }

    pub fn rand_f32(&mut self) -> f32 {
        self.rand_u32() as f32 / 4294967296.0
    }

    pub fn rand_gauss(&mut self) -> f32 {
        let mut u1 = self.rand_f32();
        let u2 = self.rand_f32();
        if u1 < 1e-6 {
            u1 = 1e-6;
        }
        (-2.0 * u1.ln()).sqrt() * (PI2 * u2).cos()
    }
}

pub fn wang_hash(seed: u32) -> u32 {
    let mut s = (seed ^ 61) ^ (seed >> 16);
    s = s.wrapping_mul(9);
    s ^= s >> 4;
    s = s.wrapping_mul(0x27d4eb2d);
    s ^= s >> 15;
    s
}

pub fn deep_dispersion(k_mag: f32, gravity: f32, period: f32) -> (f32, f32) {
    let mut w = (gravity * k_mag).abs().sqrt();
    if period > 0.0 {
        let this_period = PI2 / w;
        let loops = (period / this_period).ceil();
        let snapped_period = period / loops;
        w = PI2 / snapped_period;
    }
    (w, gravity / (2.0 * w))
}

fn pierson_moskowitz_wind_term(w: f32, gravity: f32, wind_speed: f32) -> f32 {
    let wm = 0.87 * gravity / wind_speed;
    (-1.291 * (wm / w).powi(4)).exp()
}

fn pos_cos_squared_directional_spreading(cos_theta: f32, turbulence: f32) -> f32 {
    if cos_theta > 0.0 {
        (INV_PI2 * (cos_theta * cos_theta)).lerp(PI4, turbulence)
    } else {
        PI4 * turbulence
    }
}

fn spectrum_power_at(octave_index: f32, power_log: &[f32; 14]) -> f32 {
    let tc = octave_index.clamp(0.0, 13.0);
    let i0 = tc.floor() as usize;
    let i1 = (i0 + 1).min(13);
    let f = tc - tc.floor();
    let p0 = 10.0f32.powf(power_log[i0]);
    let p1 = 10.0f32.powf(power_log[i1]);
    p0.lerp(p1, f)
}

#[allow(clippy::too_many_arguments)]
pub fn spectrum_initialize(
    size: usize,
    lod: usize,
    lod_count: usize,
    world_base: f32,
    wind_dir: Vec2,
    wind_speed: f32,
    gravity: f32,
    period: f32,
    turbulence: f32,
    _chop: f32,
    power_log: &[f32; 14],
) -> Vec<[f32; 4]> {
    let center = size / 2;
    let world_size = world_base * (1u32 << lod) as f32;
    let mut out = vec![[0.0f32; 4]; size * size];
    for y in 0..size {
        for x in 0..size {
            let coord_x = x as i32 - center as i32;
            let coord_y = y as i32 - center as i32;
            if coord_x == 0 && coord_y == 0 {
                continue;
            }
            let max_coord = coord_x.abs().max(coord_y.abs()) as u32;
            if lod < lod_count - 1
                && !(WAVE_SAMPLE_FACTOR / 2..WAVE_SAMPLE_FACTOR).contains(&max_coord)
            {
                continue;
            }

            let k = [
                PI2 * coord_x as f32 / world_size,
                PI2 * coord_y as f32 / world_size,
            ];
            let k_mag = (k[0] * k[0] + k[1] * k[1]).sqrt();

            let seed = (lod as u32)
                .wrapping_mul((size * size) as u32)
                .wrapping_add((y * size + x) as u32);
            let mut rng = Rng {
                state: wang_hash(seed),
            };

            let (w, dwdk) = deep_dispersion(k_mag, gravity, period);

            let wavelength = PI2 / k_mag;
            let octave_index = wavelength.log2() - SPECTRUM_SMALLEST_WL_POW_2;
            let spectrum =
                spectrum_power_at(octave_index, power_log) * pierson_moskowitz_wind_term(w, gravity, wind_speed);

            let mut delta_s_pos = spectrum;
            let mut delta_s_neg = spectrum;

            let cos_theta = (k[0] * wind_dir.x + k[1] * wind_dir.y) / k_mag;
            delta_s_pos *= pos_cos_squared_directional_spreading(cos_theta, turbulence);
            delta_s_neg *= pos_cos_squared_directional_spreading(-cos_theta, turbulence);
            let d_k = PI2 / world_size;
            delta_s_pos *= (d_k * d_k) * dwdk / k_mag;
            delta_s_neg *= (d_k * d_k) * dwdk / k_mag;

            let amp_pos = rng.rand_gauss() * (delta_s_pos.abs() * 2.0).sqrt();
            let amp_neg = rng.rand_gauss() * (delta_s_neg.abs() * 2.0).sqrt();

            let phase_pos = rng.rand_f32() * PI2;
            let phase_neg = rng.rand_f32() * PI2;

            let spicey_multiplier = 1.5;
            out[y * size + x] = [
                amp_pos * phase_pos.cos() * spicey_multiplier,
                -amp_pos * phase_pos.sin() * spicey_multiplier,
                amp_neg * phase_neg.cos() * spicey_multiplier,
                -amp_neg * phase_neg.sin() * spicey_multiplier,
            ];
        }
    }
    out
}

pub struct SpectrumFrame {
    pub height: Vec<[f32; 2]>,
    pub disp_x: Vec<[f32; 2]>,
    pub disp_z: Vec<[f32; 2]>,
}

pub fn spectrum_update(
    init: &[[f32; 4]],
    size: usize,
    lod: usize,
    world_base: f32,
    time: f32,
    gravity: f32,
    period: f32,
    chop: f32,
) -> SpectrumFrame {
    let center = size / 2;
    let world_size = world_base * (1u32 << lod) as f32;
    let mut height = vec![[0.0f32; 2]; size * size];
    let mut disp_x = vec![[0.0f32; 2]; size * size];
    let mut disp_z = vec![[0.0f32; 2]; size * size];
    for y in 0..size {
        for x in 0..size {
            let coord_x = x as i32 - center as i32;
            let coord_y = y as i32 - center as i32;
            let k = [
                PI2 * coord_x as f32 / world_size,
                PI2 * coord_y as f32 / world_size,
            ];
            let k_mag = (k[0] * k[0] + k[1] * k[1]).sqrt();
            let (w, _dwdk) = deep_dispersion(k_mag, gravity, period);

            let sw = (w * time).sin();
            let cw = (w * time).cos();
            let fwd = [cw, -sw];
            let bkwd = [cw, sw];

            let h0 = init[y * size + x];
            let h = [
                h0[0] * fwd[0] - h0[1] * fwd[1] + h0[2] * bkwd[0] - h0[3] * bkwd[1],
                h0[0] * fwd[1] + h0[1] * fwd[0] + h0[2] * bkwd[1] + h0[3] * bkwd[0],
            ];

            let i = y * size + x;
            height[i] = h;
            disp_x[i] = [
                chop * (-h[1] * k[0]) / (k_mag + 0.00001),
                chop * (h[0] * k[0]) / (k_mag + 0.00001),
            ];
            disp_z[i] = [
                chop * (-h[1] * k[1]) / (k_mag + 0.00001),
                chop * (h[0] * k[1]) / (k_mag + 0.00001),
            ];
        }
    }
    SpectrumFrame {
        height,
        disp_x,
        disp_z,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GerstnerSpectrumComponent {
    pub wavelength: f32,
    pub amplitude: f32,
    pub direction: Vec2,
    pub speed: f32,
    pub phase: f32,
    pub chop_amplitude: f32,
}

fn gerstner_get_amplitude(
    wavelength: f32,
    components_per_octave: f32,
    wind_speed: f32,
    gravity: f32,
    power_log: &[f32; 14],
) -> f32 {
    if gravity <= 0.0 || wind_speed <= 0.0 {
        return 0.0;
    }
    let mut wl_pow2 = wavelength.log2();
    wl_pow2 = wl_pow2.clamp(SPECTRUM_SMALLEST_WL_POW_2, SPECTRUM_SMALLEST_WL_POW_2 + 13.0);
    let lower = 2.0f32.powf(wl_pow2.floor());
    let index = (wl_pow2 - SPECTRUM_SMALLEST_WL_POW_2) as usize;
    let index = index.min(13);
    let this_power = power_log[index];
    let next_index = index + 1;
    let has_next = next_index < 14;
    let next_power = if has_next { power_log[next_index] } else { MIN_POWER_LOG };

    let wl_lo = 2.0f32.powf(wl_pow2.floor());
    let k_lo = PI2 / wl_lo;
    let c_lo = (gravity / k_lo).sqrt();
    let omega_lo = k_lo * c_lo;
    let wl_hi = 2.0 * wl_lo;
    let k_hi = PI2 / wl_hi;
    let c_hi = (gravity / k_hi).sqrt();
    let omega_hi = k_hi * c_hi;
    let d_omega = (omega_lo - omega_hi) / components_per_octave;

    let alpha = (wavelength - lower) / lower;
    let mut power = if has_next {
        this_power.lerp(next_power, alpha)
    } else {
        this_power
    };
    power = 10.0f32.powf(power);

    let wm = 0.87 * gravity / wind_speed;
    let w = (gravity * PI2 / wavelength).sqrt();
    power *= (-1.291 * (wm / w).powi(4)).exp();

    let a_2 = 2.0 * power * d_omega;
    let mut a = a_2.sqrt();
    a *= 5.0;
    a
}

pub fn gerstner_amplitudes(
    wind_speed: f32,
    gravity: f32,
    chop: f32,
    power_log: &[f32; 14],
) -> Vec<GerstnerSpectrumComponent> {
    const COMPONENTS_PER_OCTAVE: usize = 4;
    const WAVE_DIRECTION_VARIANCE: f32 = 90.0;
    const OCTAVE_COUNT: usize = 14;
    const DOMINANT_COUNT: usize = 16;

    let total = OCTAVE_COUNT * COMPONENTS_PER_OCTAVE;
    let mut wavelengths = vec![0.0f32; total];
    let mut angle_degs = vec![0.0f32; total];
    let mut rng = Rng::new(0);

    let mut min_wavelength = 2.0f32.powf(SPECTRUM_SMALLEST_WL_POW_2);
    let inv_cpo = 1.0 / COMPONENTS_PER_OCTAVE as f32;
    for octave in 0..OCTAVE_COUNT {
        for i in 0..COMPONENTS_PER_OCTAVE {
            let index = octave * COMPONENTS_PER_OCTAVE + i;
            let min_i = min_wavelength + inv_cpo * min_wavelength * i as f32;
            let max_i = (min_i + inv_cpo * min_wavelength).min(2.0 * min_wavelength);
            wavelengths[index] = min_i.lerp(max_i, rng.rand_f32());
            let rnd = (i as f32 + rng.rand_f32()) * inv_cpo;
            angle_degs[index] = (2.0 * rnd - 1.0) * WAVE_DIRECTION_VARIANCE;
        }
        min_wavelength *= 2.0;
    }

    let mut amplitudes = vec![0.0f32; total];
    for index in 0..total {
        amplitudes[index] = rng.rand_f32()
            * gerstner_get_amplitude(
                wavelengths[index],
                COMPONENTS_PER_OCTAVE as f32,
                wind_speed,
                gravity,
                power_log,
            );
    }

    let mut phases = vec![0.0f32; total];
    for octave in 0..OCTAVE_COUNT {
        for i in 0..COMPONENTS_PER_OCTAVE {
            let index = octave * COMPONENTS_PER_OCTAVE + i;
            let rnd = (i as f32 + rng.rand_f32()) / COMPONENTS_PER_OCTAVE as f32;
            phases[index] = PI2 * rnd;
        }
    }

    let mut components: Vec<GerstnerSpectrumComponent> = (0..total)
        .map(|index| {
            let wavelength = wavelengths[index];
            let angle = angle_degs[index].to_radians();
            GerstnerSpectrumComponent {
                wavelength,
                amplitude: amplitudes[index],
                direction: Vec2::new(angle.cos(), angle.sin()),
                speed: (wavelength * gravity / PI2).sqrt(),
                phase: phases[index],
                chop_amplitude: -chop * amplitudes[index],
            }
        })
        .collect();

    components.sort_by(|a, b| b.amplitude.total_cmp(&a.amplitude));
    components.truncate(DOMINANT_COUNT);
    components
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wang_hash_matches_hand_traced_values() {
        assert_eq!(wang_hash(0), 3232319850);
        assert_eq!(wang_hash(1), 663891101);
        assert_eq!(wang_hash(42), 1462734105);
        assert_eq!(wang_hash(123), 1144277844);
    }

    #[test]
    fn xorshift_matches_hand_traced_values() {
        let mut rng = Rng { state: 1 };
        assert_eq!(rng.rand_u32(), 270369);
        assert_eq!(rng.rand_u32(), 67634689);
        assert_eq!(rng.rand_u32(), 2647435461);
    }

    #[test]
    fn rng_from_seed_matches_hand_traced_sequence() {
        let mut rng = Rng::new(7);
        assert_eq!(rng.rand_u32(), 3501121479);
        assert_eq!(rng.rand_u32(), 1636923372);
        assert_eq!(rng.rand_u32(), 2734086938);
    }

    #[test]
    fn rand_f32_and_rand_gauss_match_reference_values() {
        let mut rng = Rng::new(7);
        assert!((rng.rand_f32() - 0.8151684).abs() < 1e-6);
        assert!((rng.rand_f32() - 0.38112592).abs() < 1e-6);
        let mut rng = Rng::new(7);
        assert!((rng.rand_gauss() - -0.4691235).abs() < 1e-6);
        assert!((rng.rand_gauss() - -0.2564173).abs() < 1e-6);
    }

    #[test]
    fn deep_dispersion_snaps_period_to_integer_loops() {
        let gravity = 9.81;
        let period = 32.0;
        let k = 1.7;
        let (w, _dwdk) = deep_dispersion(k, gravity, period);
        let wave_period = PI2 / w;
        let loops = period / wave_period;
        assert!((loops - loops.round()).abs() < 1e-4);
        assert!(loops >= 1.0);
    }

    #[test]
    fn deep_dispersion_without_period_is_sqrt_gk() {
        let (w, dwdk) = deep_dispersion(2.0, 9.81, -1.0);
        assert!((w - (9.81f32 * 2.0).sqrt()).abs() < 1e-6);
        assert!((dwdk - 9.81 / (2.0 * w)).abs() < 1e-6);
    }

    #[test]
    fn spectrum_initialize_is_deterministic() {
        let a = spectrum_initialize(
            16, 3, 7, 4.0, Vec2::X, 4.0, 9.81, 32.0, 0.4, 1.6, &POWER_LOG,
        );
        let b = spectrum_initialize(
            16, 3, 7, 4.0, Vec2::X, 4.0, 9.81, 32.0, 0.4, 1.6, &POWER_LOG,
        );
        assert_eq!(a, b);
        assert!(a.iter().any(|v| v.iter().any(|c| c.abs() > 0.0)));
    }

    #[test]
    fn spectrum_initialize_is_zero_at_dc() {
        let size = 16;
        let data = spectrum_initialize(
            size, 3, 7, 4.0, Vec2::X, 4.0, 9.81, 32.0, 0.4, 1.6, &POWER_LOG,
        );
        let center = size / 2;
        assert_eq!(data[center * size + center], [0.0; 4]);
    }

    #[test]
    fn spectrum_initialize_octave_splitting_zeroes_out_of_band_texels() {
        let size = 64;
        let data = spectrum_initialize(
            size, 0, 7, 4.0, Vec2::X, 4.0, 9.81, 32.0, 0.4, 1.6, &POWER_LOG,
        );
        let center = size / 2;
        for y in 0..size {
            for x in 0..size {
                let cx = (x as i32 - center as i32).abs();
                let cy = (y as i32 - center as i32).abs();
                let max_coord = cx.max(cy);
                let out_of_band = !(4..8).contains(&max_coord);
                if out_of_band {
                    assert_eq!(data[y * size + x], [0.0; 4]);
                }
            }
        }
        let in_band = (0..size).any(|y| {
            (0..size).any(|x| {
                let cx = (x as i32 - center as i32).abs();
                let cy = (y as i32 - center as i32).abs();
                let max_coord = cx.max(cy);
                (4..8).contains(&max_coord) && data[y * size + x] != [0.0; 4]
            })
        });
        assert!(in_band);
    }

    #[test]
    fn spectrum_initialize_max_cascade_has_no_band_restriction() {
        let size = 64;
        let data = spectrum_initialize(
            size, 6, 7, 4.0, Vec2::X, 4.0, 9.81, 32.0, 0.4, 1.6, &POWER_LOG,
        );
        let center = size / 2;
        let far = data[(center + 20) * size + center];
        assert_ne!(far, [0.0; 4]);
    }

    #[test]
    fn spectrum_update_advances_time_deterministically() {
        let init = spectrum_initialize(
            16, 3, 7, 4.0, Vec2::X, 4.0, 9.81, 32.0, 0.4, 1.6, &POWER_LOG,
        );
        let a = spectrum_update(&init, 16, 3, 4.0, 1.5, 9.81, 32.0, 1.6);
        let b = spectrum_update(&init, 16, 3, 4.0, 1.5, 9.81, 32.0, 1.6);
        assert_eq!(a.height, b.height);
        assert_eq!(a.disp_x, b.disp_x);
        assert_eq!(a.disp_z, b.disp_z);
        let c = spectrum_update(&init, 16, 3, 4.0, 3.0, 9.81, 32.0, 1.6);
        assert_ne!(a.height, c.height);
    }

    #[test]
    fn gerstner_amplitudes_are_deterministic_and_bounded() {
        let a = gerstner_amplitudes(4.0, 9.81, 1.6, &POWER_LOG);
        let b = gerstner_amplitudes(4.0, 9.81, 1.6, &POWER_LOG);
        assert_eq!(a.len(), 16);
        assert_eq!(a, b);
        assert!(a.iter().all(|c| c.amplitude > 0.0));
        assert!(a.windows(2).all(|w| w[0].amplitude >= w[1].amplitude));
        assert!(a.iter().all(|c| c.wavelength > 0.0 && c.speed > 0.0));
    }
}
