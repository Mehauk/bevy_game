import package::shaders::shading::hemisphere::hemisphere_mix;

fn shore_edge_mask(depth: f32, foam_width: f32) -> f32 {
    return max(1.0 - depth / foam_width, 0.0);
}

fn proximity_edge_mask(distance: f32, foam_width: f32, emerged: f32) -> f32 {
    return max(1.0 - distance / foam_width, 0.0) * emerged;
}

fn foam_noise_mask(edge: f32, pattern: f32, strength: f32, view_fade: f32) -> f32 {
    return clamp(strength * edge * edge * (0.15 + 0.85 * pattern) * view_fade, 0.0, 1.0);
}

fn apply_foam_roughness(roughness: f32, foam: f32) -> f32 {
    return clamp(roughness + 0.6 * foam, 0.0, 1.0);
}

fn blend_foam_color(color: vec3<f32>, normal: vec3<f32>, foam: f32) -> vec3<f32> {
    let foam_color = hemisphere_mix(vec3<f32>(0.45), vec3<f32>(0.9), normal);
    return mix(color, foam_color, foam);
}
