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

struct DynWavesUniforms {
    dt: f32,
    wave_speed: f32,
    damping: f32,
    resolution: u32,
    emitter_count: u32,
    lod_count: u32,
    base_texel_size: f32,
    pad: f32,
    shifts: array<vec4<f32>, 8>,
}

@group(0) @binding(0)
var src_tex: texture_2d_array<f32>;

@group(0) @binding(1)
var out_tex: texture_storage_2d_array<rg32float, write>;

@group(0) @binding(2)
var<uniform> uni: DynWavesUniforms;

@group(0) @binding(3)
var<storage, read> emitters: array<vec4<f32>>;

@group(0) @binding(4)
var<storage, read> emitter_velocities: array<vec4<f32>>;

@group(0) @binding(5)
var<uniform> lods: LodUniforms;

fn interaction_falloff(a: f32, x: f32) -> f32 {
    let ax = a * x;
    let ax2 = ax * ax;
    let ax4 = ax2 * ax2;
    return ax / (1.0 + ax2 * ax4);
}

@compute @workgroup_size(8, 8, 1)
fn dynwaves_update(@builtin(global_invocation_id) gid: vec3<u32>) {
    let res = uni.resolution;
    if (gid.x >= res || gid.y >= res || gid.z >= uni.lod_count) {
        return;
    }
    let layer = i32(gid.z);
    let res_i = i32(res);
    let shift = vec2<i32>(i32(uni.shifts[gid.z].x), i32(uni.shifts[gid.z].y));
    let base = vec2<i32>(
        (i32(gid.x) + shift.x % res_i + res_i) % res_i,
        (i32(gid.y) + shift.y % res_i + res_i) % res_i,
    );
    let here = vec2<i32>(i32(gid.x), i32(gid.y));
    let xm = vec2<i32>((base.x - 1 + res_i) % res_i, base.y);
    let xp = vec2<i32>((base.x + 1) % res_i, base.y);
    let ym = vec2<i32>(base.x, (base.y - 1 + res_i) % res_i);
    let yp = vec2<i32>(base.x, (base.y + 1) % res_i);
    let texel = uni.base_texel_size * f32(1u << gid.z);
    let dt = uni.dt;

    let ft_v = textureLoad(src_tex, base, layer, 0).xy;
    let ft = ft_v.x;
    let vt = ft_v.y;

    let coeff = dt * uni.wave_speed * uni.wave_speed / (texel * texel);
    let laplacian = textureLoad(src_tex, xm, layer, 0).x
        + textureLoad(src_tex, xp, layer, 0).x
        + textureLoad(src_tex, ym, layer, 0).x
        + textureLoad(src_tex, yp, layer, 0).x
        - 4.0 * ft;

    let u = (f32(gid.x) + 0.5) / f32(res);
    let v = (f32(gid.y) + 0.5) / f32(res);
    let dx = min(u, 1.0 - u);
    let dy = min(v, 1.0 - v);
    let edge = mix(0.95, 1.0, clamp(min(dx, dy) / 0.1, 0.0, 1.0));

    var v_next = (vt + coeff * laplacian) * (1.0 - min(1.0, uni.damping * dt)) * edge;
    var h_next = (ft + dt * v_next) * edge;
    if (!(abs(h_next) < 1e30) || !(abs(v_next) < 1e30)) {
        h_next = 0.0;
        v_next = 0.0;
    }
    textureStore(out_tex, here, layer, vec4<f32>(h_next, v_next, 0.0, 0.0));
}

@compute @workgroup_size(8, 8, 1)
fn dynwaves_splat(@builtin(global_invocation_id) gid: vec3<u32>) {
    let res = uni.resolution;
    if (gid.x >= res || gid.y >= res || gid.z >= uni.lod_count) {
        return;
    }
    let layer = i32(gid.z);
    let here = vec2<i32>(i32(gid.x), i32(gid.y));
    let t = lods.lods[gid.z];
    let world_xz = t.origin.xz + (vec2<f32>(f32(gid.x), f32(gid.y)) + vec2<f32>(0.5)) * t.texel_size;

    var value = textureLoad(src_tex, here, layer, 0).xy;
    for (var i: u32 = 0u; i < uni.emitter_count; i = i + 1u) {
        let e = emitters[i];
        let v = emitter_velocities[i];
        if (e.z <= 0.0) {
            continue;
        }
        let a = 1.67 / (2.0 * e.z);
        let dist = length(world_xz - e.xy);
        if (dist < e.z + 3.0 / a) {
            let w = interaction_falloff(a, e.z - dist);
            value.x = value.x + e.w * w;
            value.y = value.y + v.w * w;
        }
    }
    textureStore(out_tex, here, layer, vec4<f32>(value, 0.0, 0.0));
}
