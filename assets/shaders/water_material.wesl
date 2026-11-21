import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::main_pass_post_lighting_processing,
    forward_io::{VertexOutput, FragmentOutput},
};

struct WaterSurface {
    shallow_color: vec4<f32>,
    deep_color: vec4<f32>,
    sky_zenith: vec4<f32>,
    sky_horizon: vec4<f32>,
    sun_color: vec4<f32>,
    sun_direction: vec4<f32>,
    wind: vec4<f32>,
    absorption: vec4<f32>,
    time: f32,
    wave_scale: f32,
    wave_amplitude: f32,
    foam_depth: f32,
    foam_scale: f32,
    frozen_blend: f32,
    ice_roughness: f32,
    fresnel_strength: f32,
    kind: u32,
    use_vertex_data: u32,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(31) var<uniform> water: WaterSurface;

const KIND_SEA: u32 = 0u;
const KIND_LAKE: u32 = 1u;
const KIND_RIVER: u32 = 2u;
const KIND_WATERFALL: u32 = 3u;

const DETAIL_RATIO: f32 = 1.73;
const FLOW_CYCLE: f32 = 9.0;
const FLOW_DETAIL: f32 = 0.11;
const FLOW_MIN_SPEED: f32 = 0.18;
const FLOW_SLOPE_GAIN: f32 = 0.9;
const RIPPLE_FREQ: f32 = 2.4;
const RIPPLE_AMP: f32 = 0.35;
const PI: f32 = 3.14159265;

fn hash21(p: vec2<f32>) -> f32 {
    let s = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(s) * 43758.5453123);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn wave_height(p: vec2<f32>) -> f32 {
    let a = value_noise(p);
    let b = value_noise(p * 2.17 + vec2<f32>(17.0, 3.0));
    return a + 0.5 * b;
}

fn wave_gradient(p: vec2<f32>) -> vec2<f32> {
    let e = 0.15;
    let hx = wave_height(p + vec2<f32>(e, 0.0)) - wave_height(p - vec2<f32>(e, 0.0));
    let hy = wave_height(p + vec2<f32>(0.0, e)) - wave_height(p - vec2<f32>(0.0, e));
    return vec2<f32>(hx, hy) / (2.0 * e);
}

fn standing_gradient(world_xz: vec2<f32>) -> vec2<f32> {
    let drive = water.wind.xy * water.wind.z;
    let g1 = wave_gradient(world_xz * water.wave_scale + drive * (water.time * 0.05));
    let cross_wind = vec2<f32>(-drive.y, drive.x);
    let g2 = wave_gradient(world_xz * (water.wave_scale * DETAIL_RATIO) + cross_wind * (water.time * 0.031));
    return g1 + 0.6 * g2;
}

fn flow_envelope(phase: f32) -> f32 {
    return clamp((1.0 - abs((phase - 0.5) * 2.0)) * 1.5, 0.0, 1.0);
}

fn running_gradient(world_xz: vec2<f32>, flow: vec2<f32>, across: f32, slope: f32) -> vec2<f32> {
    let dir = normalize(flow);
    let channel = sin(clamp(across, 0.0, 1.0) * PI);
    let speed = (FLOW_MIN_SPEED + (1.0 - FLOW_MIN_SPEED) * channel) * (1.0 + slope * FLOW_SLOPE_GAIN);
    let downstream = dot(world_xz, dir);
    var g = vec2<f32>(0.0);
    for (var i: u32 = 0u; i < 3u; i = i + 1u) {
        let phase = fract(downstream / FLOW_CYCLE - water.time * speed * 0.12 + f32(i) * 0.33333333);
        let w = flow_envelope(phase);
        let p = (world_xz + dir * (phase * FLOW_CYCLE)) * FLOW_DETAIL;
        g = g + w * wave_gradient(p);
    }
    return g / 2.0;
}

@fragment
fn fragment(vertex_output: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(vertex_output, is_front);
    let base_alpha = pbr_input.material.base_color.a;
    let world_pos = vertex_output.world_position.xyz;

    var vdata = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    @if(VERTEX_COLORS)
    if (water.use_vertex_data == 1u) {
        vdata = vertex_output.color;
    }
    @if(true)

    let dpos_dx = dpdx(vertex_output.world_position.xyz);
    let dpos_dy = dpdy(vertex_output.world_position.xyz);
    let du_x = dpdx(vertex_output.uv.x);
    let du_y = dpdy(vertex_output.uv.x);
    let dv_x = dpdx(vertex_output.uv.y);
    let dv_y = dpdy(vertex_output.uv.y);
    let det = du_x * dv_y - du_y * dv_x;

    var flow = vec2<f32>(0.0);
    var slope = 0.0;
    if (abs(det) > 1e-7) {
        let down = (dpos_dy * du_x - dpos_dx * du_y) / det;
        slope = abs(down.y) / max(length(vec2<f32>(down.x, down.z)), 1e-5);
        if (water.kind == KIND_RIVER) {
            flow = vec2<f32>(vdata.x, vdata.y);
        } else if (water.kind == KIND_WATERFALL) {
            let down_xz = vec2<f32>(down.x, down.z);
            if (length(down_xz) > 1e-6) {
                flow = normalize(down_xz);
            }
        }
    }
    let running = length(flow) > 1e-4;

    var depth = max(vdata.r, 0.0);
    var frozen = water.frozen_blend;
    var grad = vec2<f32>(0.0);
    var ripple = 0.0;
    var shore = 0.0;
    let ripple_phase = depth * RIPPLE_FREQ - water.time * 2.2;
    let ripple_grad = vec2<f32>(dpdx(ripple_phase), dpdy(ripple_phase));
    if (running) {
        depth = 0.0;
        grad = running_gradient(world_pos.xz, flow, vertex_output.uv.x, min(slope, 3.0));
    } else {
        frozen = clamp(max(vdata.g, water.frozen_blend), 0.0, 1.0);
        grad = standing_gradient(world_pos.xz);
        shore = 1.0 - smoothstep(0.0, max(water.foam_depth, 0.01) * 2.0, depth);
        ripple = sin(ripple_phase);
        grad = grad + ripple_grad * (RIPPLE_AMP * shore);
    }

    var wave_n = normalize(vec3<f32>(-grad.x * water.wave_amplitude, 1.0, -grad.y * water.wave_amplitude));
    if (!is_front) {
        wave_n = -wave_n;
    }
    let geo_n = pbr_input.N;
    var N = normalize(mix(geo_n, wave_n, mix(0.85, 0.15, frozen)));
    if (water.kind == KIND_WATERFALL) {
        N = normalize(geo_n + vec3<f32>(grad.x, 0.0, grad.y) * 0.35);
    }

    let V = pbr_input.V;
    let ndv = max(dot(N, V), 0.0);
    let fresnel = clamp(water.fresnel_strength + (1.0 - water.fresnel_strength) * pow(1.0 - ndv, 5.0), 0.0, 1.0);

    let transmit = exp(-depth * water.absorption.rgb * water.absorption.w);
    var body = mix(water.deep_color.rgb, water.shallow_color.rgb, transmit);

    let streak = value_noise(vec2<f32>(world_pos.x * 0.7, world_pos.z * 2.2 - water.time * 1.8));
    var foam = 0.0;
    if (water.kind == KIND_WATERFALL) {
        foam = clamp(0.55 + 0.6 * streak, 0.0, 1.0);
    } else if (water.kind == KIND_RIVER) {
        let bank = min(vertex_output.uv.x, 1.0 - vertex_output.uv.x);
        let bank_foam = 1.0 - smoothstep(0.0, 0.14, bank);
        foam = clamp(bank_foam * (0.35 + 0.65 * streak), 0.0, 1.0);
    } else {
        let depth_foam = 1.0 - smoothstep(0.0, max(water.foam_depth, 0.01), depth);
        foam = clamp(max(depth_foam, shore * max(ripple, 0.0) * 0.3) * (0.4 + 0.9 * streak) * water.foam_scale, 0.0, 1.0);
    }

    var roughness = mix(0.06, water.ice_roughness, frozen);
    if (water.kind == KIND_WATERFALL) {
        roughness = 0.4;
    }

    var colour = body;
    var under_absorb = 0.0;
    if (is_front) {
        let R = reflect(-V, N);
        let sky = mix(water.sky_horizon.rgb, water.sky_zenith.rgb, clamp(R.y, 0.0, 1.0));
        colour = mix(colour, sky, fresnel);
        let L = water.sun_direction.xyz;
        let H = normalize(V + L);
        let spec = pow(max(dot(N, H), 0.0), mix(220.0, 24.0, roughness));
        colour = colour + water.sun_color.rgb * (water.sun_direction.w * spec);
    } else {
        under_absorb = clamp(1.0 - (transmit.r + transmit.g + transmit.b) / 3.0, 0.0, 1.0);
        colour = mix(water.shallow_color.rgb, water.deep_color.rgb, 0.2 + 0.6 * under_absorb);
        let shimmer = value_noise(vec2<f32>(world_pos.x * 1.3 + water.time * 0.6, world_pos.z * 1.1 - water.time * 0.4));
        colour = colour + water.sun_color.rgb * water.sun_direction.w * (0.16 + 0.24 * shimmer) * (1.0 - under_absorb);
        colour = colour + vec3<f32>(0.04 * streak);
    }

    if (frozen > 0.0) {
        let crack_field = abs(value_noise(world_pos.xz * 0.35) - 0.5);
        let cracks = 1.0 - smoothstep(0.0, 0.035, crack_field);
        let ice = mix(vec3<f32>(0.82, 0.90, 0.95), vec3<f32>(0.45, 0.58, 0.68), cracks);
        colour = mix(colour, ice, frozen);
        foam = foam * (1.0 - frozen);
    }

    colour = mix(colour, vec3<f32>(0.94, 0.97, 1.0), foam);

    var alpha = base_alpha;
    if (!is_front) {
        alpha = base_alpha * mix(0.55, 0.9, under_absorb);
    }
    alpha = mix(alpha, 1.0, frozen);
    alpha = clamp(alpha + foam * 0.5, 0.0, 1.0);

    pbr_input.N = N;
    pbr_input.world_normal = N;
    pbr_input.material.base_color = vec4<f32>(colour, alpha);
    pbr_input.material.perceptual_roughness = roughness;
    pbr_input.material.metallic = 0.0;

    var out: FragmentOutput;
    out.color = vec4<f32>(colour, alpha);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}