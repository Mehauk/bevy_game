import bevy_pbr::render::forward_io::{VertexOutput, FragmentOutput};
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::pbr_fragment::pbr_input_from_standard_material;
import bevy_pbr::render::pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing};
import package::shaders::math::aa::phase_derivative_aa;
import package::shaders::math::falloff::{quadratic_falloff, view_distance_fade};
import package::shaders::noise::fbm2d::scrolling_fbm_2d;
import package::shaders::sdf::contact::project_onto_contact;
import package::shaders::sdf::field_array::{
    field_array_contact, field_array_raymarch_depth,
};
import package::shaders::water::foam::{
    apply_foam_roughness, blend_foam_color, foam_noise_mask, proximity_edge_mask, shore_edge_mask,
};
import package::shaders::water::optics::water_volume_in_scatter;
import package::shaders::water::ripples::{
    perturb_normal_horizontal, radial_contact_distance, ripple_wave_signal,
};
import package::shaders::water::types::{
    WaterContext, water, water_field0, water_field1, water_field2, water_field3,
};

const WATER_VIEW_FADE_RANGE: f32 = 60.0;
const WATER_DEPTH_STEPS_DEFAULT: i32 = 12;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var out: FragmentOutput;
    var pbr_input = pbr_input_from_standard_material(in, front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );

    var ctx: WaterContext;
    ctx.p = pbr_input.world_position.xyz;
    ctx.view_fade = view_distance_fade(ctx.p, view.world_position, WATER_VIEW_FADE_RANGE);

    var fields = water.fields;
    let count = i32(water.field_count);

    let contact = field_array_contact(&fields, count, water_field0, water_field1, water_field2, water_field3, ctx.p, water.ripple_range);
    ctx.distance = contact.distance;
    ctx.gradient = contact.gradient;
    ctx.boundary = contact.boundary;

    var depth_steps: i32 = WATER_DEPTH_STEPS_DEFAULT;
    @if(WATER_DEPTH_STEPS) { depth_steps = i32(constants::WATER_DEPTH_STEPS); }
    ctx.depth = field_array_raymarch_depth(&fields, count, water_field0, water_field1, water_field2, water_field3, vec3<f32>(ctx.p.x, water.water_height, ctx.p.z), water.max_depth, depth_steps);
    pbr_input.material.thickness = ctx.depth;

    let surface = project_onto_contact(ctx.p, ctx.gradient, ctx.distance, water.ripple_range, water.water_height);
    ctx.point = surface.point;
    ctx.outward = surface.outward;
    ctx.emerged = surface.emerged;

    @if(WATER_RIPPLES) {
        ctx.ripple_distance = radial_contact_distance(ctx.p, ctx.point, water.water_height);
        let phase = ctx.ripple_distance * water.ripple_frequency - water.time * water.ripple_speed;
        let wave = ripple_wave_signal(ctx.ripple_distance, water.ripple_frequency, water.time, water.ripple_speed);
        let ripple_aa = phase_derivative_aa(phase);
        let falloff = quadratic_falloff(ctx.ripple_distance, water.ripple_range);
        if (ctx.distance < water.ripple_range) {
            let amp = water.ripple_strength * ctx.emerged * ctx.boundary * falloff * ripple_aa * ctx.view_fade;
            pbr_input.N = perturb_normal_horizontal(pbr_input.N, ctx.outward, wave, amp);
        }
    }

    @if(WATER_FOAM) {
        ctx.edge = max(shore_edge_mask(ctx.depth, water.foam_width), proximity_edge_mask(ctx.distance, water.foam_width, ctx.emerged));
        ctx.pattern = scrolling_fbm_2d(ctx.p.xz * 2.2, water.time);
        ctx.foam = foam_noise_mask(ctx.edge, ctx.pattern, water.foam_strength, ctx.view_fade);
        if (ctx.foam > 0.0) {
            pbr_input.material.perceptual_roughness = apply_foam_roughness(pbr_input.material.perceptual_roughness, ctx.foam);
        }
    }

    out.color = apply_pbr_lighting(pbr_input);

    @if(WATER_OPTICS) {
        let in_scatter = water_volume_in_scatter(pbr_input.V, pbr_input.N, ctx.depth, water.scattering, water.absorption, water.sun_direction, water.sun_illuminance, water.ambient_illuminance, water.phase_g);
        out.color = vec4<f32>(out.color.rgb + in_scatter, out.color.a);
    }

    @if(WATER_FOAM) {
        if (ctx.foam > 0.0) {
            out.color = vec4<f32>(blend_foam_color(out.color.rgb, pbr_input.N, ctx.foam), out.color.a);
        }
    }

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
