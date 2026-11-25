import bevy_render::view::{View, position_ndc_to_world, uv_to_ndc};

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@if(MULTISAMPLED)
@group(0) @binding(2) var depth_texture: texture_depth_multisampled_2d;
@if(!MULTISAMPLED)
@group(0) @binding(2) var depth_texture: texture_depth_2d;
@group(0) @binding(3) var<uniform> view: View;

fn sample_scene_depth(frag_coord: vec2<f32>) -> f32 {
    return textureLoad(depth_texture, vec2<i32>(frag_coord), 0);
}

fn near_plane_world(uv: vec2<f32>) -> vec3<f32> {
    return position_ndc_to_world(vec3<f32>(uv_to_ndc(uv), 1.0), view.world_from_clip);
}

fn scene_world_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    return position_ndc_to_world(vec3<f32>(uv_to_ndc(uv), depth), view.world_from_clip);
}
