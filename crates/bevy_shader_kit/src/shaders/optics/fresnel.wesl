fn fresnel_schlick(f0: f32, ndv: f32) -> f32 {
    return f0 + (1.0 - f0) * pow(1.0 - ndv, 5.0);
}
