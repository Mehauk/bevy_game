import package::shaders::crest_math::wang_hash;
import package::shaders::crest_math::rng_gauss;
import package::shaders::crest_math::rng_float;
import package::shaders::crest_math::deep_dispersion;
import package::shaders::crest_math::pm_wind_term;
import package::shaders::crest_math::pos_cos_squared_spreading;
import package::shaders::crest_math::sample_power_log;

const PI2_S: f32 = 6.28318530717;

struct SpectrumUniforms {
    size: u32,
    lod_count: u32,
    wind_speed: f32,
    gravity: f32,
    loop_period: f32,
    turbulence: f32,
    chop: f32,
    fft_world: f32,
    wind_direction: vec2<f32>,
    power_log: array<vec4<f32>, 14>,
}

@group(0) @binding(0)
var out_spectrum: texture_storage_2d_array<rgba16float, write>;

@group(0) @binding(1)
var<uniform> uni: SpectrumUniforms;

@compute @workgroup_size(8, 8, 1)
fn spectrum_init(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = uni.size;
    if (gid.x >= size || gid.y >= size || gid.z >= uni.lod_count) {
        return;
    }
    let center = i32(size / 2u);
    let coord = vec2<i32>(i32(gid.x) - center, i32(gid.y) - center);
    let layer = i32(gid.z);
    if (coord.x == 0 && coord.y == 0) {
        textureStore(out_spectrum, vec2<i32>(i32(gid.x), i32(gid.y)), layer, vec4<f32>(0.0));
        return;
    }
    let max_coord = u32(max(abs(coord.x), abs(coord.y)));
    if (gid.z < uni.lod_count - 1u && (max_coord < 4u || max_coord >= 8u)) {
        textureStore(out_spectrum, vec2<i32>(i32(gid.x), i32(gid.y)), layer, vec4<f32>(0.0));
        return;
    }
    let world_size = uni.fft_world * f32(1u << gid.z);
    let k = PI2_S * vec2<f32>(coord) / world_size;
    let k_mag = length(k);
    var state = wang_hash(gid.z * size * size + gid.y * size + gid.x);
    let disp = deep_dispersion(k_mag, uni.gravity, uni.loop_period);
    let w = disp.x;
    let dwdk = disp.y;
    let wavelength = PI2_S / k_mag;
    let octave_index = log2(wavelength) + 4.0;
    let spectrum = sample_power_log(uni.power_log, octave_index) * pm_wind_term(w, uni.gravity, uni.wind_speed);
    let cos_theta = dot(k, uni.wind_direction) / k_mag;
    let spread_pos = pos_cos_squared_spreading(cos_theta, uni.turbulence);
    let spread_neg = pos_cos_squared_spreading(-cos_theta, uni.turbulence);
    let d_k = PI2_S / world_size;
    let scale = d_k * d_k * dwdk / k_mag;
    let amp_pos = rng_gauss(&state) * sqrt(abs(spectrum * spread_pos * scale) * 2.0);
    let amp_neg = rng_gauss(&state) * sqrt(abs(spectrum * spread_neg * scale) * 2.0);
    let phase_pos = rng_float(&state) * PI2_S;
    let phase_neg = rng_float(&state) * PI2_S;
    let h_pos = amp_pos * vec2<f32>(cos(phase_pos), -sin(phase_pos));
    let h_neg = amp_neg * vec2<f32>(cos(phase_neg), -sin(phase_neg));
    textureStore(out_spectrum, vec2<i32>(i32(gid.x), i32(gid.y)), layer, vec4<f32>(h_pos, h_neg) * 1.5);
}
