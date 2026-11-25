import bevy_pbr::render::forward_io::{VertexOutput, FragmentOutput};
import bevy_pbr::render::pbr_fragment::pbr_input_from_standard_material;
import bevy_pbr::render::pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing};

struct Wetness {
    height: f32,
    feather: f32,
    darkening: f32,
    wet_roughness: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> wetness: Wetness;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var out: FragmentOutput;
    var pbr_input = pbr_input_from_standard_material(in, front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );
    let wet = 1.0 - smoothstep(
        wetness.height - wetness.feather,
        wetness.height + wetness.feather,
        pbr_input.world_position.y,
    );
    let tinted = pbr_input.material.base_color;
    pbr_input.material.base_color = vec4<f32>(tinted.rgb * mix(1.0, wetness.darkening, wet), tinted.a);
    pbr_input.material.perceptual_roughness = mix(
        pbr_input.material.perceptual_roughness,
        wetness.wet_roughness,
        wet,
    );
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
