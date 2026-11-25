import package::shaders::noise::value2d::value_noise_2d;

fn scrolling_fbm_2d(p: vec2<f32>, time: f32) -> f32 {
    let t = time * 0.12;
    let a = value_noise_2d(p + vec2<f32>(t, t * 0.7));
    let b = value_noise_2d(p * 2.3 - vec2<f32>(t * 0.8, t));
    let c = value_noise_2d(p * 0.5 + vec2<f32>(-t * 0.5, t * 0.4));
    let n = a * 0.55 + b * 0.2 + c * 0.25;
    return clamp((n - 0.4) * 2.8, 0.0, 1.0);
}
