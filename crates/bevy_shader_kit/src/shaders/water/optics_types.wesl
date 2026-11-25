struct WaterOptics {
    scattering: vec3<f32>,
    absorption: vec3<f32>,
    sun_direction: vec3<f32>,
    sun_illuminance: vec3<f32>,
    ambient_illuminance: vec3<f32>,
    max_depth: f32,
    phase_g: f32,
    pad0: f32,
    pad1: f32,
}
