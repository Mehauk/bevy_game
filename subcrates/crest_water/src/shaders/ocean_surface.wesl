import bevy_pbr::render::forward_io::{VertexOutput, FragmentOutput};
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::pbr_fragment::pbr_input_from_standard_material;
import bevy_pbr::render::pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing};

struct OceanLodTransform {
    origin: vec3<f32>,
    texel_size: f32,
    extent: f32,
    lod_index: f32,
    pad: f32,
}

struct OceanCascadeUniform {
    lods: array<OceanLodTransform, 8>,
    lod_count: u32,
    resolution: u32,
    time: f32,
    water_level: f32,
}

struct OceanShading {
    deep_color: vec3<f32>,
    shallow_color: vec3<f32>,
    sun_direction: vec3<f32>,
    sun_color: vec3<f32>,
    sky_color: vec3<f32>,
    depth_falloff: f32,
    foam_strength: f32,
    fresnel_strength: f32,
    specular_strength: f32,
    specular_shininess: f32,
    normal_strength: f32,
    flow_gain: f32,
    pad: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> ocean: OceanCascadeUniform;

@group(constants::MATERIAL_BIND_GROUP) @binding(101)
var<uniform> shading: OceanShading;

@group(constants::MATERIAL_BIND_GROUP) @binding(102)
var waves_tex: texture_2d_array<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(103)
var derivs_tex: texture_2d_array<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(104)
var depth_tex: texture_2d_array<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(105)
var foam_tex: texture_2d_array<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(106)
var flow_tex: texture_2d_array<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(107)
var cascade_sampler: sampler;

fn lod_for_distance(distance: f32) -> u32 {
    for (var i: u32 = 0u; i < ocean.lod_count; i = i + 1u) {
        if (distance <= ocean.lods[i].extent * 0.5) {
            return i;
        }
    }
    return ocean.lod_count - 1u;
}

fn cascade_uv(index: u32, world_xz: vec2<f32>) -> vec2<f32> {
    let t = ocean.lods[index];
    return clamp((world_xz - t.origin.xz) / t.extent, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
}

fn cascade_blend(index: u32, world_xz: vec2<f32>) -> f32 {
    let uv = (world_xz - ocean.lods[index].origin.xz) / ocean.lods[index].extent;
    let edge = max(abs(uv.x - 0.5), abs(uv.y - 0.5));
    return smoothstep(0.4, 0.5, edge);
}

fn sample_cascaded(tex: texture_2d_array<f32>, world_xz: vec2<f32>) -> vec4<f32> {
    let distance = length(world_xz - view.world_position.xz);
    let index = lod_for_distance(distance);
    let near = textureSampleLevel(tex, cascade_sampler, cascade_uv(index, world_xz), i32(index), 0.0);
    let next = min(index + 1u, ocean.lod_count - 1u);
    let far = textureSampleLevel(tex, cascade_sampler, cascade_uv(next, world_xz), i32(next), 0.0);
    return mix(near, far, cascade_blend(index, world_xz));
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    var pbr_input = pbr_input_from_standard_material(in, true);
    let world = in.world_position.xyz;

    if (ocean.lod_count == 0u) {
        out.color = vec4<f32>(shading.deep_color, 1.0);
        return out;
    }

    let waves = sample_cascaded(waves_tex, world.xz);
    let derivs = sample_cascaded(derivs_tex, world.xz);
    let flow = sample_cascaded(flow_tex, world.xz);
    let depth = max(sample_cascaded(depth_tex, world.xz).x + waves.y, 0.0);
    let folding = clamp(0.65 - waves.w, 0.0, 1.0);
    let foam = clamp(sample_cascaded(foam_tex, world.xz).x + 0.35 * folding, 0.0, 1.0) * shading.foam_strength;

    let tilt = derivs.xy * shading.normal_strength + flow.xz * shading.flow_gain;
    let normal = normalize(vec3<f32>(-tilt.x, 1.0, -tilt.y));
    pbr_input.N = normal;

    let depth_mix = 1.0 - exp(-depth * max(shading.depth_falloff, 0.001));
    var color = mix(shading.shallow_color, shading.deep_color, depth_mix);
    color = mix(color, vec3<f32>(1.0, 1.0, 1.0), foam);
    pbr_input.material.base_color = vec4<f32>(color, 1.0);
    pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.9, foam);

    out.color = apply_pbr_lighting(pbr_input);

    let view_dir = pbr_input.V;
    let fresnel = clamp(shading.fresnel_strength * pow(1.0 - max(dot(normal, view_dir), 0.0), 5.0), 0.0, 1.0);
    out.color = vec4<f32>(mix(out.color.rgb, shading.sky_color, fresnel), out.color.a);

    let sun = normalize(shading.sun_direction);
    let halfway = normalize(view_dir + sun);
    let specular = pow(max(dot(normal, halfway), 0.0), max(shading.specular_shininess, 1.0));
    out.color = vec4<f32>(out.color.rgb + shading.sun_color * (shading.specular_strength * specular), out.color.a);

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
