import package::shaders::water::swell_types::{ShoreContext, shore_factor, swell_amplitude_span, swell_height_shored};
import package::shaders::water::swells::swells;
import package::shaders::water::types::water;

const COAST_SWASH_RUNUP: f32 = 0.15;
const COAST_SWASH_TUCK: f32 = -0.15;
const COAST_SWASH_BLEND: f32 = 1.5;
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

fn coast_texel_of(world_xz: vec2<f32>) -> vec2<i32> {
    let uv = (world_xz - coast.bounds_min) * coast.inv_size;
    return coast_clamp(vec2<i32>(floor(uv * coast.resolution)));
}

fn coast_sample_at(texel: vec2<i32>) -> vec2<f32> {
    return textureLoad(coast_map, coast_clamp(texel), 0).rg;
}

fn terrain_height_at(world_xz: vec2<f32>) -> f32 {
    return coast_sample_at(coast_texel_of(world_xz)).x;
}

fn spill_height_at(world_xz: vec2<f32>) -> f32 {
    return coast_sample_at(coast_texel_of(world_xz)).y;
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

fn coastal_wash_mask(shore: ShoreContext, swell: f32) -> f32 {
    let span = max(swell_amplitude_span(swells), 0.05);
    return clamp(0.5 + 0.5 * swell / span, 0.0, 1.0);
}

fn coastal_surface_y(rest_xz: vec2<f32>, time: f32) -> f32 {
    let shore = shore_context_at(rest_xz);
    let terrain_y = water.water_height - shore.rest_depth;
    let swell = swell_height_shored(swells, rest_xz, time, shore);
    let offshore_y = water.water_height + swell;
    let wash = mix(COAST_SWASH_TUCK, COAST_SWASH_RUNUP, coastal_wash_mask(shore, swell));
    let near_land = 1.0 - smoothstep(0.0, COAST_SWASH_BLEND, shore.rest_depth);
    return mix(offshore_y, terrain_y + wash, near_land);
}

fn coastal_normal(rest_xz: vec2<f32>, time: f32) -> vec3<f32> {
    let e = 0.5;
    let h0 = coastal_surface_y(rest_xz, time);
    let hx = coastal_surface_y(rest_xz + vec2(e, 0.0), time) - h0;
    let hz = coastal_surface_y(rest_xz + vec2(0.0, e), time) - h0;
    return normalize(vec3(-hx / e, 1.0, -hz / e));
}
