import package::shaders::sdf::volume::SdfVolume;

struct WaterParams {
    time: f32,
    water_height: f32,
    ripple_range: f32,
    ripple_frequency: f32,
    ripple_speed: f32,
    ripple_strength: f32,
    foam_width: f32,
    foam_strength: f32,
    field_count: u32,
    max_depth: f32,
    phase_g: f32,
    pad: f32,
    scattering: vec3<f32>,
    absorption: vec3<f32>,
    sun_direction: vec3<f32>,
    sun_illuminance: vec3<f32>,
    ambient_illuminance: vec3<f32>,
    fields: array<SdfVolume, 4>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> water: WaterParams;

@group(constants::MATERIAL_BIND_GROUP) @binding(101)
var water_field0: texture_3d<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(102)
var water_field1: texture_3d<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(103)
var water_field2: texture_3d<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(104)
var water_field3: texture_3d<f32>;

struct WaterContext {
    p: vec3<f32>,
    view_fade: f32,
    distance: f32,
    gradient: vec3<f32>,
    boundary: f32,
    depth: f32,
    point: vec3<f32>,
    outward: vec2<f32>,
    emerged: f32,
    ripple_distance: f32,
    edge: f32,
    pattern: f32,
    foam: f32,
}
