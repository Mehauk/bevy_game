import package::shaders::water::fft_types::{
    fft, fft_chop_derivatives, fft_chop_displacement, fft_sampler, fft_swell_derivatives, fft_swell_displacement,
};

fn fft_frame(time: f32, period: f32, frames: u32) -> vec3<f32> {
    let u = fract(time / period) * f32(frames);
    let lower = floor(u);
    let first = u32(lower) % frames;
    let second = (first + 1u) % frames;
    return vec3<f32>(f32(first), f32(second), u - lower);
}

fn fft_sample_layer(
    tex: texture_2d_array<f32>,
    xz: vec2<f32>,
    time: f32,
    tile_size: f32,
    period: f32,
    frames: u32,
) -> vec4<f32> {
    let uv = xz / tile_size;
    let f = fft_frame(time, period, frames);
    let a = textureSampleLevel(tex, fft_sampler, uv, i32(f.x), 0.0);
    let b = textureSampleLevel(tex, fft_sampler, uv, i32(f.y), 0.0);
    return mix(a, b, f.z);
}

fn fft_swell_atten(cam_dist: f32) -> f32 {
    return 1.0 - smoothstep(fft.swell_fade_start, fft.swell_fade_end, cam_dist);
}

fn fft_chop_atten(cam_dist: f32) -> f32 {
    return 1.0 - smoothstep(fft.chop_fade_start, fft.chop_fade_end, cam_dist);
}

fn fft_vertex_displacement(xz: vec2<f32>, time: f32, cam_dist: f32) -> vec3<f32> {
    let swell = fft_sample_layer(fft_swell_displacement, xz, time, fft.swell_tile_size, fft.swell_period, fft.frames);
    let chop = fft_sample_layer(fft_chop_displacement, xz, time, fft.chop_tile_size, fft.chop_period, fft.frames);
    let swell_w = fft.swell_strength * fft_swell_atten(cam_dist);
    let chop_w = fft.chop_strength * fft_chop_atten(cam_dist);
    return swell.xyz * swell_w + chop.xyz * chop_w;
}

fn fft_vertex_slope(xz: vec2<f32>, time: f32, cam_dist: f32) -> vec2<f32> {
    let swell = fft_sample_layer(fft_swell_derivatives, xz, time, fft.swell_tile_size, fft.swell_period, fft.frames);
    let chop = fft_sample_layer(fft_chop_derivatives, xz, time, fft.chop_tile_size, fft.chop_period, fft.frames);
    let swell_w = fft.swell_strength * fft_swell_atten(cam_dist);
    let chop_w = fft.chop_strength * fft_chop_atten(cam_dist);
    return swell.xy * swell_w + chop.xy * chop_w;
}

fn fft_vertex_normal(xz: vec2<f32>, time: f32, cam_dist: f32) -> vec3<f32> {
    let slope = fft_vertex_slope(xz, time, cam_dist);
    return normalize(vec3<f32>(-slope.x, 1.0, -slope.y));
}

fn fft_sample_mip(
    tex: texture_2d_array<f32>,
    xz: vec2<f32>,
    time: f32,
    tile_size: f32,
    period: f32,
    frames: u32,
) -> vec4<f32> {
    let uv = xz / tile_size;
    let f = fft_frame(time, period, frames);
    let a = textureSample(tex, fft_sampler, uv, i32(f.x));
    let b = textureSample(tex, fft_sampler, uv, i32(f.y));
    return mix(a, b, f.z);
}

fn fft_whitecap(jacobian: f32) -> f32 {
    return smoothstep(fft.whitecap_threshold, fft.whitecap_threshold - 0.55, jacobian) * fft.whitecap_gain;
}

fn fft_surface(xz: vec2<f32>, time: f32) -> vec4<f32> {
    let swell_slope = fft_sample_mip(fft_swell_derivatives, xz, time, fft.swell_tile_size, fft.swell_period, fft.frames);
    let chop_slope = fft_sample_mip(fft_chop_derivatives, xz, time, fft.chop_tile_size, fft.chop_period, fft.frames);
    let swell_disp = fft_sample_layer(fft_swell_displacement, xz, time, fft.swell_tile_size, fft.swell_period, fft.frames);
    let chop_disp = fft_sample_layer(fft_chop_displacement, xz, time, fft.chop_tile_size, fft.chop_period, fft.frames);
    let slope = swell_slope.xy * fft.swell_strength + chop_slope.xy * fft.chop_strength;
    let swell_j = 1.0 - (1.0 - swell_disp.w) * fft.swell_strength * fft.swell_strength;
    let chop_j = 1.0 - (1.0 - chop_disp.w) * fft.chop_strength * fft.chop_strength;
    let whitecap = max(fft_whitecap(swell_j), fft_whitecap(chop_j));
    return vec4<f32>(slope, whitecap, clamp(fft.chop_strength, 0.15, 1.2));
}
