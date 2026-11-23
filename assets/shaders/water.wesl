import bevy_pbr::render::forward_io::{VertexOutput, FragmentOutput};
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::pbr_fragment::pbr_input_from_standard_material;
import bevy_pbr::render::pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing};
import package::shaders::distance_field::{df_sample, df_gradient};

struct WaterField {
    field_from_world: mat4x4<f32>,
    bounds_min: vec3<f32>,
    bounds_max: vec3<f32>,
    inv_bounds_size: vec3<f32>,
    resolution: vec3<f32>,
}

struct WaterParams {
    time: f32,
    water_height: f32,
    ripple_range: f32,
    ripple_frequency: f32,
    ripple_speed: f32,
    ripple_strength: f32,
    foam_width: f32,
    foam_strength: f32,
    field_count: u32,
    max_depth: f32,
    phase_g: f32,
    pad: f32,
    scattering: vec3<f32>,
    absorption: vec3<f32>,
    sun_direction: vec3<f32>,
    sun_illuminance: vec3<f32>,
    ambient_illuminance: vec3<f32>,
    fields: array<WaterField, 4>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> water: WaterParams;

@group(constants::MATERIAL_BIND_GROUP) @binding(101)
var water_field0: texture_3d<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(102)
var water_field1: texture_3d<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(103)
var water_field2: texture_3d<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(104)
var water_field3: texture_3d<f32>;

const WATER_OUTSIDE: f32 = 1.0e6;
const WATER_IOR: f32 = 1.333;
const WATER_F0: f32 = 0.02;
const WATER_DEPTH_STEPS: i32 = 12;

fn water_field_uvw(field: WaterField, p: vec3<f32>) -> vec3<f32> {
    let q = (field.field_from_world * vec4<f32>(p, 1.0)).xyz;
    return (q - field.bounds_min) * field.inv_bounds_size;
}

fn water_field_inside(uvw: vec3<f32>) -> bool {
    return all(uvw >= vec3<f32>(0.0)) && all(uvw <= vec3<f32>(1.0));
}

fn water_field_distance(field: WaterField, tex: texture_3d<f32>, p: vec3<f32>) -> f32 {
    let uvw = water_field_uvw(field, p);
    if (!water_field_inside(uvw)) {
        return WATER_OUTSIDE;
    }
    return df_sample(tex, field.resolution, uvw);
}

fn water_field_gradient(field: WaterField, tex: texture_3d<f32>, p: vec3<f32>) -> vec3<f32> {
    let uvw = water_field_uvw(field, p);
    if (!water_field_inside(uvw)) {
        return vec3<f32>(0.0);
    }
    let world_per_texel = (field.bounds_max - field.bounds_min) / field.resolution;
    return df_gradient(tex, field.resolution, uvw, world_per_texel);
}

fn water_min_distance(p: vec3<f32>) -> f32 {
    let count = i32(water.field_count);
    var d = WATER_OUTSIDE;
    if (count > 0) {
        d = min(d, water_field_distance(water.fields[0], water_field0, p));
    }
    if (count > 1) {
        d = min(d, water_field_distance(water.fields[1], water_field1, p));
    }
    if (count > 2) {
        d = min(d, water_field_distance(water.fields[2], water_field2, p));
    }
    if (count > 3) {
        d = min(d, water_field_distance(water.fields[3], water_field3, p));
    }
    return d;
}

fn water_contact(p: vec3<f32>) -> vec4<f32> {
    let count = i32(water.field_count);
    var d = WATER_OUTSIDE;
    var winner = -1;
    if (count > 0) {
        let d0 = water_field_distance(water.fields[0], water_field0, p);
        if (d0 < d) {
            d = d0;
            winner = 0;
        }
    }
    if (count > 1) {
        let d1 = water_field_distance(water.fields[1], water_field1, p);
        if (d1 < d) {
            d = d1;
            winner = 1;
        }
    }
    if (count > 2) {
        let d2 = water_field_distance(water.fields[2], water_field2, p);
        if (d2 < d) {
            d = d2;
            winner = 2;
        }
    }
    if (count > 3) {
        let d3 = water_field_distance(water.fields[3], water_field3, p);
        if (d3 < d) {
            d = d3;
            winner = 3;
        }
    }
    var g = vec3<f32>(0.0);
    if (winner == 0) {
        g = water_field_gradient(water.fields[0], water_field0, p);
    } else if (winner == 1) {
        g = water_field_gradient(water.fields[1], water_field1, p);
    } else if (winner == 2) {
        g = water_field_gradient(water.fields[2], water_field2, p);
    } else if (winner == 3) {
        g = water_field_gradient(water.fields[3], water_field3, p);
    }
    return vec4<f32>(g, d);
}

fn water_depth(p: vec3<f32>) -> f32 {
    let d = water_min_distance(p);
    if (d >= water.max_depth) {
        return water.max_depth;
    }
    var t = max(d, 0.0);
    for (var i: i32 = 0; i < WATER_DEPTH_STEPS; i = i + 1) {
        let step = water_min_distance(p - vec3<f32>(0.0, t, 0.0));
        if (step <= 0.0) {
            break;
        }
        t = t + max(step, 0.02);
        if (t >= water.max_depth) {
            return water.max_depth;
        }
    }
    return min(t, water.max_depth);
}

fn water_refract(v: vec3<f32>, n: vec3<f32>, eta: f32) -> vec3<f32> {
    let cos_i = dot(v, n);
    let cos_t2 = 1.0 - eta * eta * (1.0 - cos_i * cos_i);
    if (cos_t2 < 0.0) {
        return reflect(-v, n);
    }
    let cos_t = -(select(1.0, -1.0, cos_i >= 0.0)) * sqrt(cos_t2);
    return (eta * cos_i + cos_t) * n - eta * v;
}

fn schlick_phase(g: f32, cos_theta: f32) -> f32 {
    let d = max(1.0 + g * g - 2.0 * g * cos_theta, 1.0e-4);
    return (1.0 - g * g) / pow(d, 1.5);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var out: FragmentOutput;
    var pbr_input = pbr_input_from_standard_material(in, front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );

    let p = pbr_input.world_position.xyz;
    let contact = water_contact(p);
    let d = contact.w;
    let grad = contact.xyz;

    let depth = water_depth(vec3<f32>(p.x, water.water_height, p.z));
    pbr_input.material.thickness = depth;

    let dc = min(d, water.ripple_range);
    var q = p;
    var outward = vec2<f32>(0.0);
    if (d < water.ripple_range) {
        let grad_len = length(grad);
        if (grad_len > 0.0) {
            let nrm = grad / grad_len;
            let xz_len = max(length(vec2<f32>(nrm.x, nrm.z)), 1.0e-4);
            outward = vec2<f32>(nrm.x, nrm.z) / xz_len;
            q = p - nrm * dc;
        }
    }
    let vertical = q.y - water.water_height;
    let hd = length(vec2<f32>(p.x - q.x, p.z - q.z));
    let r = hd + abs(vertical);
    let emerged = exp(-abs(vertical) * 4.0);
    let phase = r * water.ripple_frequency - water.time * water.ripple_speed;
    let phase_delta = max(abs(dpdx(phase)), abs(dpdy(phase)));
    let ripple_aa = 1.0 - clamp(phase_delta * 0.33, 0.0, 1.0);
    let falloff = max(1.0 - r / water.ripple_range, 0.0);
    let view_dist = length(p - view.world_position);
    let dist_fade = clamp(1.0 - view_dist / 60.0, 0.0, 1.0);
    if (d < water.ripple_range) {
        let amp = water.ripple_strength * emerged * falloff * falloff * ripple_aa * dist_fade;
        pbr_input.N = normalize(pbr_input.N + vec3<f32>(outward.x, 0.0, outward.y) * (sin(phase) * amp));
    }

    let foam_phase = depth * water.ripple_frequency - water.time * water.ripple_speed;
    let foam_delta = max(abs(dpdx(foam_phase)), abs(dpdy(foam_phase)));
    let foam_aa = 1.0 - clamp(foam_delta * 0.33, 0.0, 1.0);
    let shallow = max(1.0 - depth / water.foam_width, 0.0);
    let foam = clamp(
        water.foam_strength * shallow * shallow * (0.7 + 0.3 * sin(foam_phase)) * foam_aa * dist_fade,
        0.0,
        1.0,
    );
    if (foam > 0.0) {
        pbr_input.material.base_color = mix(pbr_input.material.base_color, vec4<f32>(1.0), foam);
        pbr_input.material.perceptual_roughness = clamp(
            pbr_input.material.perceptual_roughness + 0.5 * foam,
            0.0,
            1.0,
        );
    }

    out.color = apply_pbr_lighting(pbr_input);

    let extinction = water.scattering + water.absorption;
    let transmittance = exp(-extinction * depth);
    let scattering_amount = clamp(
        water.scattering * (1.0 - transmittance) / max(extinction, vec3<f32>(1.0e-5)),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
    var in_scatter = vec3<f32>(0.0);
    let under_dir = water_refract(pbr_input.V, pbr_input.N, 1.0 / WATER_IOR);
    let phase_value = schlick_phase(water.phase_g, dot(-water.sun_direction, under_dir));
    let ndv = max(dot(pbr_input.N, pbr_input.V), 0.0);
    let fresnel = WATER_F0 + (1.0 - WATER_F0) * pow(1.0 - ndv, 5.0);
    let incoming = water.sun_illuminance * phase_value + water.ambient_illuminance;
    in_scatter = incoming * scattering_amount * (1.0 - fresnel);

    out.color = vec4<f32>(out.color.rgb + in_scatter, out.color.a);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
