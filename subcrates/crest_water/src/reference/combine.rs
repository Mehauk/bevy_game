use bevy::prelude::*;

use super::Seabed;
use super::fft::FftBaked;
use super::gerstner::GerstnerSurface;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimatedWaveSample {
    pub height: f32,
    pub displacement: Vec3,
    pub normal: Vec3,
    pub jacobian: f32,
}

fn texel_indices(size: usize, u: f32, v: f32) -> (usize, usize, usize, usize, f32, f32) {
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
    (
        v0 * size + u0,
        v0 * size + u1,
        v1 * size + u0,
        v1 * size + u1,
        ut - ut.floor(),
        vt - vt.floor(),
    )
}

fn lerp4(a: [f32; 4], b: [f32; 4], c: [f32; 4], d: [f32; 4], fu: f32, fv: f32) -> [f32; 4] {
    [
        a[0].lerp(b[0], fu).lerp(c[0].lerp(d[0], fu), fv),
        a[1].lerp(b[1], fu).lerp(c[1].lerp(d[1], fu), fv),
        a[2].lerp(b[2], fu).lerp(c[2].lerp(d[2], fu), fv),
        a[3].lerp(b[3], fu).lerp(c[3].lerp(d[3], fu), fv),
    ]
}

fn lerp2(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2], fu: f32, fv: f32) -> [f32; 2] {
    [
        a[0].lerp(b[0], fu).lerp(c[0].lerp(d[0], fu), fv),
        a[1].lerp(b[1], fu).lerp(c[1].lerp(d[1], fu), fv),
    ]
}

#[allow(clippy::too_many_arguments)]
pub fn animated_wave_height(
    baked: &FftBaked,
    size: usize,
    lod_texel: f32,
    lod_origin: (f32, f32),
    world_x: f32,
    world_z: f32,
    water_level: f32,
    seabed: &Seabed,
    attenuation_in_shallows: f32,
    shallows_max_depth: f32,
    gerstner: &GerstnerSurface,
) -> AnimatedWaveSample {
    let extent = lod_texel * size as f32;
    let u = (world_x - lod_origin.0) / extent;
    let v = (world_z - lod_origin.1) / extent;
    let (i00, i10, i01, i11, fu, fv) = texel_indices(size, u, v);

    let disp = lerp4(
        baked.disp[i00],
        baked.disp[i10],
        baked.disp[i01],
        baked.disp[i11],
        fu,
        fv,
    );
    let derivs = lerp2(
        baked.derivs[i00],
        baked.derivs[i10],
        baked.derivs[i01],
        baked.derivs[i11],
        fu,
        fv,
    );

    let depth = water_level - seabed.height(world_x, world_z);
    let weight = if shallows_max_depth > 0.0 {
        (depth / shallows_max_depth).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let attenuation = attenuation_in_shallows * weight + (1.0 - attenuation_in_shallows);

    let fft_disp = Vec3::new(disp[0], disp[1], disp[2]) * attenuation;
    let displacement = fft_disp + gerstner.displacement;
    let height = water_level + displacement.y;

    let fft_normal = Vec3::new(-derivs[0], 1.0, -derivs[1]).normalize_or_zero();
    let normal = (fft_normal + (gerstner.normal - Vec3::Y)).normalize_or_zero();

    AnimatedWaveSample {
        height,
        displacement,
        normal,
        jacobian: disp[3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::fft::bake_frame;
    use crate::reference::gerstner::{GerstnerWave, gerstner_surface};

    fn flat_baked(size: usize) -> FftBaked {
        let zero = vec![[0.0f32; 2]; size * size];
        bake_frame(&zero, &zero, &zero, size, 1.0)
    }

    #[test]
    fn zero_baked_flat_gerstner_returns_water_level() {
        let baked = flat_baked(8);
        let gerstner = gerstner_surface(&[], 1.0, 2.0, 0.0);
        let sample = animated_wave_height(
            &baked,
            8,
            1.0,
            (0.0, 0.0),
            3.0,
            4.0,
            0.0,
            &Seabed::Flat { height: -50.0 },
            0.95,
            1000.0,
            &gerstner,
        );
        assert!((sample.height - 0.0).abs() < 1e-6);
        assert!((sample.normal.y - 1.0).abs() < 1e-5);
        assert!((sample.jacobian - 1.0).abs() < 1e-5);
    }

    #[test]
    fn gerstner_raises_height() {
        let baked = flat_baked(8);
        let waves = [GerstnerWave {
            wavelength: 4.0,
            amplitude: 0.5,
            direction: Vec2::X,
            speed: 2.0,
            steepness: 0.0,
            phase: 0.0,
        }];
        let gerstner = gerstner_surface(&waves, 0.0, 0.0, 0.0);
        let sample = animated_wave_height(
            &baked,
            8,
            1.0,
            (0.0, 0.0),
            0.0,
            0.0,
            0.0,
            &Seabed::Flat { height: -50.0 },
            0.95,
            1000.0,
            &gerstner,
        );
        assert!((sample.height - 0.5).abs() < 1e-5);
    }

    #[test]
    fn shallow_water_attenuates_fft_displacement() {
        let size = 8;
        let mut height = vec![[0.0f32; 2]; size * size];
        height[0] = [3.0, 0.0];
        let zero = vec![[0.0f32; 2]; size * size];
        let baked = bake_frame(&height, &zero, &zero, size, 1.0);
        let gerstner = gerstner_surface(&[], 0.0, 0.0, 0.0);
        let deep = animated_wave_height(
            &baked,
            size,
            1.0,
            (0.0, 0.0),
            0.5,
            0.5,
            0.0,
            &Seabed::Flat { height: -500.0 },
            0.95,
            1000.0,
            &gerstner,
        );
        let shallow = animated_wave_height(
            &baked,
            size,
            1.0,
            (0.0, 0.0),
            0.5,
            0.5,
            0.0,
            &Seabed::Flat { height: -1.0 },
            0.95,
            1000.0,
            &gerstner,
        );
        assert!(shallow.displacement.length() < deep.displacement.length());
    }

    #[test]
    fn tiled_sampling_is_periodic() {
        let size = 8;
        let mut height = vec![[0.0f32; 2]; size * size];
        for (i, cell) in height.iter_mut().enumerate() {
            *cell = [((i * 3) % 7) as f32, 0.0];
        }
        let zero = vec![[0.0f32; 2]; size * size];
        let baked = bake_frame(&height, &zero, &zero, size, 1.0);
        let gerstner = gerstner_surface(&[], 0.0, 0.0, 0.0);
        let extent = 1.0 * size as f32;
        let a = animated_wave_height(
            &baked, size, 1.0, (0.0, 0.0), 2.3, 5.1, 0.0,
            &Seabed::Flat { height: -50.0 }, 0.95, 1000.0, &gerstner,
        );
        let b = animated_wave_height(
            &baked, size, 1.0, (0.0, 0.0), 2.3 + extent, 5.1 - extent, 0.0,
            &Seabed::Flat { height: -50.0 }, 0.95, 1000.0, &gerstner,
        );
        assert!((a.height - b.height).abs() < 1e-4);
    }
}
