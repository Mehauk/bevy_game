import package::shaders::crest_math::deep_dispersion;
import package::shaders::crest_math::cmul;

const PI2_U: f32 = 6.28318530717;

struct SpectrumUpdateUniforms {
    time: f32,
    chop: f32,
    gravity: f32,
    loop_period: f32,
    size: u32,
    lod_count: u32,
    fft_world: f32,
    pad1: f32,
}

@group(0) @binding(0)
var init_spectrum: texture_2d_array<f32>;

@group(0) @binding(1)
var out_height: texture_storage_2d_array<rg32float, write>;

@group(0) @binding(2)
var out_disp_x: texture_storage_2d_array<rg32float, write>;

@group(0) @binding(3)
var out_disp_z: texture_storage_2d_array<rg32float, write>;

@group(0) @binding(4)
var<uniform> uni: SpectrumUpdateUniforms;

@compute @workgroup_size(8, 8, 1)
fn spectrum_update(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = uni.size;
    if (gid.x >= size || gid.y >= size || gid.z >= uni.lod_count) {
        return;
    }
    let center = i32(size / 2u);
    let coord = vec2<i32>(i32(gid.x) - center, i32(gid.y) - center);
    let layer = i32(gid.z);
    let world_size = uni.fft_world * f32(1u << gid.z);
    let k = PI2_U * vec2<f32>(coord) / world_size;
    let k_mag = length(k);
    let disp = deep_dispersion(k_mag, uni.gravity, uni.loop_period);
    let w = disp.x;
    let sw = sin(w * uni.time);
    let cw = cos(w * uni.time);
    let h0 = textureLoad(init_spectrum, vec2<i32>(i32(gid.x), i32(gid.y)), layer, 0);
    let fwd = vec2<f32>(cw, -sw);
    let bkwd = vec2<f32>(cw, sw);
    let h = cmul(h0.xy, fwd) + cmul(h0.zw, bkwd);
    let coord3 = vec2<i32>(i32(gid.x), i32(gid.y));
    textureStore(out_height, coord3, layer, vec4<f32>(h, 0.0, 0.0));
    textureStore(out_disp_x, coord3, layer, vec4<f32>(uni.chop * vec2<f32>(-h.y * k.x, h.x * k.x) / (k_mag + 0.00001), 0.0, 0.0));
    textureStore(out_disp_z, coord3, layer, vec4<f32>(uni.chop * vec2<f32>(-h.y * k.y, h.x * k.y) / (k_mag + 0.00001), 0.0, 0.0));
}
