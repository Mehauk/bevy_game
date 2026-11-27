struct FillUniforms {
    value: f32,
    resolution: u32,
    lod_count: u32,
    carve_count: u32,
}

@group(0) @binding(0)
var src_tex: texture_2d_array<f32>;

@group(0) @binding(1)
var out_tex: texture_storage_2d_array<r16float, write>;

@group(0) @binding(2)
var<uniform> uni: FillUniforms;

@compute @workgroup_size(8, 8, 1)
fn copy_foam(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= uni.resolution || gid.y >= uni.resolution || gid.z >= uni.lod_count) {
        return;
    }
    let coord = vec2<i32>(i32(gid.x), i32(gid.y));
    let value = textureLoad(src_tex, coord, i32(gid.z), 0).x;
    textureStore(out_tex, coord, i32(gid.z), vec4<f32>(value, 0.0, 0.0, 0.0));
}
