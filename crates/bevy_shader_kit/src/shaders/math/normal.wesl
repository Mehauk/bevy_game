fn tilt_normal(normal: vec3<f32>, slope: vec2<f32>) -> vec3<f32> {
    return normalize(normal - vec3<f32>(slope.x, 0.0, slope.y));
}
