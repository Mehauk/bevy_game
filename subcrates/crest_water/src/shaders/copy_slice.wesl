struct CopySliceUniforms {
    lod: u32,
    resolution: u32,
    slice_size: u32,
    pad0: u32,
    slice_origin: vec2<f32>,
    lod_origin: vec2<f32>,
    texel: f32,
    extent: f32,
    pad1: f32,
    pad2: f32,
}

@group(0) @binding(0)
var src_tex: texture_2d_array<f32>;

@group(0) @binding(1)
var out_slice: texture_storage_2d<rgba32float, write>;

@group(0) @binding(2)
var<uniform> uni: CopySliceUniforms;

@compute @workgroup_size(8, 8, 1)
fn copy_slice(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= uni.slice_size || gid.y >= uni.slice_size) {
        return;
    }
    let world_xz = uni.slice_origin + (vec2<f32>(f32(gid.x), f32(gid.y)) + vec2<f32>(0.5)) * uni.texel;
    let uv = (world_xz - uni.lod_origin) / max(uni.extent, 0.0001);
    let res = i32(uni.resolution);
    let coord = clamp(vec2<i32>(i32(uv.x * f32(res)), i32(uv.y * f32(res))), vec2<i32>(0), vec2<i32>(res - 1));
    let value = textureLoad(src_tex, coord, i32(uni.lod), 0);
    textureStore(out_slice, vec2<i32>(i32(gid.x), i32(gid.y)), value);
}
