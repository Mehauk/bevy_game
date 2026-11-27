struct LodTransform {
    origin: vec3<f32>,
    texel_size: f32,
    extent: f32,
    lod_index: f32,
    pad: f32,
}

struct LodUniforms {
    lods: array<LodTransform, 8>,
    lod_count: u32,
    resolution: u32,
    time: f32,
    water_level: f32,
}

struct FoamUniforms {
    dt: f32,
    decay: f32,
    wave_folding_threshold: f32,
    shoreline_depth: f32,
    shoreline_width: f32,
    wave_gain: f32,
    shoreline_gain: f32,
    dynamic_gain: f32,
    resolution: u32,
    lod_count: u32,
    spline_count: u32,
    pad: f32,
    shifts: array<vec4<f32>, 8>,
}

@group(0) @binding(0)
var prev_tex: texture_2d_array<f32>;

@group(0) @binding(1)
var waves_tex: texture_2d_array<f32>;

@group(0) @binding(2)
var depth_tex: texture_2d_array<f32>;

@group(0) @binding(3)
var dyn_tex: texture_2d_array<f32>;

@group(0) @binding(4)
var out_foam: texture_storage_2d_array<r16float, write>;

@group(0) @binding(5)
var<uniform> uni: FoamUniforms;

@group(0) @binding(6)
var<storage, read> headers: array<vec4<f32>>;

@group(0) @binding(7)
var<storage, read> points: array<vec4<f32>>;

@group(0) @binding(8)
var<uniform> lods: LodUniforms;

@compute @workgroup_size(8, 8, 1)
fn foam(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= uni.resolution || gid.y >= uni.resolution || gid.z >= uni.lod_count) {
        return;
    }
    let layer = i32(gid.z);
    let res_i = i32(uni.resolution);
    let here = vec2<i32>(i32(gid.x), i32(gid.y));
    let shift = vec2<i32>(i32(uni.shifts[gid.z].x), i32(uni.shifts[gid.z].y));
    let prev_coord = vec2<i32>(
        (here.x + shift.x % res_i + res_i) % res_i,
        (here.y + shift.y % res_i + res_i) % res_i,
    );
    let t = lods.lods[gid.z];
    let world_xz = t.origin.xz + (vec2<f32>(f32(gid.x), f32(gid.y)) + vec2<f32>(0.5)) * t.texel_size;

    let prev = textureLoad(prev_tex, prev_coord, layer, 0).x;
    let det = textureLoad(waves_tex, here, layer, 0).w;
    let depth = textureLoad(depth_tex, here, layer, 0).x;
    let dyn = textureLoad(dyn_tex, here, layer, 0).x;
    let dt = uni.dt;

    var source = 5.0 * uni.wave_gain * clamp(uni.wave_folding_threshold - det, 0.0, 1.0);
    source = source + uni.shoreline_gain * clamp(1.0 - depth / max(uni.shoreline_depth, 0.0001), 0.0, 1.0);
    source = source + uni.dynamic_gain * clamp(dyn, 0.0, 1.0);

    var foam = prev * max(0.0, 1.0 - uni.decay * dt) + source * dt;

    for (var i: u32 = 0u; i < uni.spline_count; i = i + 1u) {
        let header = headers[i];
        let offset = i32(header.x);
        let count = i32(header.y);
        let width = max(header.z, 0.001);
        let amount = header.w;
        var best_dist_sq = 1e30;
        for (var j: i32 = 0; j < count - 1; j = j + 1) {
            let a = points[offset + j].xz;
            let b = points[offset + j + 1].xz;
            let seg = b - a;
            let seg_len_sq = dot(seg, seg);
            if (seg_len_sq <= 0.0) {
                continue;
            }
            let t = clamp(dot(world_xz - a, seg) / seg_len_sq, 0.0, 1.0);
            let to_point = world_xz - (a + seg * t);
            best_dist_sq = min(best_dist_sq, dot(to_point, to_point));
        }
        if (best_dist_sq < 1e29) {
            let edge = clamp(sqrt(best_dist_sq) / width, 0.0, 1.0);
            foam = foam + amount * dt * (1.0 - edge * edge * (3.0 - 2.0 * edge));
        }
    }
    textureStore(out_foam, here, layer, vec4<f32>(clamp(foam, 0.0, 1.0), 0.0, 0.0, 0.0));
}
