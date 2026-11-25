import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import package::shaders::noise::fbm2d::scrolling_fbm_2d;
import package::shaders::optics::fresnel::fresnel_schlick;
import package::shaders::optics::scattering::{beer_lambert_in_scatter, schlick_phase};
import package::shaders::post_process::bindings::{
    depth_texture, near_plane_world, sample_scene_depth, scene_world_position, screen_sampler,
    screen_texture, view,
};

struct UnderwaterSettings {
    water_height: f32,
    time: f32,
    max_depth: f32,
    phase_g: f32,
    scattering: vec3<f32>,
    absorption: vec3<f32>,
    sun_direction: vec3<f32>,
    sun_illuminance: vec3<f32>,
    ambient_illuminance: vec3<f32>,
    meniscus_thickness: f32,
    distortion_strength: f32,
    enabled: u32,
    pad0: f32,
    meniscus_color: vec3<f32>,
}

@group(0) @binding(4) var<uniform> settings: UnderwaterSettings;

const WATER_F0: f32 = 0.02;
const MENISCUS_STRENGTH: f32 = 0.35;
const MENISCUS_WAVE_SCALE: f32 = 3.0;
const MENISCUS_WAVE_HEIGHT: f32 = 0.04;
const DISTORTION_FADE: f32 = 0.35;
const DISTORTION_WAVE_SCALE: f32 = 1.5;

fn surface_height(p: vec2<f32>, time: f32) -> f32 {
    let wave = scrolling_fbm_2d(p * MENISCUS_WAVE_SCALE, time) - 0.5;
    return settings.water_height + wave * MENISCUS_WAVE_HEIGHT;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let above = textureSampleLevel(screen_texture, screen_sampler, in.uv, 0.0);
    let lens = near_plane_world(in.uv);
    let origin = view.world_position;
    let ray = normalize(lens - origin);

    let surface_y = surface_height(lens.xz, settings.time);
    let submersion = surface_y - lens.y;
    let half_band = max(settings.meniscus_thickness * 0.5, 1.0e-4);
    if (submersion <= -half_band) {
        return above;
    }
    let submerged = smoothstep(-half_band, half_band, submersion);

    var water_distance = settings.max_depth;
    let depth = sample_scene_depth(in.position.xy);
    if (depth > 0.0) {
        water_distance = distance(origin, scene_world_position(in.uv, depth));
    }
    if (ray.y > 0.0) {
        let exit_distance = (settings.water_height - origin.y) / ray.y;
        water_distance = min(water_distance, max(exit_distance, 0.0));
    }
    water_distance = min(water_distance, settings.max_depth);

    let fade = smoothstep(half_band, half_band + DISTORTION_FADE, submersion);
    let ripple = scrolling_fbm_2d(lens.xz * DISTORTION_WAVE_SCALE + ray.xz, settings.time) - 0.5;
    let offset = ray.xz * ripple * settings.distortion_strength * fade;
    let scene = textureSampleLevel(screen_texture, screen_sampler, in.uv + offset, 0.0);

    let extinction = settings.scattering + settings.absorption;
    let transmittance = exp(-extinction * water_distance);
    let phase_value = schlick_phase(settings.phase_g, dot(-settings.sun_direction, ray));
    let fresnel = fresnel_schlick(WATER_F0, abs(ray.y));
    let in_scatter = beer_lambert_in_scatter(
        settings.scattering,
        settings.absorption,
        water_distance,
        settings.sun_illuminance,
        settings.ambient_illuminance,
        phase_value,
        fresnel,
    );

    var color = mix(above.rgb, scene.rgb * transmittance + in_scatter, submerged);
    let rim = 1.0 - min(abs(submersion) / half_band, 1.0);
    color = mix(color, settings.meniscus_color, rim * MENISCUS_STRENGTH);
    return vec4<f32>(color, above.a);
}
