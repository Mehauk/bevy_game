import package::shaders::sdf::volume::SdfVolume;

@group(0) @binding(0)
var bake_out: texture_storage_3d<r32float, write>;

@group(0) @binding(1)
var<uniform> bake_volume: SdfVolume;

@group(0) @binding(2)
var<storage, read> bake_vertices: array<vec4<f32>>;

@group(0) @binding(3)
var<storage, read> bake_indices: array<u32>;

fn closest_point_on_triangle(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, c: vec3<f32>) -> vec3<f32> {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if (d1 <= 0.0 && d2 <= 0.0) {
        return a;
    }

    let bq = p - b;
    let d3 = dot(ab, bq);
    let d4 = dot(ac, bq);
    if (d3 >= 0.0 && d4 <= d3) {
        return b;
    }

    let vc = d1 * d4 - d3 * d2;
    if (vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0) {
        return a + ab * (d1 / (d1 - d3));
    }

    let cq = p - c;
    let d5 = dot(ab, cq);
    let d6 = dot(ac, cq);
    if (d6 >= 0.0 && d5 <= d6) {
        return c;
    }

    let vb = d5 * d2 - d1 * d6;
    if (vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0) {
        return a + ac * (d2 / (d2 - d6));
    }

    let va = d3 * d6 - d5 * d4;
    if (va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0) {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }

    let inv = 1.0 / (va + vb + vc);
    return a + ab * (vb * inv) + ac * (vc * inv);
}

@compute @workgroup_size(4, 4, 4)
fn bake_sdf(@builtin(global_invocation_id) gid: vec3<u32>) {
    let res = vec3<u32>(bake_volume.resolution);
    if (gid.x >= res.x || gid.y >= res.y || gid.z >= res.z) {
        return;
    }

    let texel_size = (bake_volume.bounds_max - bake_volume.bounds_min) / bake_volume.resolution;
    let p = bake_volume.bounds_min + (vec3<f32>(gid) + vec3<f32>(0.5)) * texel_size;
    let tri_count = arrayLength(&bake_indices) / 3u;

    var best_d2 = 1.0e30;
    var best_sign = 1.0;
    for (var t: u32 = 0u; t < tri_count; t = t + 1u) {
        let a = bake_vertices[bake_indices[t * 3u]].xyz;
        let b = bake_vertices[bake_indices[t * 3u + 1u]].xyz;
        let c = bake_vertices[bake_indices[t * 3u + 2u]].xyz;
        let q = closest_point_on_triangle(p, a, b, c);
        let d = p - q;
        let d2 = dot(d, d);
        if (d2 < best_d2) {
            best_d2 = d2;
            if (dot(d, cross(b - a, c - a)) < 0.0) {
                best_sign = -1.0;
            } else {
                best_sign = 1.0;
            }
        }
    }

    textureStore(bake_out, gid, vec4<f32>(sqrt(best_d2) * best_sign, 0.0, 0.0, 0.0));
}
