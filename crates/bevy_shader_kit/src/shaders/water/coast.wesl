import package::shaders::water::swell_types::{ShoreContext, shore_factor, swell_amplitude_span, swell_height_shored};
import package::shaders::water::swells::swells;
import package::shaders::water::types::water;

const COAST_SWASH_BLEND: f32 = 1.5;
const COAST_TROUGH_CLEARANCE: f32 = 0.03;
const COAST_SPAN_FLOOR: f32 = 0.05;
const COAST_SHOAL_MAX: f32 = 2.5;
const COAST_BLUR_TEXELS: f32 = 4.0;

struct CoastField {
    bounds_min: vec2<f32>,
    texel: vec2<f32>,
    inv_size: vec2<f32>,
    resolution: vec2<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(105)
var<uniform> coast: CoastField;

@group(constants::MATERIAL_BIND_GROUP) @binding(106)
var coast_map: texture_2d<f32>;

fn coast_clamp(texel: vec2<i32>) -> vec2<i32> {
    return clamp(texel, vec2<i32>(0), vec2<i32>(coast.resolution) - vec2<i32>(1));
}

fn coast_sample_at(texel: vec2<i32>) -> vec2<f32> {
    return textureLoad(coast_map, coast_clamp(texel), 0).rg;
}

fn coast_weight_of(world_xz: vec2<f32>) -> vec2<f32> {
    let uv = (world_xz - coast.bounds_min) * coast.inv_size;
    return clamp(
        uv * coast.resolution - vec2<f32>(0.5),
        vec2<f32>(0.0),
        coast.resolution - vec2<f32>(1.0),
    );
}

fn terrain_height_at(world_xz: vec2<f32>) -> f32 {
    let origin = floor(coast_weight_of(world_xz));
    let weight = coast_weight_of(world_xz) - origin;
    let corner = vec2<i32>(i32(origin.x), i32(origin.y));
    let west = coast_sample_at(corner).x;
    let east = coast_sample_at(corner + vec2<i32>(1, 0)).x;
    let north = coast_sample_at(corner + vec2<i32>(0, 1)).x;
    let south = coast_sample_at(corner + vec2<i32>(1, 1)).x;
    return mix(mix(west, east, weight.x), mix(north, south, weight.x), weight.y);
}

fn spill_height_at(world_xz: vec2<f32>) -> f32 {
    let origin = floor(coast_weight_of(world_xz));
    let corner = vec2<i32>(i32(origin.x), i32(origin.y));
    let west = coast_sample_at(corner).y;
    let east = coast_sample_at(corner + vec2<i32>(1, 0)).y;
    let north = coast_sample_at(corner + vec2<i32>(0, 1)).y;
    let south = coast_sample_at(corner + vec2<i32>(1, 1)).y;
    return min(min(west, east), min(north, south));
}

fn terrain_gradient_at(world_xz: vec2<f32>) -> vec2<f32> {
    let step = coast.texel * COAST_BLUR_TEXELS;
    let hx = terrain_height_at(world_xz + vec2(step.x, 0.0)) - terrain_height_at(world_xz - vec2(step.x, 0.0));
    let hz = terrain_height_at(world_xz + vec2(0.0, step.y)) - terrain_height_at(world_xz - vec2(0.0, step.y));
    return vec2(hx, hz) / (2.0 * step);
}

fn shore_context_at(world_xz: vec2<f32>) -> ShoreContext {
    var shore: ShoreContext;
    shore.rest_depth = water.water_height - terrain_height_at(world_xz);
    shore.shore_factor = shore_factor(shore.rest_depth);
    shore.energy = water.wave_energy;
    let gradient = terrain_gradient_at(world_xz);
    let gradient_len = dot(gradient, gradient);
    if (gradient_len > 1.0e-8) {
        shore.shore_dir = gradient / sqrt(gradient_len);
    } else {
        shore.shore_dir = vec2<f32>(0.0);
    }
    return shore;
}

fn swell_span() -> f32 {
    return max(swell_amplitude_span(swells), COAST_SPAN_FLOOR);
}

fn max_crest_height() -> f32 {
    return swell_span() * COAST_SHOAL_MAX * (0.5 + water.wave_energy);
}

fn coastal_wash_mask(shore: ShoreContext, swell: f32) -> f32 {
    return clamp(0.5 + 0.5 * swell / swell_span(), 0.0, 1.0);
}

fn coastal_surface_y(rest_xz: vec2<f32>, time: f32, swell_atten: f32) -> f32 {
    let shore = shore_context_at(rest_xz);
    let swell = swell_height_shored(swells, rest_xz, time, shore) * swell_atten;
    let trough_floor = -max(shore.rest_depth - COAST_TROUGH_CLEARANCE, 0.0);
    return water.water_height + max(swell, trough_floor);
}

fn coastal_normal(rest_xz: vec2<f32>, time: f32, swell_atten: f32) -> vec3<f32> {
    let e = 0.5;
    let h0 = coastal_surface_y(rest_xz, time, swell_atten);
    let hx = coastal_surface_y(rest_xz + vec2(e, 0.0), time, swell_atten) - h0;
    let hz = coastal_surface_y(rest_xz + vec2(0.0, e), time, swell_atten) - h0;
    return normalize(vec3(-hx / e, 1.0, -hz / e));
}
