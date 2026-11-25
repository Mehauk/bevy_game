fn debug_region_color(id: u32) -> vec3<f32> {
    var h: u32 = id * 0x9E3779B9u;
    h = (h ^ (h >> 15u)) * 0x85EBCA6Bu;
    h = (h ^ (h >> 13u)) * 0xC2B2AE35u;
    h = h ^ (h >> 16u);
    let r = f32(h & 0xFFu) / 255.0;
    let g = f32((h >> 8u) & 0xFFu) / 255.0;
    let b = f32((h >> 16u) & 0xFFu) / 255.0;
    return vec3<f32>(0.35, 0.35, 0.35) + vec3<f32>(0.65, 0.65, 0.65) * vec3<f32>(r, g, b);
}
