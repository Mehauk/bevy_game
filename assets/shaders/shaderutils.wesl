const PI: f32 = 3.14159265;

fn hash21(p: vec2<f32>) -> f32 {
    let s = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(s) * 43758.5453123);
}

fn hash22(p: vec2<f32>) -> vec2<f32> {
    let sx = fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453123);
    let sy = fract(sin(dot(p, vec2<f32>(269.5, 183.3))) * 43758.5453123);
    return vec2<f32>(sx, sy);
}

fn hash33(p: vec3<f32>) -> vec3<f32> {
    let s = fract(sin(vec3<f32>(
        dot(p, vec3<f32>(127.1, 311.7, 74.7)),
        dot(p, vec3<f32>(269.5, 183.3, 246.1)),
        dot(p, vec3<f32>(113.5, 271.9, 124.6)),
    )) * 43758.5453123);
    return s;
}

fn smooth_noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn noise_and_grad(p: vec2<f32>) -> vec3<f32> {
    let e = 0.15;
    let v = smooth_noise2(p);
    let dx = smooth_noise2(p + vec2<f32>(e, 0.0)) - smooth_noise2(p - vec2<f32>(e, 0.0));
    let dy = smooth_noise2(p + vec2<f32>(0.0, e)) - smooth_noise2(p - vec2<f32>(0.0, e));
    return vec3<f32>(v, dx / (2.0 * e), dy / (2.0 * e));
}

fn fbm2(p: vec2<f32>, octaves: i32) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var norm = 0.0;
    var pos = p;
    for (var i: i32 = 0; i < octaves; i = i + 1) {
        sum = sum + amp * smooth_noise2(pos);
        norm = norm + amp;
        amp = amp * 0.5;
        pos = pos * 2.17 + vec2<f32>(17.0, 3.0);
    }
    return sum / max(norm, 1e-4);
}

fn schlick_fresnel(ndv: f32, f0: f32) -> f32 {
    return f0 + (1.0 - f0) * pow(1.0 - ndv, 5.0);
}

fn ggx_specular(ndh: f32, ndl: f32, ndv: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = ndh * ndh * (a2 - 1.0) + 1.0;
    let d2 = d * d;
    let vis = a2 / max(ndl * max(ndv * d2, 1e-4), 1e-4);
    return vis * 0.25;
}
