use std::f32::consts::TAU;
use std::sync::mpsc::Receiver;
use std::sync::Mutex;

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};

pub const FFT_FRAMES: u32 = 64;
pub const FFT_RESOLUTION: u32 = 128;
pub const SWELL_TILE_SIZE: f32 = 120.0;
pub const SWELL_LOOP_PERIOD: f32 = 12.0;
pub const CHOP_TILE_SIZE: f32 = 37.0;
pub const CHOP_LOOP_PERIOD: f32 = 8.0;
pub const SWELL_FADE: (f32, f32) = (250.0, 500.0);
pub const CHOP_FADE: (f32, f32) = (40.0, 80.0);

const GRAVITY: f32 = 9.81;
const CPU_SWELL_RESOLUTION: u32 = 64;

#[derive(Clone, Copy, Debug)]
pub struct FftBandConfig {
    pub tile_size: f32,
    pub resolution: u32,
    pub frames: u32,
    pub loop_period: f32,
    pub wind_speed: f32,
    pub wind_direction: Vec2,
    pub fetch: f32,
    pub directional_spread: f32,
    pub min_wavelength: f32,
    pub max_wavelength: f32,
    pub choppiness: f32,
    pub amplitude: f32,
    pub seed: u32,
}

impl FftBandConfig {
    pub const SWELL: Self = Self {
        tile_size: SWELL_TILE_SIZE,
        resolution: FFT_RESOLUTION,
        frames: FFT_FRAMES,
        loop_period: SWELL_LOOP_PERIOD,
        wind_speed: 12.0,
        wind_direction: Vec2::new(0.94, 0.34),
        fetch: 4000.0,
        directional_spread: 3.0,
        min_wavelength: 20.0,
        max_wavelength: 120.0,
        choppiness: 0.9,
        amplitude: 1.0,
        seed: 0x5EA0_1234,
    };

    pub const CHOP: Self = Self {
        tile_size: CHOP_TILE_SIZE,
        resolution: FFT_RESOLUTION,
        frames: FFT_FRAMES,
        loop_period: CHOP_LOOP_PERIOD,
        wind_speed: 9.0,
        wind_direction: Vec2::new(-0.37, 0.93),
        fetch: 600.0,
        directional_spread: 6.0,
        min_wavelength: 0.6,
        max_wavelength: 20.0,
        choppiness: 1.4,
        amplitude: 1.0,
        seed: 0x9F3C_7712,
    };
}

pub fn quantized_omega(wavenumber: f32, loop_period: f32) -> f32 {
    let delta = TAU / loop_period;
    let omega = (GRAVITY * wavenumber).sqrt();
    (omega / delta).round().max(1.0) * delta
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn wave_frequency(index: usize, resolution: usize) -> f32 {
    let half = resolution / 2;
    if index <= half {
        index as f32
    } else {
        index as f32 - resolution as f32
    }
}

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

fn gaussian_pair(ix: u32, iz: u32, seed: u32) -> [f32; 2] {
    let u1 = ((hash_u32(ix.wrapping_mul(0x9E37_79B9) ^ iz ^ seed) >> 8) as f32 + 1.0) / 16_777_217.0;
    let u2 = (hash_u32(iz.wrapping_mul(0x85EB_Ca6B) ^ ix ^ seed.rotate_left(15)) >> 8) as f32
        / 16_777_216.0;
    let radius = (-2.0 * u1.ln()).sqrt();
    let theta = TAU * u2;
    [radius * theta.cos(), radius * theta.sin()]
}

fn phillips_density(wavenumber: f32, direction: Vec2, config: &FftBandConfig) -> f32 {
    if wavenumber < 1.0e-4 {
        return 0.0;
    }
    let wind = config.wind_direction.normalize_or_zero();
    let outer = (GRAVITY / (config.wind_speed * config.wind_speed).max(0.5)).max(TAU / config.fetch);
    let inner = TAU / (config.min_wavelength * 0.5);
    let inv_k2 = 1.0 / (wavenumber * wavenumber);
    let mut density = 0.008
        * inv_k2
        * inv_k2
        * (-(outer * outer) * inv_k2).exp()
        * (-(wavenumber * wavenumber) / (inner * inner)).exp();
    let along = if wind.length_squared() > 0.0 {
        direction.dot(wind) * inv_k2.sqrt()
    } else {
        1.0
    };
    let spread = ((1.0 + along.clamp(-1.0, 1.0)) * 0.5).powf(config.directional_spread * 0.5);
    density *= spread * spread;
    let wavelength = TAU / wavenumber;
    let band = smoothstep(
        config.min_wavelength * 0.6,
        config.min_wavelength * 1.4,
        wavelength,
    ) * (1.0
        - smoothstep(
            config.max_wavelength * 0.6,
            config.max_wavelength * 1.4,
            wavelength,
        ));
    density * band
}

fn scale_c(scale: f32, value: (f32, f32)) -> (f32, f32) {
    (scale * value.0, scale * value.1)
}

fn pack(real: (f32, f32), imag: (f32, f32)) -> (f32, f32) {
    (real.0 - imag.1, real.1 + imag.0)
}

fn twiddles(resolution: usize) -> (Vec<f32>, Vec<f32>) {
    let mut re = vec![0.0f32; resolution / 2];
    let mut im = vec![0.0f32; resolution / 2];
    for (i, (r, i_out)) in re.iter_mut().zip(im.iter_mut()).enumerate() {
        let (s, c) = (TAU * i as f32 / resolution as f32).sin_cos();
        *r = c;
        *i_out = -s;
    }
    (re, im)
}

fn fft1d(re: &mut [f32], im: &mut [f32], inverse: bool, tw_re: &[f32], tw_im: &[f32]) {
    let n = re.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let half = len >> 1;
        let step = n / len;
        let mut start = 0;
        while start < n {
            for k in 0..half {
                let (wr, mut wi) = (tw_re[k * step], tw_im[k * step]);
                if inverse {
                    wi = -wi;
                }
                let a = start + k;
                let b = a + half;
                let xr = re[b] * wr - im[b] * wi;
                let xi = re[b] * wi + im[b] * wr;
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
            }
            start += len;
        }
        len <<= 1;
    }
    if inverse {
        let scale = 1.0 / n as f32;
        for value in re.iter_mut() {
            *value *= scale;
        }
        for value in im.iter_mut() {
            *value *= scale;
        }
    }
}

fn fft2(re: &mut [f32], im: &mut [f32], n: usize, inverse: bool, tw_re: &[f32], tw_im: &[f32], scratch_re: &mut [f32], scratch_im: &mut [f32]) {
    for row in 0..n {
        let start = row * n;
        fft1d(&mut re[start..start + n], &mut im[start..start + n], inverse, tw_re, tw_im);
    }
    for col in 0..n {
        for row in 0..n {
            scratch_re[row] = re[row * n + col];
            scratch_im[row] = im[row * n + col];
        }
        fft1d(scratch_re, scratch_im, inverse, tw_re, tw_im);
        for row in 0..n {
            re[row * n + col] = scratch_re[row];
            im[row * n + col] = scratch_im[row];
        }
    }
}

fn f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let fraction = bits & 0x007f_ffff;
    if exponent >= 0x1f {
        return sign | if fraction != 0 { 0x7e00 } else { 0x7c00 };
    }
    if exponent > 0 {
        let rounded = fraction + 0x0fff + ((fraction >> 13) & 1);
        if rounded & 0x0080_0000 != 0 {
            return sign | (((exponent + 1) as u16) << 10);
        }
        return sign | ((exponent as u16) << 10) | ((rounded >> 13) as u16);
    }
    if exponent < -10 {
        return sign;
    }
    let mantissa = fraction | 0x0080_0000;
    let shift = (14 - exponent) as u32;
    let rounded = (mantissa >> shift) + ((mantissa >> (shift - 1)) & 1);
    sign | (rounded as u16)
}

fn push_f16(bytes: &mut Vec<u8>, value: f32) {
    bytes.extend_from_slice(&f16_bits(value).to_le_bytes());
}

fn push_f16_vec4(bytes: &mut Vec<u8>, value: [f32; 4]) {
    for component in value {
        push_f16(bytes, component);
    }
}

fn downsample_mips(source: &[f32], width: usize, height: usize, bytes: &mut Vec<u8>) {
    let mut level: Vec<f32> = source.to_vec();
    let mut w = width;
    let mut h = height;
    loop {
        for texel in level.chunks_exact(4) {
            push_f16_vec4(bytes, [texel[0], texel[1], texel[2], texel[3]]);
        }
        if w == 1 && h == 1 {
            break;
        }
        let nw = (w / 2).max(1);
        let nh = (h / 2).max(1);
        let mut next = vec![0.0f32; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                let x0 = (x * 2).min(w - 1);
                let x1 = (x * 2 + 1).min(w - 1);
                let y0 = (y * 2).min(h - 1);
                let y1 = (y * 2 + 1).min(h - 1);
                let mut sum = [0.0f32; 4];
                for (sx, sy) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
                    let base = (sy * w + sx) * 4;
                    for c in 0..4 {
                        sum[c] += level[base + c];
                    }
                }
                let base = (y * nw + x) * 4;
                for c in 0..4 {
                    next[base + c] = sum[c] * 0.25;
                }
            }
        }
        level = next;
        w = nw;
        h = nh;
    }
}

#[derive(Clone, Debug)]
pub struct FftSwellField {
    pub resolution: u32,
    pub frames: u32,
    pub tile_size: f32,
    pub loop_period: f32,
    pub crest_span: f32,
    data: Vec<[f32; 3]>,
}

impl FftSwellField {
    fn texel(&self, frame: usize, x: usize, z: usize) -> [f32; 3] {
        let res = self.resolution as usize;
        self.data[(frame * res + z) * res + x]
    }

    pub fn offset_at(&self, x: f32, z: f32, time: f32) -> Vec3 {
        let res = self.resolution as usize;
        let frames = self.frames as usize;
        let u = (x / self.tile_size).rem_euclid(1.0) * res as f32 - 0.5;
        let v = (z / self.tile_size).rem_euclid(1.0) * res as f32 - 0.5;
        let x0 = u.floor();
        let z0 = v.floor();
        let fx = u - x0;
        let fz = v - z0;
        let ix0 = x0 as i64;
        let iz0 = z0 as i64;
        let wrap = |i: i64| (i.rem_euclid(res as i64)) as usize;
        let ft = (time / self.loop_period).rem_euclid(1.0) * frames as f32;
        let f0 = ft.floor();
        let ff = ft - f0;
        let i0 = f0 as usize % frames;
        let i1 = (i0 + 1) % frames;
        let sample = |frame: usize| -> [f32; 3] {
            let a = self.texel(frame, wrap(ix0), wrap(iz0));
            let b = self.texel(frame, wrap(ix0 + 1), wrap(iz0));
            let c = self.texel(frame, wrap(ix0), wrap(iz0 + 1));
            let d = self.texel(frame, wrap(ix0 + 1), wrap(iz0 + 1));
            let ab = [
                a[0] + (b[0] - a[0]) * fx,
                a[1] + (b[1] - a[1]) * fx,
                a[2] + (b[2] - a[2]) * fx,
            ];
            let cd = [
                c[0] + (d[0] - c[0]) * fx,
                c[1] + (d[1] - c[1]) * fx,
                c[2] + (d[2] - c[2]) * fx,
            ];
            [
                ab[0] + (cd[0] - ab[0]) * fz,
                ab[1] + (cd[1] - ab[1]) * fz,
                ab[2] + (cd[2] - ab[2]) * fz,
            ]
        };
        let a = sample(i0);
        let b = sample(i1);
        Vec3::new(
            a[0] + (b[0] - a[0]) * ff,
            a[1] + (b[1] - a[1]) * ff,
            a[2] + (b[2] - a[2]) * ff,
        )
    }

    pub fn height_at(&self, x: f32, z: f32, time: f32) -> f32 {
        let drift = self.offset_at(x, z, time);
        self.offset_at(x - drift.x, z - drift.z, time).y
    }
}

pub struct FftBakedBand {
    pub tile_size: f32,
    pub loop_period: f32,
    pub resolution: u32,
    pub frames: u32,
    pub mip_levels: u32,
    pub displacement: Vec<u8>,
    pub derivatives: Vec<u8>,
    pub swell_field: FftSwellField,
    pub hermitian_residual: f32,
}

pub fn bake_band(config: &FftBandConfig) -> FftBakedBand {
    let n = config.resolution as usize;
    let frames = config.frames as usize;
    let cells = n * n;
    let mut h0_re = vec![0.0f32; cells];
    let mut h0_im = vec![0.0f32; cells];
    for iz in 0..n {
        for ix in 0..n {
            let kx = TAU * wave_frequency(ix, n) / config.tile_size;
            let kz = TAU * wave_frequency(iz, n) / config.tile_size;
            let wavenumber = kx.hypot(kz);
            let density = phillips_density(wavenumber, Vec2::new(kx, kz), config);
            let amp = (density * 0.5).sqrt() * config.amplitude;
            let [g0, g1] = gaussian_pair(ix as u32, iz as u32, config.seed);
            let index = iz * n + ix;
            h0_re[index] = amp * g0;
            h0_im[index] = amp * g1;
        }
    }

    let (tw_re, tw_im) = twiddles(n);
    let mut scratch_re = vec![0.0f32; n];
    let mut scratch_im = vec![0.0f32; n];
    let mut fields = [
        vec![(0.0f32, 0.0f32); cells],
        vec![(0.0f32, 0.0f32); cells],
        vec![(0.0f32, 0.0f32); cells],
        vec![(0.0f32, 0.0f32); cells],
        vec![(0.0f32, 0.0f32); cells],
        vec![(0.0f32, 0.0f32); cells],
        vec![(0.0f32, 0.0f32); cells],
        vec![(0.0f32, 0.0f32); cells],
    ];
    let mut g0_re = vec![0.0f32; cells];
    let mut g0_im = vec![0.0f32; cells];
    let mut g1_re = vec![0.0f32; cells];
    let mut g1_im = vec![0.0f32; cells];
    let mut g2_re = vec![0.0f32; cells];
    let mut g2_im = vec![0.0f32; cells];
    let mut g3_re = vec![0.0f32; cells];
    let mut g3_im = vec![0.0f32; cells];

    let mut displacement = Vec::with_capacity(cells * frames * 8);
    let mut derivatives = Vec::new();
    let mut hermitian_residual = 0.0f32;

    let cpu_res = (CPU_SWELL_RESOLUTION as usize).min(n);
    let cpu_step = (n / cpu_res).max(1);
    let mut swell_data = vec![[0.0f32; 3]; cpu_res * cpu_res * frames];

    let half = n / 2;
    for frame in 0..frames {
        let time = frame as f32 * config.loop_period / frames as f32;
        for iz in 0..n {
            for ix in 0..n {
                let index = iz * n + ix;
                let kx = TAU * wave_frequency(ix, n) / config.tile_size;
                let kz = TAU * wave_frequency(iz, n) / config.tile_size;
                let wavenumber = kx.hypot(kz);
                let omega = quantized_omega(wavenumber, config.loop_period);
                let (cos, sin) = (omega * time).sin_cos();
                let mir_ix = if ix == 0 { 0 } else { n - ix };
                let mir_iz = if iz == 0 { 0 } else { n - iz };
                let mirror = mir_iz * n + mir_ix;
                let (a, b) = (h0_re[index], h0_im[index]);
                let (p, q) = (h0_re[mirror], h0_im[mirror]);
                let hr = a * cos - b * sin + p * cos - q * sin;
                let hi = a * sin + b * cos - p * sin - q * cos;
                let h = (hr, hi);
                let ih = (-hi, hr);
                let zero = (0.0f32, 0.0f32);
                let (sx, sz, dx, dz, jxx, jzz, jxz) = if wavenumber > 1.0e-4 {
                    let inv_k = 1.0 / wavenumber;
                    let lambda = config.choppiness;
                    let kill_x = ix == half;
                    let kill_z = iz == half;
                    (
                        if kill_x { zero } else { scale_c(kx, ih) },
                        if kill_z { zero } else { scale_c(kz, ih) },
                        if kill_x { zero } else { scale_c(-lambda * kx * inv_k, ih) },
                        if kill_z { zero } else { scale_c(-lambda * kz * inv_k, ih) },
                        scale_c(lambda * kx * kx * inv_k, h),
                        scale_c(lambda * kz * kz * inv_k, h),
                        if kill_x || kill_z { zero } else { scale_c(lambda * kx * kz * inv_k, h) },
                    )
                } else {
                    (zero, zero, zero, zero, zero, zero, zero)
                };
                fields[0][index] = h;
                fields[1][index] = sx;
                fields[2][index] = sz;
                fields[3][index] = dx;
                fields[4][index] = dz;
                fields[5][index] = jxx;
                fields[6][index] = jzz;
                fields[7][index] = jxz;
            }
        }
        for field in &fields {
            for iz in 0..n {
                for ix in 0..n {
                    let index = iz * n + ix;
                    let mir_ix = if ix == 0 { 0 } else { n - ix };
                    let mir_iz = if iz == 0 { 0 } else { n - iz };
                    let mirror = mir_iz * n + mir_ix;
                    let (re, im) = field[index];
                    let (mr, mi) = field[mirror];
                    hermitian_residual =
                        hermitian_residual.max((re - mr).abs()).max((im + mi).abs());
                }
            }
        }
        for index in 0..cells {
            let g0 = pack(fields[0][index], fields[3][index]);
            let g1 = pack(fields[4][index], fields[1][index]);
            let g2 = pack(fields[2][index], fields[5][index]);
            let g3 = pack(fields[6][index], fields[7][index]);
            g0_re[index] = g0.0;
            g0_im[index] = g0.1;
            g1_re[index] = g1.0;
            g1_im[index] = g1.1;
            g2_re[index] = g2.0;
            g2_im[index] = g2.1;
            g3_re[index] = g3.0;
            g3_im[index] = g3.1;
        }
        fft2(&mut g0_re, &mut g0_im, n, true, &tw_re, &tw_im, &mut scratch_re, &mut scratch_im);
        fft2(&mut g1_re, &mut g1_im, n, true, &tw_re, &tw_im, &mut scratch_re, &mut scratch_im);
        fft2(&mut g2_re, &mut g2_im, n, true, &tw_re, &tw_im, &mut scratch_re, &mut scratch_im);
        fft2(&mut g3_re, &mut g3_im, n, true, &tw_re, &tw_im, &mut scratch_re, &mut scratch_im);

        let mut deriv_layer = vec![0.0f32; cells * 4];
        for iz in 0..n {
            for ix in 0..n {
                let index = iz * n + ix;
                let dy = g0_re[index];
                let dx = g0_im[index];
                let dz = g1_re[index];
                let sx = g1_im[index];
                let sz = g2_re[index];
                let jxx = g2_im[index];
                let jzz = g3_re[index];
                let jxz = g3_im[index];
                let jacobian = (1.0 + jxx) * (1.0 + jzz) - jxz * jxz;
                push_f16_vec4(&mut displacement, [dx, dy, dz, jacobian]);
                let base = index * 4;
                deriv_layer[base] = sx;
                deriv_layer[base + 1] = sz;
                deriv_layer[base + 2] = (1.0 - jacobian).max(0.0);
                deriv_layer[base + 3] = 0.0;
                if ix % cpu_step == 0 && iz % cpu_step == 0 {
                    let cx = ix / cpu_step;
                    let cz = iz / cpu_step;
                    swell_data[(frame * cpu_res + cz) * cpu_res + cx] = [dx, dy, dz];
                }
            }
        }
        downsample_mips(&deriv_layer, n, n, &mut derivatives);
    }

    let mip_levels = (n as f32).log2() as u32 + 1;
    let crest_span = swell_data
        .iter()
        .map(|texel| texel[1].abs())
        .fold(0.0f32, f32::max);
    FftBakedBand {
        tile_size: config.tile_size,
        loop_period: config.loop_period,
        resolution: config.resolution,
        frames: config.frames,
        mip_levels,
        displacement,
        derivatives,
        swell_field: FftSwellField {
            resolution: cpu_res as u32,
            frames: config.frames,
            tile_size: config.tile_size,
            loop_period: config.loop_period,
            crest_span,
            data: swell_data,
        },
        hermitian_residual,
    }
}

#[derive(Resource)]
pub struct FftBakeReceiver {
    pub receiver: Mutex<Receiver<(FftBakedBand, FftBakedBand)>>,
}

pub fn bake_water_bands() -> (FftBakedBand, FftBakedBand) {
    (
        bake_band(&FftBandConfig::SWELL),
        bake_band(&FftBandConfig::CHOP),
    )
}

pub fn spawn_fft_bake() -> FftBakeReceiver {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(bake_water_bands());
    });
    FftBakeReceiver {
        receiver: Mutex::new(receiver),
    }
}

#[derive(Resource, Default)]
pub struct FftWaterTextures {
    pub ready: bool,
    pub swell_displacement: Handle<Image>,
    pub swell_derivatives: Handle<Image>,
    pub chop_displacement: Handle<Image>,
    pub chop_derivatives: Handle<Image>,
}

pub fn fft_sampler_descriptor() -> ImageSamplerDescriptor {
    ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    }
}

pub fn fft_placeholder_image() -> Image {
    let mut image = Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 2,
        },
        TextureDimension::D2,
        vec![0; 16],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(fft_sampler_descriptor());
    image
}

pub fn fft_displacement_image(band: &FftBakedBand) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: band.resolution,
            height: band.resolution,
            depth_or_array_layers: band.frames,
        },
        TextureDimension::D2,
        band.displacement.clone(),
        TextureFormat::Rgba16Float,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(fft_sampler_descriptor());
    image
}

pub fn fft_derivatives_image(band: &FftBakedBand) -> Image {
    let mut image = Image::new_uninit(
        Extent3d {
            width: band.resolution,
            height: band.resolution,
            depth_or_array_layers: band.frames,
        },
        TextureDimension::D2,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = band.mip_levels;
    image.data = Some(band.derivatives.clone());
    image.texture_descriptor.usage |= TextureUsages::TEXTURE_BINDING;
    image.sampler = ImageSampler::Descriptor(fft_sampler_descriptor());
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_config() -> FftBandConfig {
        FftBandConfig {
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
        }
    }

    fn f16_value(bits: u16) -> f32 {
        let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
        let exponent = ((bits >> 10) & 0x1f) as i32;
        let fraction = (bits & 0x3ff) as f32;
        if exponent == 0 {
            sign * fraction * 2.0f32.powi(-24)
        } else if exponent == 0x1f {
            if fraction == 0.0 { f32::INFINITY } else { f32::NAN }
        } else {
            sign * (1.0 + fraction / 1024.0) * 2.0f32.powi(exponent - 15)
        }
    }

    #[test]
    fn f16_encodes_known_values() {
        assert_eq!(f16_bits(0.0), 0x0000);
        assert_eq!(f16_bits(1.0), 0x3c00);
        assert_eq!(f16_bits(-1.0), 0xbc00);
        assert_eq!(f16_bits(0.5), 0x3800);
        assert_eq!(f16_bits(-2.0), 0xc000);
        assert_eq!(f16_bits(65504.0), 0x7bff);
        assert_eq!(f16_bits(1.0e-8), 0x0000);
        assert_eq!(f16_bits(f32::INFINITY), 0x7c00);
    }

    #[test]
    fn f16_round_trips_typical_heights() {
        for value in [-3.0, -0.25, 0.0, 0.125, 0.75, 2.5] {
            let decoded = f16_value(f16_bits(value));
            assert!((decoded - value).abs() <= value.abs() * 1.0e-3 + 1.0e-6, "{value} -> {decoded}");
        }
    }

    #[test]
    fn fft_round_trips_a_known_signal() {
        let n = 8;
        let (tw_re, tw_im) = twiddles(n);
        let mut re = vec![0.0f32; n];
        let mut im = vec![0.0f32; n];
        re[1] = 2.0;
        re[5] = -1.0;
        let original_re = re.clone();
        let original_im = im.clone();
        fft1d(&mut re, &mut im, false, &tw_re, &tw_im);
        fft1d(&mut re, &mut im, true, &tw_re, &tw_im);
        for i in 0..n {
            assert!((re[i] - original_re[i]).abs() < 1.0e-5);
            assert!((im[i] - original_im[i]).abs() < 1.0e-5);
        }
    }

    #[test]
    fn dispersion_snaps_to_loop_multiples() {
        for k in [0.01, 0.1, 1.0, 10.0, 100.0] {
            let omega = quantized_omega(k, 12.0);
            let delta = TAU / 12.0;
            let ratio = omega / delta;
            assert!((ratio - ratio.round()).abs() < 1.0e-3, "omega {omega} is not a multiple of {delta}");
            assert!(ratio >= 1.0);
        }
    }

    #[test]
    fn baked_bands_stay_finite_and_real() {
        let band = bake_band(&tiny_config());
        assert!(band.hermitian_residual < 1.0e-3, "hermitian symmetry leaked residual {}", band.hermitian_residual);
        let texels = band.displacement.chunks_exact(8);
        let mut saw_wave = false;
        for texel in texels {
            let dy = f16_value(u16::from_le_bytes([texel[2], texel[3]]));
            let jacobian = f16_value(u16::from_le_bytes([texel[6], texel[7]]));
            assert!(dy.is_finite());
            assert!(jacobian > 0.0 && jacobian < 10.0, "jacobian {jacobian} left the sane range");
            if dy.abs() > 1.0e-4 {
                saw_wave = true;
            }
        }
        assert!(saw_wave, "the tiny bake produced a flat sea");
    }

    #[test]
    fn the_swell_field_loops_and_tiles_exactly() {
        let band = bake_band(&tiny_config());
        let field = &band.swell_field;
        for step in 0..6 {
            let x = step as f32 * 1.3 - 3.0;
            let z = step as f32 * -0.7 + 2.0;
            let start = field.offset_at(x, z, 0.0);
            let looped = field.offset_at(x, z, band.loop_period);
            assert!((start - looped).length() < 1.0e-5, "the loop must close");
            let tiled = field.offset_at(x + band.tile_size, z - band.tile_size, 0.0);
            assert!((start - tiled).length() < 1.0e-5, "the tile must wrap");
        }
    }

    #[test]
    fn the_swell_height_stays_finite_across_the_loop() {
        let band = bake_band(&tiny_config());
        let field = &band.swell_field;
        for step in 0..16 {
            let time = step as f32 * band.loop_period / 16.0;
            let height = field.height_at(3.0, -5.0, time);
            assert!(height.is_finite());
        }
    }

    #[test]
    fn mip_data_covers_every_level() {
        let band = bake_band(&tiny_config());
        let texel_bytes = 8usize;
        let mut expected = 0usize;
        let mut w = band.resolution as usize;
        let mut h = band.resolution as usize;
        loop {
            expected += w * h * texel_bytes;
            if w == 1 && h == 1 {
                break;
            }
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        assert_eq!(band.derivatives.len(), expected * band.frames as usize);
    }
}
