use bevy::prelude::*;

const PI2: f32 = std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GerstnerWave {
    pub wavelength: f32,
    pub amplitude: f32,
    pub direction: Vec2,
    pub speed: f32,
    pub steepness: f32,
    pub phase: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GerstnerSurface {
    pub displacement: Vec3,
    pub normal: Vec3,
    pub jacobian: f32,
}

fn wave_q(wave: &GerstnerWave, k: f32, count: usize) -> f32 {
    if wave.amplitude <= 0.0 || k <= 0.0 || count == 0 {
        return 0.0;
    }
    wave.steepness.clamp(0.0, 1.0) / (k * wave.amplitude * count as f32)
}

pub fn gerstner_surface(waves: &[GerstnerWave], x: f32, z: f32, t: f32) -> GerstnerSurface {
    let mut displacement = Vec3::ZERO;
    let mut delfdelx = Vec3::new(1.0, 0.0, 0.0);
    let mut delfdelz = Vec3::new(0.0, 0.0, 1.0);
    let mut jac_a = 0.0f32;
    let mut jac_b = 0.0f32;
    let mut jac_c = 0.0f32;

    let count = waves.len();
    for wave in waves {
        if wave.amplitude <= 0.0 || wave.wavelength <= 0.0 {
            continue;
        }
        let k = PI2 / wave.wavelength;
        let dir = if wave.direction.length_squared() > 0.0 {
            wave.direction.normalize()
        } else {
            Vec2::X
        };
        let omega = k * wave.speed;
        let angle = k * (dir.x * x + dir.y * z) - omega * t + wave.phase;
        let (sin_angle, cos_angle) = angle.sin_cos();
        let q = wave_q(wave, k, count);

        displacement += Vec3::new(
            -q * wave.amplitude * dir.x * sin_angle,
            wave.amplitude * cos_angle,
            -q * wave.amplitude * dir.y * sin_angle,
        );

        let d_cos = k * -q * cos_angle;
        let disp_x = dir.x * d_cos;
        let disp_z = dir.y * d_cos;
        let disp_y = -k * sin_angle;
        delfdelx += wave.amplitude * Vec3::new(dir.x * disp_x, dir.x * disp_y, dir.y * disp_x);
        delfdelz += wave.amplitude * Vec3::new(dir.x * disp_z, dir.y * disp_y, dir.y * disp_z);

        let qa = q * k * wave.amplitude * cos_angle;
        jac_a += qa * dir.x * dir.x;
        jac_b += qa * dir.x * dir.y;
        jac_c += qa * dir.y * dir.y;
    }

    let normal = delfdelz.cross(delfdelx).normalize_or_zero();
    let jacobian = (1.0 - jac_a) * (1.0 - jac_c) - jac_b * jac_b;

    GerstnerSurface {
        displacement,
        normal,
        jacobian,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_wave_crest_and_trough_match_hand_computed() {
        let wave = GerstnerWave {
            wavelength: 2.0 * PI2,
            amplitude: 1.0,
            direction: Vec2::X,
            speed: 2.0,
            steepness: 0.0,
            phase: 0.0,
        };
        let k = PI2 / wave.wavelength;
        let t = 0.0;
        let crest = gerstner_surface(&[wave], 0.0, 0.0, t);
        assert!((crest.displacement.y - 1.0).abs() < 1e-6);
        assert!(crest.displacement.x.abs() < 1e-6);
        assert!((crest.jacobian - 1.0).abs() < 1e-6);

        let trough_x = PI2 * 0.5 / k;
        let trough = gerstner_surface(&[wave], trough_x, 0.0, t);
        assert!((trough.displacement.y + 1.0).abs() < 1e-5);
        assert!((trough.jacobian - 1.0).abs() < 1e-5);
    }

    #[test]
    fn choppy_wave_crest_displaces_horizontally_and_jacobian_dips() {
        let wave = GerstnerWave {
            wavelength: 4.0,
            amplitude: 0.5,
            direction: Vec2::X,
            speed: 3.0,
            steepness: 1.0,
            phase: 0.0,
        };
        let k = PI2 / 4.0;
        let q = 1.0 / (k * 0.5);
        let crest = gerstner_surface(&[wave], 0.0, 0.0, 0.0);
        assert!((crest.displacement.y - 0.5).abs() < 1e-5);
        assert!((crest.jacobian - (1.0 - q * k * 0.5)).abs() < 1e-4);
        assert!(crest.jacobian >= -1e-6);
    }

    #[test]
    fn steepness_clamp_keeps_jacobian_non_negative() {
        let waves = [
            GerstnerWave {
                wavelength: 7.0,
                amplitude: 0.8,
                direction: Vec2::X,
                speed: 3.0,
                steepness: 1.0,
                phase: 0.0,
            },
            GerstnerWave {
                wavelength: 3.0,
                amplitude: 0.6,
                direction: Vec2::new(0.6, 0.8),
                speed: 2.0,
                steepness: 1.0,
                phase: 0.0,
            },
            GerstnerWave {
                wavelength: 1.5,
                amplitude: 0.4,
                direction: Vec2::new(-0.3, 0.95),
                speed: 1.5,
                steepness: 1.0,
                phase: 0.0,
            },
        ];
        for t in 0..64 {
            let time = t as f32 * 0.13;
            for i in 0..64 {
                let x = i as f32 * 0.37;
                let z = (i as f32 * 0.71).sin() * 5.0;
                let s = gerstner_surface(&waves, x, z, time);
                assert!(s.jacobian >= -1e-5, "jacobian {} went negative", s.jacobian);
            }
        }
    }

    #[test]
    fn normal_is_up_for_flat_conditions() {
        let wave = GerstnerWave {
            wavelength: 10.0,
            amplitude: 0.5,
            direction: Vec2::X,
            speed: 2.0,
            steepness: 0.0,
            phase: 0.0,
        };
        let s = gerstner_surface(&[wave], 0.0, 0.0, 0.0);
        assert!((s.normal.y - 1.0).abs() < 1e-5);
        assert!(s.normal.x.abs() < 1e-5);
    }

    #[test]
    fn empty_waves_gives_flat_surface() {
        let s = gerstner_surface(&[], 3.0, 4.0, 5.0);
        assert_eq!(s.displacement, Vec3::ZERO);
        assert!((s.normal - Vec3::Y).length() < 1e-6);
        assert!((s.jacobian - 1.0).abs() < 1e-6);
    }
}
