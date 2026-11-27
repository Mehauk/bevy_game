const PI2_C: f32 = 6.28318530717;

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

struct GerstnerUniform {
    wavelength: f32,
    amplitude: f32,
    speed: f32,
    steepness: f32,
    direction: vec2<f32>,
    phase: f32,
    chop: f32,
}

struct CombineUniforms {
    attenuation_in_shallows: f32,
    shallows_max_depth: f32,
    gerstner_count: u32,
    base_texel_size: f32,
    dynamic_gain: f32,
    average_wavelength: f32,
    fft_world: f32,
    pad2: f32,
    gerstner: array<GerstnerUniform, 16>,
}

@group(0) @binding(0)
var fft_disp_tex: texture_2d_array<f32>;

@group(0) @binding(1)
var fft_derivs_tex: texture_2d_array<f32>;

@group(0) @binding(2)
var depth_tex: texture_2d_array<f32>;

@group(0) @binding(3)
var dyn_tex: texture_2d_array<f32>;

@group(0) @binding(4)
var out_waves: texture_storage_2d_array<rgba16float, write>;

@group(0) @binding(5)
var out_derivs: texture_storage_2d_array<rg32float, write>;

@group(0) @binding(6)
var<uniform> lods: LodUniforms;

@group(0) @binding(7)
var<uniform> comb: CombineUniforms;

fn gerstner_dir(g: GerstnerUniform) -> vec2<f32> {
    if (length(g.direction) > 0.0) {
        return normalize(g.direction);
    }
    return vec2<f32>(1.0, 0.0);
}

fn gerstner_count() -> u32 {
    return max(comb.gerstner_count, 1u);
}

fn gerstner_field(world_xz: vec2<f32>, time: f32) -> vec4<f32> {
    var disp = vec3<f32>(0.0);
    var jac_a = 0.0;
    var jac_b = 0.0;
    var jac_c = 0.0;
    for (var i: u32 = 0u; i < comb.gerstner_count; i = i + 1u) {
        let g = comb.gerstner[i];
        if (g.amplitude <= 0.0 || g.wavelength <= 0.0) {
            continue;
        }
        let dir = gerstner_dir(g);
        let k = PI2_C / g.wavelength;
        let angle = k * dot(dir, world_xz) - g.speed * k * time + g.phase;
        let s = sin(angle);
        let c = cos(angle);
        let q = clamp(g.steepness, 0.0, 1.0) / (k * g.amplitude * f32(gerstner_count()));
        let shift = q * g.amplitude * s;
        disp.x = disp.x - dir.x * shift;
        disp.z = disp.z - dir.y * shift;
        disp.y = disp.y + g.amplitude * c;
        let qa = q * k * g.amplitude * c;
        jac_a = jac_a + qa * dir.x * dir.x;
        jac_b = jac_b + qa * dir.x * dir.y;
        jac_c = jac_c + qa * dir.y * dir.y;
    }
    let jac = (1.0 - jac_a) * (1.0 - jac_c) - jac_b * jac_b;
    return vec4<f32>(disp, jac);
}

fn gerstner_gradient(world_xz: vec2<f32>, time: f32) -> vec2<f32> {
    var dydx = 0.0;
    var dydz = 0.0;
    for (var i: u32 = 0u; i < comb.gerstner_count; i = i + 1u) {
        let g = comb.gerstner[i];
        if (g.amplitude <= 0.0 || g.wavelength <= 0.0) {
            continue;
        }
        let dir = gerstner_dir(g);
        let k = PI2_C / g.wavelength;
        let angle = k * dot(dir, world_xz) - g.speed * k * time + g.phase;
        let s = sin(angle);
        dydx = dydx - g.amplitude * k * s * dir.x;
        dydz = dydz - g.amplitude * k * s * dir.y;
    }
    return vec2<f32>(dydx, dydz);
}

fn sample_bilinear(tex: texture_2d_array<f32>, uv: vec2<f32>, res: i32, layer: i32) -> vec4<f32> {
    var t = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)) * f32(res) - vec2<f32>(0.5);
    t = max(t, vec2<f32>(0.0));
    let t0 = floor(t);
    let i0 = vec2<i32>(i32(t0.x), i32(t0.y));
    let i1 = min(i0 + vec2<i32>(1, 1), vec2<i32>(res - 1, res - 1));
    let f = t - t0;
    let a = textureLoad(tex, i0, layer, 0);
    let b = textureLoad(tex, vec2<i32>(i1.x, i0.y), layer, 0);
    let c = textureLoad(tex, vec2<i32>(i0.x, i1.y), layer, 0);
    let d = textureLoad(tex, i1, layer, 0);
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

fn tile_axis(u: f32, size: i32) -> vec3<f32> {
    var t = fract(u) * f32(size) - 0.5;
    if (t < 0.0) {
        t = t + f32(size);
    }
    let t0 = floor(t);
    let i0 = i32(t0) % size;
    let i1 = (i0 + 1) % size;
    return vec3<f32>(f32(i0), f32(i1), t - t0);
}

@compute @workgroup_size(8, 8, 1)
fn shape_combine(@builtin(global_invocation_id) gid: vec3<u32>) {
    let lod = gid.z;
    if (lod >= lods.lod_count || gid.x >= lods.resolution || gid.y >= lods.resolution) {
        return;
    }
    let t = lods.lods[lod];
    let layer = i32(lod);
    let size = i32(lods.resolution);
    let world_xz = t.origin.xz + (vec2<f32>(f32(gid.x), f32(gid.y)) + vec2<f32>(0.5)) * t.texel_size;

    let fft_world = comb.fft_world * exp2(f32(lod));
    let ux = tile_axis(world_xz.x / fft_world, size);
    let uy = tile_axis(world_xz.y / fft_world, size);
    let x0 = vec2<i32>(i32(ux.x), i32(uy.x));
    let x1 = vec2<i32>(i32(ux.y), i32(uy.x));
    let x2 = vec2<i32>(i32(ux.x), i32(uy.y));
    let x3 = vec2<i32>(i32(ux.y), i32(uy.y));
    let fu = ux.z;
    let fv = uy.z;

    let d00 = textureLoad(fft_disp_tex, x0, layer, 0);
    let d10 = textureLoad(fft_disp_tex, x1, layer, 0);
    let d01 = textureLoad(fft_disp_tex, x2, layer, 0);
    let d11 = textureLoad(fft_disp_tex, x3, layer, 0);
    let disp0 = mix(mix(d00, d10, fu), mix(d01, d11, fu), fv);

    let r00 = textureLoad(fft_derivs_tex, x0, layer, 0);
    let r10 = textureLoad(fft_derivs_tex, x1, layer, 0);
    let r01 = textureLoad(fft_derivs_tex, x2, layer, 0);
    let r11 = textureLoad(fft_derivs_tex, x3, layer, 0);
    let derivs0 = mix(mix(r00, r10, fu), mix(r01, r11, fu), fv);

    let res_i = i32(lods.resolution);
    let cascade_uv = (world_xz - t.origin.xz) / t.extent;
    let depth = sample_bilinear(depth_tex, cascade_uv, res_i, layer).x;
    var weight = clamp(depth / max(comb.shallows_max_depth, 0.0001), 0.0, 1.0);
    if (comb.shallows_max_depth <= 0.0) {
        weight = 1.0;
    }
    let atten = comb.attenuation_in_shallows * weight + (1.0 - comb.attenuation_in_shallows);

    let g = gerstner_field(world_xz, lods.time);
    let grad = gerstner_gradient(world_xz, lods.time);

    var disp = disp0.xyz * atten + g.xyz;
    let jac = disp0.w * g.w;
    var derivs = derivs0.xy + grad;

    let dyn = sample_bilinear(dyn_tex, cascade_uv, res_i, layer);
    disp.y = disp.y + dyn.x * comb.dynamic_gain;

    let coord3 = vec2<i32>(i32(gid.x), i32(gid.y));
    textureStore(out_waves, coord3, layer, vec4<f32>(disp, jac));
    textureStore(out_derivs, coord3, layer, vec4<f32>(derivs.x, derivs.y, 0.0, 0.0));
}
