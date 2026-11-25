fn radial_contact_distance(p: vec3<f32>, surface: vec3<f32>, water_height: f32) -> f32 {
    let vertical = surface.y - water_height;
    let hd = length(vec2<f32>(p.x - surface.x, p.z - surface.z));
    return hd + abs(vertical);
}

fn ripple_wave_signal(distance: f32, frequency: f32, time: f32, speed: f32) -> f32 {
    return sin(distance * frequency - time * speed);
}

fn perturb_normal_horizontal(normal: vec3<f32>, outward: vec2<f32>, wave: f32, amplitude: f32) -> vec3<f32> {
    return normalize(normal + vec3<f32>(outward.x, 0.0, outward.y) * (wave * amplitude));
}
