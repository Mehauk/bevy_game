use bevy::prelude::*;

pub fn sample_bilinear(data: &[f32], size: usize, u: f32, v: f32) -> f32 {
    let mut ut = (u - u.floor()) * size as f32 - 0.5;
    let mut vt = (v - v.floor()) * size as f32 - 0.5;
    if ut < 0.0 {
        ut += size as f32;
    }
    if vt < 0.0 {
        vt += size as f32;
    }
    let u0 = ut.floor() as usize % size;
    let v0 = vt.floor() as usize % size;
    let u1 = (u0 + 1) % size;
    let v1 = (v0 + 1) % size;
    let fu = ut - ut.floor();
    let fv = vt - vt.floor();
    let h00 = data[v0 * size + u0];
    let h10 = data[v0 * size + u1];
    let h01 = data[v1 * size + u0];
    let h11 = data[v1 * size + u1];
    h00.lerp(h10, fu).lerp(h01.lerp(h11, fu), fv)
}

pub fn fpi_surface_height(
    height_at: impl Fn(f32, f32) -> f32,
    ray_origin: Vec3,
    ray_dir: Vec3,
    max_distance: f32,
) -> Option<f32> {
    if ray_dir.y >= -1e-6 || max_distance <= 0.0 {
        return None;
    }
    let surface_offset = |t: f32| {
        let x = ray_origin.x + ray_dir.x * t;
        let z = ray_origin.z + ray_dir.z * t;
        ray_origin.y + ray_dir.y * t - height_at(x, z)
    };

    let steps = 64;
    let step = max_distance / steps as f32;
    if surface_offset(0.0) <= 0.0 {
        return Some(0.0);
    }
    let mut t_lo = 0.0f32;
    let mut t_hi = 0.0f32;
    let mut found = false;
    for i in 1..=steps {
        let t = i as f32 * step;
        if surface_offset(t) <= 0.0 {
            t_lo = t - step;
            t_hi = t;
            found = true;
            break;
        }
    }
    if !found {
        return None;
    }
    let f_lo = surface_offset(t_lo);
    let f_hi = surface_offset(t_hi);
    let mut t = if (f_lo - f_hi).abs() > 1e-12 {
        t_lo + step * (f_lo / (f_lo - f_hi))
    } else {
        (t_lo + t_hi) * 0.5
    };

    for _ in 0..32 {
        let x = ray_origin.x + ray_dir.x * t;
        let z = ray_origin.z + ray_dir.z * t;
        let f = ray_origin.y + ray_dir.y * t - height_at(x, z);
        if f > 0.0 {
            t_lo = t;
        } else {
            t_hi = t;
        }
        let proposed = (height_at(x, z) - ray_origin.y) / ray_dir.y;
        let next = if proposed.is_finite() && proposed > t_lo && proposed < t_hi {
            proposed
        } else {
            (t_lo + t_hi) * 0.5
        };
        let done = (next - t).abs() < 1e-4;
        t = next;
        if done {
            break;
        }
    }
    if !t.is_finite() || t < 0.0 || t > max_distance {
        return None;
    }
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::gerstner::{GerstnerWave, gerstner_surface};

    #[test]
    fn bilinear_returns_texel_values_at_centers() {
        let size = 4;
        let data: Vec<f32> = (0..16).map(|i| i as f32).collect();
        assert!((sample_bilinear(&data, size, 0.125, 0.125) - 0.0).abs() < 1e-6);
        assert!((sample_bilinear(&data, size, 0.375, 0.125) - 1.0).abs() < 1e-6);
        assert!((sample_bilinear(&data, size, 0.125, 0.375) - 4.0).abs() < 1e-6);
    }

    #[test]
    fn bilinear_interpolates_between_texels() {
        let size = 2;
        let data = vec![0.0, 10.0, 0.0, 0.0];
        let mid = sample_bilinear(&data, size, 0.5, 0.25);
        assert!((mid - 5.0).abs() < 1e-5);
    }

    #[test]
    fn bilinear_wraps_out_of_range_uv() {
        let size = 4;
        let data: Vec<f32> = (0..16).map(|i| i as f32).collect();
        let a = sample_bilinear(&data, size, 0.125, 0.125);
        let b = sample_bilinear(&data, size, 1.125, -0.875);
        assert!((a - b).abs() < 1e-5);
    }

    #[test]
    fn fpi_hits_flat_surface() {
        let height = |_x: f32, _z: f32| 2.0f32;
        let origin = Vec3::new(1.0, 10.0, -3.0);
        let dir = Vec3::new(0.2, -1.0, 0.1).normalize();
        let t = fpi_surface_height(height, origin, dir, 100.0).expect("hit");
        let expected = (2.0 - 10.0) / dir.y;
        assert!((t - expected).abs() < 1e-3);
    }

    #[test]
    fn fpi_hits_gerstner_surface() {
        let waves = [
            GerstnerWave {
                wavelength: 6.0,
                amplitude: 0.7,
                direction: Vec2::X,
                speed: 3.0,
                steepness: 0.6,
                phase: 0.0,
            },
            GerstnerWave {
                wavelength: 3.0,
                amplitude: 0.4,
                direction: Vec2::new(0.4, 0.9),
                speed: 2.0,
                steepness: 0.6,
                phase: 0.0,
            },
        ];
        let height = |x: f32, z: f32| gerstner_surface(&waves, x, z, 1.25).displacement.y;
        let origin = Vec3::new(2.0, 8.0, -1.0);
        let dir = Vec3::new(0.1, -1.0, 0.05).normalize();
        let t = fpi_surface_height(height, origin, dir, 50.0).expect("hit");
        let residual = (origin.y + dir.y * t - height(origin.x + dir.x * t, origin.z + dir.z * t))
            .abs();
        assert!(residual < 1e-3, "residual {residual}");
    }

    #[test]
    fn fpi_misses_when_ray_points_up() {
        let height = |_x: f32, _z: f32| 0.0f32;
        assert!(fpi_surface_height(height, Vec3::new(0.0, 5.0, 0.0), Vec3::Y, 100.0).is_none());
    }

    #[test]
    fn fpi_misses_when_surface_out_of_range() {
        let height = |_x: f32, _z: f32| 0.0f32;
        let origin = Vec3::new(0.0, 10.0, 0.0);
        let dir = Vec3::new(0.0, -1.0, 0.0);
        assert!(fpi_surface_height(height, origin, dir, 5.0).is_none());
    }
}
