struct LodTransform {
    origin: vec3<f32>,
    texel_size: f32,
    extent: f32,
    lod_index: f32,
    pad: f32,
}

struct WaterQueryUniforms {
    lods: array<LodTransform, 8>,
    count: u32,
    resolution: u32,
    lod_count: u32,
    pad: u32,
}

@group(0) @binding(0)
var waves_tex: texture_2d_array<f32>;

@group(0) @binding(1)
var flow_tex: texture_2d_array<f32>;

@group(0) @binding(2)
var depth_tex: texture_2d_array<f32>;

@group(0) @binding(3)
var derivs_tex: texture_2d_array<f32>;

@group(0) @binding(4)
var out_animated: texture_storage_2d<rgba32float, write>;

@group(0) @binding(5)
var out_flow: texture_storage_2d<rgba32float, write>;

@group(0) @binding(6)
var out_depth: texture_storage_2d<rgba32float, write>;

@group(0) @binding(7)
var<uniform> uni: WaterQueryUniforms;

@group(0) @binding(8)
var<storage, read> points: array<vec4<f32>>;

fn lod_for(world_xz: vec2<f32>) -> u32 {
    for (var lod: u32 = 0u; lod < uni.lod_count; lod = lod + 1u) {
        let t = uni.lods[lod];
        let uv = (world_xz - t.origin.xz) / max(t.extent, 0.0001);
        if (uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0) {
            return lod;
        }
    }
    return uni.lod_count - 1u;
}

@compute @workgroup_size(8, 1, 1)
fn water_query(@builtin(global_invocation_id) gid: vec3<u32>) {
    let index = gid.x;
    if (index >= uni.count) {
        return;
    }
    let world_xz = points[index].xy;
    let lod = lod_for(world_xz);
    let t = uni.lods[lod];
    let layer = i32(lod);
    let res = i32(uni.resolution);
    let uv = (world_xz - t.origin.xz) / max(t.extent, 0.0001);
    let coord = clamp(vec2<i32>(i32(uv.x * f32(res)), i32(uv.y * f32(res))), vec2<i32>(0), vec2<i32>(res - 1));
    let pos = vec2<i32>(i32(index), 0);
    let waves = textureLoad(waves_tex, coord, layer, 0);
    let flow = textureLoad(flow_tex, coord, layer, 0);
    let depth = textureLoad(depth_tex, coord, layer, 0);
    let derivs = textureLoad(derivs_tex, coord, layer, 0);
    textureStore(out_animated, pos, waves);
    textureStore(out_flow, pos, vec4<f32>(flow.x, flow.y, 0.0, 0.0));
    textureStore(out_depth, pos, vec4<f32>(depth.x, derivs.x, derivs.y, 0.0));
}
