import package::shaders::crest_math::cmul;

const PI2_F: f32 = 6.28318530717;
const FFT_SIZE: u32 = 64u;

struct FftUniforms {
    size: u32,
    axis: u32,
    inverse: u32,
    pad: u32,
}

@group(0) @binding(0)
var src: texture_2d_array<f32>;

@group(0) @binding(1)
var dst: texture_storage_2d_array<rg32float, write>;

@group(0) @binding(2)
var<uniform> uni: FftUniforms;

var<workgroup> line: array<vec2<f32>, 64>;

fn reverse_bits(value: u32) -> u32 {
    var v = value;
    var r: u32 = 0u;
    for (var b: u32 = 0u; b < 6u; b = b + 1u) {
        r = r * 2u + v % 2u;
        v = v / 2u;
    }
    return r;
}

@compute @workgroup_size(64, 1, 1)
fn fft(
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(local_invocation_index) lid: u32,
) {
    let layer = wg.x;
    let line_index = wg.y;
    for (var i: u32 = 0u; i < FFT_SIZE; i = i + 1u) {
        let coord = select(vec2<u32>(i, line_index), vec2<u32>(line_index, i), uni.axis == 1u);
        line[i] = textureLoad(src, vec2<i32>(vec2<u32>(coord)), i32(layer), 0).xy;
    }
    workgroupBarrier();

    let rev = reverse_bits(lid);
    if (rev > lid) {
        let tmp = line[lid];
        line[lid] = line[rev];
        line[rev] = tmp;
    }
    workgroupBarrier();

    let sign = select(-1.0, 1.0, uni.inverse == 1u);
    var len: u32 = 2u;
    while (len <= FFT_SIZE) {
        let half = len / 2u;
        var t: u32 = lid;
        while (t < FFT_SIZE / 2u) {
            let g = t / half;
            let o = t % half;
            let i0 = g * len + o;
            let i1 = i0 + half;
            let angle = sign * PI2_F * f32(o) / f32(len);
            let tw = vec2<f32>(cos(angle), sin(angle));
            let a = line[i0];
            let b = cmul(tw, line[i1]);
            line[i0] = a + b;
            line[i1] = a - b;
            t = t + FFT_SIZE / 2u;
        }
        workgroupBarrier();
        len = len * 2u;
    }

    for (var i: u32 = 0u; i < FFT_SIZE; i = i + 1u) {
        let coord = select(vec2<u32>(i, line_index), vec2<u32>(line_index, i), uni.axis == 1u);
        textureStore(dst, vec2<i32>(vec2<u32>(coord)), i32(layer), vec4<f32>(line[i], 0.0, 0.0));
    }
}
