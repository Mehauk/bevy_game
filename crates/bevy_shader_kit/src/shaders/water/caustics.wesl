import package::shaders::noise::value2d::value_noise_2d;

struct WaterCaustics {
    strength: f32,
    scale: f32,
    speed: f32,
    sharpness: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(107)
var<uniform> caustics: WaterCaustics;

fn caustic_pattern(p: vec2<f32>, time: f32) -> f32 {
    let q = p / max(caustics.scale, 1.0e-3);
    let t = time * caustics.speed;
    let a = value_noise_2d(q + vec2<f32>(t, t * 0.4));
    let b = value_noise_2d(q * 1.618 - vec2<f32>(t * 0.7, -t * 0.3));
    let warp = (vec2<f32>(a, b) - vec2<f32>(0.5)) * 1.5;
    let c = value_noise_2d(q + warp + vec2<f32>(-t * 0.5, t * 0.6));
    let ridge = clamp(1.0 - abs(c * 2.0 - 1.0), 0.0, 1.0);
    return pow(ridge, max(caustics.sharpness, 1.0));
}

fn caustic_gain(pattern: f32, blend: f32) -> f32 {
    return pow(pattern, 1.5) * (1.0 - blend) * caustics.strength;
}
