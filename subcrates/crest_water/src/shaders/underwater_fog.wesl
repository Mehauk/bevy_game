import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import bevy_render::view::{View, position_ndc_to_world, uv_to_ndc};

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@if(MULTISAMPLED)
@group(0) @binding(2) var depth_texture: texture_depth_multisampled_2d;
@if(!MULTISAMPLED)
@group(0) @binding(2) var depth_texture: texture_depth_2d;
@group(0) @binding(3) var<uniform> view: View;

struct UnderwaterFogSettings {
    water_level: f32,
    time: f32,
    enabled: u32,
    pad: f32,
    tint: vec3<f32>,
    deep_color: vec3<f32>,
    depth_density: f32,
    darkening: f32,
    distortion_strength: f32,
    distortion_scale: f32,
}

@group(0) @binding(4) var<uniform> settings: UnderwaterFogSettings;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    if (settings.enabled == 0u) {
        return textureSampleLevel(screen_texture, screen_sampler, in.uv, 0.0);
    }
    let origin = view.world_position;
    let lens = position_ndc_to_world(vec3<f32>(uv_to_ndc(in.uv), 1.0), view.world_from_clip);
    let ray = normalize(lens - origin);

    let ripple = sin(in.uv.y * 24.0 * settings.distortion_scale + settings.time * 2.0)
        + 0.5 * sin(in.uv.x * 31.0 * settings.distortion_scale - settings.time * 1.7);
    let offset = ray.xz * (ripple * settings.distortion_strength);
    let scene = textureSampleLevel(screen_texture, screen_sampler, in.uv + offset, 0.0);

    let depth = textureLoad(depth_texture, vec2<i32>(in.position.xy), 0);
    var travel = 1000.0;
    if (depth > 0.0) {
        let hit = position_ndc_to_world(vec3<f32>(uv_to_ndc(in.uv), depth), view.world_from_clip);
        travel = distance(origin, hit);
    }
    if (ray.y > 0.0) {
        travel = min(travel, max((settings.water_level - origin.y) / ray.y, 0.0));
    }

    let absorption = exp(-settings.tint * travel);
    var color = scene.rgb * absorption + settings.deep_color * (vec3<f32>(1.0, 1.0, 1.0) - absorption);

    let below = max(settings.water_level - origin.y, 0.0);
    color = color * exp(-settings.darkening * below);
    return vec4<f32>(color, scene.a);
}
