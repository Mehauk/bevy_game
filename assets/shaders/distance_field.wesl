fn df_texel(sdf: texture_3d<f32>, res: vec3<f32>, i: vec3<i32>) -> f32 {
    let c = clamp(i, vec3<i32>(0, 0, 0), vec3<i32>(i32(res.x) - 1, i32(res.y) - 1, i32(res.z) - 1));
    return textureLoad(sdf, c, 0).r;
}

fn df_sample(sdf: texture_3d<f32>, res: vec3<f32>, uvw: vec3<f32>) -> f32 {
    let p = clamp(uvw, vec3<f32>(0.0), vec3<f32>(1.0)) * res - vec3<f32>(0.5);
    let base = vec3<i32>(floor(p));
    let f = p - vec3<f32>(base);
    let c000 = df_texel(sdf, res, base);
    let c100 = df_texel(sdf, res, base + vec3<i32>(1, 0, 0));
    let c010 = df_texel(sdf, res, base + vec3<i32>(0, 1, 0));
    let c110 = df_texel(sdf, res, base + vec3<i32>(1, 1, 0));
    let c001 = df_texel(sdf, res, base + vec3<i32>(0, 0, 1));
    let c101 = df_texel(sdf, res, base + vec3<i32>(1, 0, 1));
    let c011 = df_texel(sdf, res, base + vec3<i32>(0, 1, 1));
    let c111 = df_texel(sdf, res, base + vec3<i32>(1, 1, 1));
    let x0 = mix(c000, c100, f.x);
    let x1 = mix(c010, c110, f.x);
    let x2 = mix(c001, c101, f.x);
    let x3 = mix(c011, c111, f.x);
    let y0 = mix(x0, x1, f.y);
    let y1 = mix(x2, x3, f.y);
    return mix(y0, y1, f.z);
}

fn df_gradient(sdf: texture_3d<f32>, res: vec3<f32>, uvw: vec3<f32>, world_per_texel: vec3<f32>) -> vec3<f32> {
    let half_texel = 0.5 / res;
    let r = df_sample(sdf, res, uvw + vec3<f32>(half_texel.x, 0.0, 0.0));
    let l = df_sample(sdf, res, uvw - vec3<f32>(half_texel.x, 0.0, 0.0));
    let f = df_sample(sdf, res, uvw + vec3<f32>(0.0, half_texel.y, 0.0));
    let b = df_sample(sdf, res, uvw - vec3<f32>(0.0, half_texel.y, 0.0));
    let u = df_sample(sdf, res, uvw + vec3<f32>(0.0, 0.0, half_texel.z));
    let d = df_sample(sdf, res, uvw - vec3<f32>(0.0, 0.0, half_texel.z));
    return vec3<f32>((r - l) / world_per_texel.x, (f - b) / world_per_texel.y, (u - d) / world_per_texel.z);
}
