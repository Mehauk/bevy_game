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

struct FillUniforms {
    value: f32,
    resolution: u32,
    lod_count: u32,
    carve_count: u32,
}

@group(0) @binding(0)
var out_tex: texture_storage_2d_array<r8unorm, write>;

@group(0) @binding(1)
var<uniform> uni: FillUniforms;

@group(0) @binding(2)
var<storage, read> carves: array<vec4<f32>>;

@group(0) @binding(3)
var<uniform> lods: LodUniforms;

@compute @workgroup_size(8, 8, 1)
fn fill(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= uni.resolution || gid.y >= uni.resolution || gid.z >= uni.lod_count) {
        return;
    }
    let layer = i32(gid.z);
    let t = lods.lods[gid.z];
    let world_xz = t.origin.xz + (vec2<f32>(f32(gid.x), f32(gid.y)) + vec2<f32>(0.5)) * t.texel_size;
    var value = uni.value;
    for (var i: u32 = 0u; i < uni.carve_count; i = i + 1u) {
        let carve = carves[i];
        if (length(world_xz - carve.xy) < carve.z) {
            value = 0.0;
        }
    }
    textureStore(out_tex, vec2<i32>(i32(gid.x), i32(gid.y)), layer, vec4<f32>(value, 0.0, 0.0, 0.0));
}
