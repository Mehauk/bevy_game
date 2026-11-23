import bevy_pbr::render::forward_io::{VertexOutput, FragmentOutput};
import bevy_pbr::render::mesh_view_bindings::view;
import package::shaders::distance_field_view::{
    distance_field_view, distance_field_field, distance_field_field_extended,
    distance_field_gradient_field, line_box_intersect,
};

const SDF_DIR_LIGHT_COLOR: vec3<f32> = vec3<f32>(10.0, 10.0, 10.0);
const SDF_DIR_LIGHT_DIR: vec3<f32> = vec3<f32>(0.707, 0.0, 0.707);
const SDF_SKY_COLOR: vec3<f32> = vec3<f32>(0.9, 0.95, 1.0);
const SDF_ALBEDO: f32 = 0.05;
const SDF_MAX_STEPS: i32 = 64;
const SDF_TRACE_DISTANCE: f32 = 1000.0;
const SDF_SURFACE_BIAS_VOXELS: f32 = 0.25;
const SDF_MIN_STEP_VOXELS: f32 = 0.03125;
const SDF_GLOW_STRENGTH: f32 = 0.35;

fn sdf_surface_normal(p: vec3<f32>) -> vec3<f32> {
    let w2v = distance_field_view.field_from_world;
    let grad = distance_field_gradient_field(p);
    let grad_len = length(grad);
    var n_vol = vec3<f32>(0.0, 1.0, 0.0);
    if (grad_len > 0.0) {
        n_vol = grad / grad_len;
    }
    let w2v_rot = mat3x3<f32>(w2v[0].xyz, w2v[1].xyz, w2v[2].xyz);
    let world_grad = transpose(w2v_rot) * n_vol;
    let world_len = length(world_grad);
    var n = vec3<f32>(0.0, 1.0, 0.0);
    if (world_len > 0.0) {
        n = world_grad / world_len;
    }
    return n;
}

fn sdf_clay(n: vec3<f32>) -> vec3<f32> {
    let ndl = max(dot(normalize(SDF_DIR_LIGHT_DIR), n), 0.0);
    let hemi = 0.5 + 0.5 * n.y;
    return (SDF_DIR_LIGHT_COLOR * ndl + SDF_SKY_COLOR * hemi) * SDF_ALBEDO;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var out: FragmentOutput;

    let w2v = distance_field_view.field_from_world;
    let cam = (w2v * vec4<f32>(view.world_position, 1.0)).xyz;
    let frag = (w2v * vec4<f32>(in.world_position.xyz, 1.0)).xyz;
    let ray = frag - cam;
    let ray_len = length(ray);
    if (ray_len < 1.0e-5) {
        out.color = vec4<f32>(0.0);
        return out;
    }
    let dir = ray / ray_len;

    let center = 0.5 * (distance_field_view.bounds_min + distance_field_view.bounds_max);
    let extent = (distance_field_view.bounds_max - distance_field_view.bounds_min) * 0.5;
    let span = extent * 2.0;
    let texel = span / distance_field_view.resolution;
    let texel_max = max(max(texel.x, texel.y), texel.z);

    let min_step = texel_max * SDF_MIN_STEP_VOXELS;
    let surface_bias = texel_max * SDF_SURFACE_BIAS_VOXELS;
    let glow_range = distance_field_view.glow_range;

    let cam_v = cam - center;
    let far = cam_v + dir * SDF_TRACE_DISTANCE;
    let march_extent = extent + vec3<f32>(glow_range);
    let cam_inside = all(abs(cam_v) <= march_extent);
    if (front == cam_inside) {
        out.color = vec4<f32>(0.0);
        return out;
    }
    let times = line_box_intersect(cam_v, far, -march_extent, march_extent) * SDF_TRACE_DISTANCE;
    if (times.x >= times.y) {
        out.color = vec4<f32>(0.0);
        return out;
    }
    let field_times = line_box_intersect(cam_v, far, -extent, extent) * SDF_TRACE_DISTANCE;

    var t = times.x;
    var hit = -1.0;
    var min_d = 1.0e9;
    var p_min = cam_v;
    for (var i: i32 = 0; i < SDF_MAX_STEPS; i = i + 1) {
        let p = cam_v + center + dir * t;
        let d_ext = distance_field_field_extended(p);
        let d = distance_field_field(p);
        if (t > glow_range && t >= field_times.x && t <= field_times.y && d < min_d) {
            min_d = d;
            p_min = p;
        }
        let expand = surface_bias * saturate(t / (2.0 * surface_bias));
        if (d < expand && t >= field_times.x && t <= field_times.y) {
            hit = clamp(t + d - expand, field_times.x, field_times.y);
            break;
        }
        t = t + max(d_ext, min_step);
        if (t > times.y + expand) {
            break;
        }
    }

    if (hit >= 0.0) {
        let p = cam_v + center + dir * hit;
        out.color = vec4<f32>(sdf_clay(sdf_surface_normal(p)), distance_field_view.clay_alpha);
        return out;
    }

    if (min_d >= glow_range) {
        out.color = vec4<f32>(0.0);
        return out;
    }
    let f = pow(saturate(1.0 - min_d / glow_range), 2.0);
    let glow = sdf_clay(sdf_surface_normal(p_min)) * (f * SDF_GLOW_STRENGTH);
    let a = max(glow.r, max(glow.g, glow.b));
    out.color = vec4<f32>(glow / max(a, 1.0e-4), min(a, 1.0));
    return out;
}
