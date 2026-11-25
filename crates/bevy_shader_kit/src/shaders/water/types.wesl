struct WaterParams {
    time: f32,
    water_height: f32,
    foam_width: f32,
    foam_strength: f32,
    max_depth: f32,
    phase_g: f32,
    ripple_normal: f32,
    ripple_foam: f32,
    scattering: vec3<f32>,
    absorption: vec3<f32>,
    sun_direction: vec3<f32>,
    sun_illuminance: vec3<f32>,
    ambient_illuminance: vec3<f32>,
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
}
