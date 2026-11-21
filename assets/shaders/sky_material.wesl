import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::main_pass_post_lighting_processing,
    forward_io::{VertexOutput, FragmentOutput},
};

struct SkySurface {
    sun_direction: vec4<f32>,
    sun_color: vec4<f32>,
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    ground: vec4<f32>,
    scattering: vec2<f32>,
    snow: f32,
    exposure_gain: f32,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(31) var<uniform> sky: SkySurface;

const RAYLEIGH_BETA: f32 = 0.09;
const MIE_BETA: f32 = 0.045;
const RAYLEIGH_NORMAL: f32 = 0.0596831;
const MIE_NORMAL: f32 = 0.0795775;

fn rayleigh_phase(cos_theta: f32) -> f32 {
    return RAYLEIGH_NORMAL * (1.0 + cos_theta * cos_theta);
}

fn mie_phase(cos_theta: f32, asymmetry: f32) -> f32 {
    let g2 = asymmetry * asymmetry;
    let denom = max(1.0 + g2 - 2.0 * asymmetry * cos_theta, 0.01);
    return MIE_NORMAL * (1.0 - g2) / (denom * sqrt(denom));
}

@fragment
fn fragment(vertex_output: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(vertex_output, is_front);

    var view_dir: vec3<f32>;
    @if(VERTEX_COLORS)
    view_dir = normalize(vertex_output.color.rgb);
    @else
    view_dir = normalize(vertex_output.world_position.xyz);
    @if(true)

    let sun_dir = normalize(sky.sun_direction.xyz);
    let sun_weight = sky.sun_direction.w;
    let height = clamp(view_dir.y, -1.0, 1.0);
    let cos_sun = clamp(dot(view_dir, sun_dir), -1.0, 1.0);

    let view_mass = 1.0 / (max(height, 0.0) + 0.12);
    let sun_mass = 1.0 / (max(sun_dir.y, 0.0) + 0.12);
    let sunset = clamp(sun_dir.y * 2.4 + 0.2, 0.0, 1.0);

    var colour = mix(sky.horizon.rgb, sky.zenith.rgb, pow(max(height, 0.0), 0.42));

    let rayleigh_depth = RAYLEIGH_BETA * view_mass * sun_mass / (view_mass + sun_mass);
    let rayleigh_glow = rayleigh_phase(cos_sun) * (1.0 - exp(-rayleigh_depth * 8.0));
    let mie_depth = MIE_BETA * sky.scattering.y * view_mass * sun_mass / (view_mass + sun_mass);
    let mie_glow = mie_phase(cos_sun, sky.scattering.x) * (1.0 - exp(-mie_depth * 8.0));

    let warm = vec3<f32>(1.0, 0.55, 0.28);
    let scatter_tint = mix(warm, sky.sun_color.rgb, sunset);
    let scatter = scatter_tint * (rayleigh_glow * 2.2 + mie_glow * 9.0) * sun_weight;
    colour = colour + scatter * mix(vec3<f32>(1.0), vec3<f32>(0.85, 0.95, 1.15), 1.0 - sunset);

    let disc = smoothstep(0.9990, 0.9997, cos_sun);
    let halo = smoothstep(0.96, 0.9995, cos_sun) * 0.25;
    colour = colour + sky.sun_color.rgb * (disc * 6.0 + halo) * sun_weight;

    colour = mix(sky.ground.rgb, colour, smoothstep(-0.10, 0.03, height));
    colour = mix(colour, vec3<f32>(0.88, 0.93, 0.99), sky.snow * 0.25);
    colour = colour * sky.exposure_gain;

    pbr_input.material.base_color = vec4<f32>(colour, 1.0);
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr_input, vec4<f32>(colour, 1.0));
    return out;
}