import package::shaders::distance_field::{df_sample, df_gradient};

struct DistanceFieldView {
    field_from_world: mat4x4<f32>,
    bounds_min: vec3<f32>,
    bounds_max: vec3<f32>,
    inv_bounds_size: vec3<f32>,
    resolution: vec3<f32>,
    time: f32,
    glow_range: f32,
    clay_alpha: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> distance_field_view: DistanceFieldView;

@group(constants::MATERIAL_BIND_GROUP) @binding(101)
var distance_field_texture: texture_3d<f32>;

const DISTANCE_FIELD_OUTSIDE: f32 = 1.0e6;

fn distance_field_uvw(world_pos: vec3<f32>) -> vec3<f32> {
    let p = (distance_field_view.field_from_world * vec4<f32>(world_pos, 1.0)).xyz;
    return (p - distance_field_view.bounds_min) * distance_field_view.inv_bounds_size;
}

fn distance_field_inside(uvw: vec3<f32>) -> bool {
    return all(uvw >= vec3<f32>(0.0)) && all(uvw <= vec3<f32>(1.0));
}

fn distance_field_world(world_pos: vec3<f32>) -> f32 {
    let uvw = distance_field_uvw(world_pos);
    if (!distance_field_inside(uvw)) {
        return DISTANCE_FIELD_OUTSIDE;
    }
    return df_sample(distance_field_texture, distance_field_view.resolution, uvw);
}

fn distance_field_field(field_pos: vec3<f32>) -> f32 {
    let uvw = (field_pos - distance_field_view.bounds_min) * distance_field_view.inv_bounds_size;
    if (!distance_field_inside(uvw)) {
        return DISTANCE_FIELD_OUTSIDE;
    }
    return df_sample(distance_field_texture, distance_field_view.resolution, uvw);
}

fn distance_field_field_extended(field_pos: vec3<f32>) -> f32 {
    let uvw = (field_pos - distance_field_view.bounds_min) * distance_field_view.inv_bounds_size;
    let clamped = clamp(uvw, vec3<f32>(0.0), vec3<f32>(1.0));
    return df_sample(distance_field_texture, distance_field_view.resolution, clamped);
}

fn distance_field_gradient_field(field_pos: vec3<f32>) -> vec3<f32> {
    let uvw = (field_pos - distance_field_view.bounds_min) * distance_field_view.inv_bounds_size;
    if (!distance_field_inside(uvw)) {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    let span = distance_field_view.bounds_max - distance_field_view.bounds_min;
    let world_per_texel = span / distance_field_view.resolution;
    return df_gradient(distance_field_texture, distance_field_view.resolution, uvw, world_per_texel);
}

fn distance_field_gradient_world(world_pos: vec3<f32>) -> vec3<f32> {
    let uvw = distance_field_uvw(world_pos);
    if (!distance_field_inside(uvw)) {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    let span = distance_field_view.bounds_max - distance_field_view.bounds_min;
    let world_per_texel = span / distance_field_view.resolution;
    return df_gradient(distance_field_texture, distance_field_view.resolution, uvw, world_per_texel);
}

fn line_box_intersect(ro: vec3<f32>, re: vec3<f32>, box_min: vec3<f32>, box_max: vec3<f32>) -> vec2<f32> {
    let inv = 1.0 / (re - ro);
    let first = (box_min - ro) * inv;
    let second = (box_max - ro) * inv;
    let closest = min(first, second);
    let furthest = max(first, second);
    let t_near = max(closest.x, max(closest.y, closest.z));
    let t_far = min(furthest.x, min(furthest.y, furthest.z));
    return clamp(vec2<f32>(t_near, t_far), vec2<f32>(0.0), vec2<f32>(1.0));
}
