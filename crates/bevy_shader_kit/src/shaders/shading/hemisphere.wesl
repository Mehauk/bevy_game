fn hemisphere_factor(n: vec3<f32>) -> f32 {
    return 0.5 + 0.5 * n.y;
}

fn hemisphere_mix(ground: vec3<f32>, sky: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    return mix(ground, sky, hemisphere_factor(n));
}
