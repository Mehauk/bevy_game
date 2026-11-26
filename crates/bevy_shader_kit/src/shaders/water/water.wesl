import bevy_pbr::render::forward_io::{VertexOutput, FragmentOutput};
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::pbr_fragment::pbr_input_from_standard_material;
import bevy_pbr::render::pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing};
import bevy_pbr::render::view_transformations::depth_ndc_to_view_z;
import bevy_pbr::prepass::utils::prepass_depth;
import package::shaders::math::falloff::view_distance_fade;
import package::shaders::noise::fbm2d::scrolling_fbm_2d;
import package::shaders::sdf::volume::volume_sample_world;
import package::shaders::water::body::{body_sdf, body_volume};
import package::shaders::water::foam::{
    apply_foam_roughness, blend_foam_color, body_contact_mask, foam_noise_mask, shore_edge_mask,
};
import package::shaders::water::optics::water_volume_in_scatter;
import package::shaders::water::ripples::{ripple_foam_edge, ripple_normal, ripple_slope};
import package::shaders::water::types::{WaterContext, water};

const WATER_VIEW_FADE_RANGE: f32 = 60.0;

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

    ctx.depth = water.optics.max_depth;
    @if(DEPTH_PREPASS) {
        let surface_view_z = depth_ndc_to_view_z(in.position.z);
        let behind_view_z = depth_ndc_to_view_z(prepass_depth(in.position, 0u));
        ctx.depth = clamp(surface_view_z - behind_view_z, 0.0, water.optics.max_depth);
    }
    var body_distance: f32 = 1.0e6;
    @if(WATER_BODY_FOAM) {
        body_distance = volume_sample_world(body_volume, body_sdf, ctx.p);
        ctx.depth = min(ctx.depth, max(body_distance, 0.0));
    }
    pbr_input.material.thickness = ctx.depth;

    ctx.ripple = vec2<f32>(0.0);
    @if(WATER_RIPPLES) {
        ctx.ripple = ripple_slope(ctx.p.xz, water.time);
        pbr_input.N = ripple_normal(pbr_input.N, ctx.ripple, water.ripple_normal);
    }

    @if(WATER_FOAM) {
        var shore_edge = shore_edge_mask(ctx.depth, water.foam_width);
        ctx.edge = max(shore_edge, ripple_foam_edge(length(ctx.ripple), water.ripple_foam));
        @if(WATER_BODY_FOAM) {
            let surface_proximity = 1.0 - smoothstep(0.0, water.foam_width, abs(ctx.p.y - water.water_height));
            ctx.edge = max(ctx.edge, body_contact_mask(body_distance, surface_proximity));
        }
        ctx.pattern = scrolling_fbm_2d(ctx.p.xz * 2.2, water.time);
        ctx.foam = foam_noise_mask(ctx.edge, ctx.pattern, water.foam_strength, ctx.view_fade);
        if (ctx.foam > 0.0) {
            pbr_input.material.perceptual_roughness = apply_foam_roughness(pbr_input.material.perceptual_roughness, ctx.foam);
        }
    }

    out.color = apply_pbr_lighting(pbr_input);

    @if(WATER_OPTICS) {
        if (front) {
            let in_scatter = water_volume_in_scatter(pbr_input.V, pbr_input.N, ctx.depth, water.optics.scattering, water.optics.absorption, water.optics.sun_direction, water.optics.sun_illuminance, water.optics.ambient_illuminance, water.optics.phase_g);
            out.color = vec4<f32>(out.color.rgb + in_scatter, out.color.a);
        }
    }

    @if(WATER_FOAM) {
        if (ctx.foam > 0.0) {
            out.color = vec4<f32>(blend_foam_color(out.color.rgb, pbr_input.N, ctx.foam), out.color.a);
        }
    }

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
