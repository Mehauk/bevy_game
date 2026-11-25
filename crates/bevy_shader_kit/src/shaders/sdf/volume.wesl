import package::shaders::sdf::texture3d::{df_gradient, df_sample};

const SDF_OUTSIDE: f32 = 1.0e6;

struct SdfVolume {
    field_from_world: mat4x4<f32>,
    bounds_min: vec3<f32>,
    bounds_max: vec3<f32>,
    inv_bounds_size: vec3<f32>,
    resolution: vec3<f32>,
}

fn volume_uvw(volume: SdfVolume, world_pos: vec3<f32>) -> vec3<f32> {
    let p = (volume.field_from_world * vec4<f32>(world_pos, 1.0)).xyz;
    return (p - volume.bounds_min) * volume.inv_bounds_size;
}

fn volume_uvw_in_field(volume: SdfVolume, field_pos: vec3<f32>) -> vec3<f32> {
    return (field_pos - volume.bounds_min) * volume.inv_bounds_size;
}

fn volume_inside(uvw: vec3<f32>) -> bool {
    return all(uvw >= vec3<f32>(0.0)) && all(uvw <= vec3<f32>(1.0));
}

fn volume_box_distance(volume: SdfVolume, uvw: vec3<f32>) -> f32 {
    let size = volume.bounds_max - volume.bounds_min;
    let to_box = (abs(uvw - vec3<f32>(0.5)) * 2.0 - vec3<f32>(1.0)) * size * 0.5;
    let outside = length(max(vec3<f32>(0.0), to_box));
    let inside = min(0.0, max(to_box.x, max(to_box.y, to_box.z)));
    return outside + inside;
}

fn volume_sample(volume: SdfVolume, tex: texture_3d<f32>, uvw: vec3<f32>) -> f32 {
    return df_sample(tex, volume.resolution, clamp(uvw, vec3<f32>(0.0), vec3<f32>(1.0)));
}

fn volume_sample_at(volume: SdfVolume, tex: texture_3d<f32>, uvw: vec3<f32>, to_box: f32) -> f32 {
    if (to_box > 0.0) {
        return SDF_OUTSIDE;
    }
    return volume_sample(volume, tex, uvw);
}

fn volume_sample_world(volume: SdfVolume, tex: texture_3d<f32>, world_pos: vec3<f32>) -> f32 {
    let uvw = volume_uvw(volume, world_pos);
    return volume_sample_at(volume, tex, uvw, volume_box_distance(volume, uvw));
}

fn volume_gradient(volume: SdfVolume, tex: texture_3d<f32>, uvw: vec3<f32>) -> vec3<f32> {
    let world_per_texel = (volume.bounds_max - volume.bounds_min) / volume.resolution;
    return df_gradient(
        tex,
        volume.resolution,
        clamp(uvw, vec3<f32>(0.0), vec3<f32>(1.0)),
        world_per_texel,
    );
}

fn volume_gradient_world(volume: SdfVolume, tex: texture_3d<f32>, world_pos: vec3<f32>) -> vec3<f32> {
    return volume_gradient(volume, tex, volume_uvw(volume, world_pos));
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
