import package::shaders::sdf::texture3d::df_gradient;
import package::shaders::sdf::volume::{
    SDF_OUTSIDE, SdfVolume, volume_inside, volume_sample, volume_uvw_in_field,
};

struct DistanceFieldViewUniform {
    volume: SdfVolume,
    time: f32,
    glow_range: f32,
    clay_alpha: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> distance_field_view: DistanceFieldViewUniform;

@group(constants::MATERIAL_BIND_GROUP) @binding(101)
var distance_field_texture: texture_3d<f32>;

fn view_field_distance(field_pos: vec3<f32>) -> f32 {
    let uvw = volume_uvw_in_field(distance_field_view.volume, field_pos);
    if (!volume_inside(uvw)) {
        return SDF_OUTSIDE;
    }
    return volume_sample(distance_field_view.volume, distance_field_texture, uvw);
}

fn view_field_distance_extended(field_pos: vec3<f32>) -> f32 {
    let uvw = volume_uvw_in_field(distance_field_view.volume, field_pos);
    return volume_sample(distance_field_view.volume, distance_field_texture, uvw);
}

fn view_field_gradient(field_pos: vec3<f32>) -> vec3<f32> {
    let uvw = volume_uvw_in_field(distance_field_view.volume, field_pos);
    if (!volume_inside(uvw)) {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    let span = distance_field_view.volume.bounds_max - distance_field_view.volume.bounds_min;
    let world_per_texel = span / distance_field_view.volume.resolution;
    return df_gradient(
        distance_field_texture,
        distance_field_view.volume.resolution,
        uvw,
        world_per_texel,
    );
}
