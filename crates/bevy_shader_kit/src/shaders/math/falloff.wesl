fn linear_falloff(distance: f32, range: f32) -> f32 {
    return max(1.0 - distance / range, 0.0);
}

fn quadratic_falloff(distance: f32, range: f32) -> f32 {
    let f = linear_falloff(distance, range);
    return f * f;
}

fn view_distance_fade(point: vec3<f32>, camera: vec3<f32>, fade_range: f32) -> f32 {
    return clamp(1.0 - length(point - camera) / fade_range, 0.0, 1.0);
}
