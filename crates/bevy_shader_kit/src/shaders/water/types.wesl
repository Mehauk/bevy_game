import package::shaders::water::optics_types::WaterOptics;

struct WaterParams {
    time: f32,
    water_height: f32,
    foam_width: f32,
    foam_strength: f32,
    ripple_normal: f32,
    ripple_foam: f32,
    wave_energy: f32,
    refraction: f32,
    optics: WaterOptics,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> water: WaterParams;

struct WaterContext {
    p: vec3<f32>,
    view_fade: f32,
    depth: f32,
    edge: f32,
    pattern: f32,
    foam: f32,
    ripple: vec2<f32>,
    detail: vec2<f32>,
    slope: vec2<f32>,
    blend: f32,
    tint: f32,
}
