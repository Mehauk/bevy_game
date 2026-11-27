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

struct FlowUniforms {
    spline_count: u32,
    resolution: u32,
    lod_count: u32,
    pad: u32,
}

@group(0) @binding(0)
var out_flow: texture_storage_2d_array<rg32float, write>;

@group(0) @binding(1)
var<uniform> uni: FlowUniforms;

@group(0) @binding(2)
var<storage, read> headers: array<vec4<f32>>;

@group(0) @binding(3)
var<storage, read> points: array<vec4<f32>>;

@group(0) @binding(4)
var<uniform> lods: LodUniforms;

@compute @workgroup_size(8, 8, 1)
fn flow(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= uni.resolution || gid.y >= uni.resolution || gid.z >= uni.lod_count) {
        return;
    }
    let layer = i32(gid.z);
    let t = lods.lods[gid.z];
    let world_xz = t.origin.xz + (vec2<f32>(f32(gid.x), f32(gid.y)) + vec2<f32>(0.5)) * t.texel_size;
    var flow = vec2<f32>(0.0);

    for (var i: u32 = 0u; i < uni.spline_count; i = i + 1u) {
        let header = headers[i];
        let offset = i32(header.x);
        let count = i32(header.y);
        let width = header.z;
        let speed = header.w;
        if (count < 2 || width <= 0.0) {
            continue;
        }
        var best_dist_sq = 1e30;
        var best_dir = vec2<f32>(0.0);
        for (var j: i32 = 0; j < count - 1; j = j + 1) {
            let a = points[offset + j].xz;
            let b = points[offset + j + 1].xz;
            let seg = b - a;
            let seg_len_sq = dot(seg, seg);
            if (seg_len_sq <= 0.0) {
                continue;
            }
            let t = clamp(dot(world_xz - a, seg) / seg_len_sq, 0.0, 1.0);
            let closest = a + seg * t;
            let to_point = world_xz - closest;
            let dist_sq = dot(to_point, to_point);
            if (dist_sq < best_dist_sq) {
                best_dist_sq = dist_sq;
                best_dir = seg / sqrt(seg_len_sq);
            }
        }
        let dist = sqrt(best_dist_sq);
        let edge = clamp(dist / width, 0.0, 1.0);
        let falloff = 1.0 - edge * edge * (3.0 - 2.0 * edge);
        flow = flow + best_dir * (speed * falloff);
    }
    textureStore(out_flow, vec2<i32>(i32(gid.x), i32(gid.y)), layer, vec4<f32>(flow, 0.0, 0.0));
}
