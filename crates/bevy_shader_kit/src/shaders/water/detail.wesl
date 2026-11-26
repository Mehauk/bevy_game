import package::shaders::noise::value2d::value_noise_2d;

struct WaterDetail {
    drift: vec2<f32>,
    detail_scale: f32,
    flow_scale: f32,
    flow_speed: f32,
    flow_strength: f32,
    layer_ratio: f32,
    gradient_step: f32,
    blend_a: f32,
    blend_b: f32,
    normal_gain: f32,
    foam_gain: f32,
    pad0: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(105)
var<uniform> detail: WaterDetail;

fn detail_flow(p: vec2<f32>, time: f32) -> vec2<f32> {
    let t = time * detail.flow_speed;
    let q = p * detail.flow_scale;
    let u = value_noise_2d(q + vec2<f32>(t, -t * 0.6));
    let v = value_noise_2d(q * 1.7 - vec2<f32>(t * 0.8, t));
    return (vec2<f32>(u, v) - vec2<f32>(0.5)) * 2.0 * detail.flow_strength;
}

fn noise_gradient(p: vec2<f32>, step: f32) -> vec2<f32> {
    let h = max(step, 1.0e-3);
    let c = value_noise_2d(p);
    let dx = value_noise_2d(p + vec2<f32>(h, 0.0)) - c;
    let dz = value_noise_2d(p + vec2<f32>(0.0, h)) - c;
    return vec2<f32>(dx, dz) / h;
}

fn detail_slope(p: vec2<f32>, time: f32) -> vec2<f32> {
    let scrolled = p * detail.detail_scale + detail_flow(p, time);
    let travel = detail.drift * time;
    let slow = noise_gradient(scrolled - travel * 2.0, detail.gradient_step) * detail.blend_a;
    let fast = noise_gradient(scrolled * detail.layer_ratio + travel, detail.gradient_step / max(detail.layer_ratio, 1.0e-3)) * detail.blend_b * detail.layer_ratio;
    return slow + fast;
}

fn detail_foam(slope: vec2<f32>) -> f32 {
    return clamp(length(slope) * detail.foam_gain, 0.0, 1.0);
}
