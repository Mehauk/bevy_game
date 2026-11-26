struct WaterDye {
    shallow: vec3<f32>,
    deep: vec3<f32>,
    beers_law: f32,
    depth_offset: f32,
    tint_exponent: f32,
    pad0: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(106)
var<uniform> dye: WaterDye;

fn depth_blend(depth: f32) -> f32 {
    return clamp(1.0 - exp((depth + dye.depth_offset) * -dye.beers_law), 0.0, 1.0);
}

fn depth_tint(blend: f32) -> f32 {
    return clamp(pow(blend, dye.tint_exponent), 0.0, 1.0);
}

fn dye_color(tint: f32) -> vec3<f32> {
    return mix(dye.shallow, dye.deep, tint);
}

fn dyed_base_color(base: vec3<f32>, tint: f32) -> vec3<f32> {
    let color = dye_color(tint);
    return mix(base * color, color * 0.25, tint * 0.5);
}
