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

struct DepthUniforms {
    grid_origin: vec2<f32>,
    grid_texel: f32,
    grid_size: u32,
    water_level: f32,
    resolution: u32,
    lod_count: u32,
}

@group(0) @binding(0)
var grid_tex: texture_2d<f32>;

@group(0) @binding(1)
var out_depth: texture_storage_2d_array<r16float, write>;

@group(0) @binding(2)
var<uniform> uni: DepthUniforms;

@group(0) @binding(3)
var<uniform> lods: LodUniforms;

fn grid_height(fx: f32, fy: f32) -> f32 {
    let size = i32(uni.grid_size);
    let x = clamp(i32(floor(fx)), 0, size - 1);
    let y = clamp(i32(floor(fy)), 0, size - 1);
    return textureLoad(grid_tex, vec2<i32>(x, y), 0).x;
}

@compute @workgroup_size(8, 8, 1)
fn seafloor_depth(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= uni.resolution || gid.y >= uni.resolution || gid.z >= uni.lod_count) {
        return;
    }
    let layer = i32(gid.z);
    let t = lods.lods[gid.z];
    let world_xz = t.origin.xz + (vec2<f32>(f32(gid.x), f32(gid.y)) + vec2<f32>(0.5)) * t.texel_size;
    var depth = 1000.0;
    if (uni.grid_size > 0u) {
        let f = (world_xz - uni.grid_origin) / uni.grid_texel - vec2<f32>(0.5);
        if (all(f >= vec2<f32>(-0.5)) && all(f <= vec2<f32>(f32(uni.grid_size) - 0.5))) {
            let x0 = floor(f.x);
            let y0 = floor(f.y);
            let tx = f.x - x0;
            let ty = f.y - y0;
            let h00 = grid_height(x0, y0);
            let h10 = grid_height(x0 + 1.0, y0);
            let h01 = grid_height(x0, y0 + 1.0);
            let h11 = grid_height(x0 + 1.0, y0 + 1.0);
            let height = mix(mix(h00, h10, tx), mix(h01, h11, tx), ty);
            depth = max(0.0, uni.water_level - height);
        }
    }
    textureStore(out_depth, vec2<i32>(i32(gid.x), i32(gid.y)), layer, vec4<f32>(depth, 0.0, 0.0, 0.0));
}
