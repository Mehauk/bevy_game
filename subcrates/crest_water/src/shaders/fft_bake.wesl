struct BakeUniforms {
    size: u32,
    lod_count: u32,
    fft_world: f32,
    pad1: f32,
}

@group(0) @binding(0)
var height_tex: texture_2d_array<f32>;

@group(0) @binding(1)
var disp_x_tex: texture_2d_array<f32>;

@group(0) @binding(2)
var disp_z_tex: texture_2d_array<f32>;

@group(0) @binding(3)
var out_disp: texture_storage_2d_array<rgba16float, write>;

@group(0) @binding(4)
var out_derivs: texture_storage_2d_array<rg32float, write>;

@group(0) @binding(5)
var<uniform> uni: BakeUniforms;

@compute @workgroup_size(8, 8, 1)
fn fft_bake(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = uni.size;
    if (gid.x >= size || gid.y >= size || gid.z >= uni.lod_count) {
        return;
    }
    let layer = i32(gid.z);
    let size_i = i32(size);
    let x = i32(gid.x);
    let y = i32(gid.y);
    let here = vec2<i32>(x, y);
    let ix = vec2<i32>((x + 1) % size_i, y);
    let iz = vec2<i32>(x, (y + 1) % size_i);

    let texel = uni.fft_world * f32(1u << gid.z) / f32(size);
    let sign = select(1.0, -1.0, (x + y) % 2 == 1);
    let sign_x = -sign;
    let sign_z = -sign;

    let h = sign * textureLoad(height_tex, here, layer, 0).x;
    let dx = sign * textureLoad(disp_x_tex, here, layer, 0).x;
    let dz = sign * textureLoad(disp_z_tex, here, layer, 0).x;
    let h_x = sign_x * textureLoad(height_tex, ix, layer, 0).x;
    let h_z = sign_z * textureLoad(height_tex, iz, layer, 0).x;
    let dx_x = sign_x * textureLoad(disp_x_tex, ix, layer, 0).x;
    let dz_x = sign_x * textureLoad(disp_z_tex, ix, layer, 0).x;
    let dx_z = sign_z * textureLoad(disp_x_tex, iz, layer, 0).x;
    let dz_z = sign_z * textureLoad(disp_z_tex, iz, layer, 0).x;

    let d_dx_dx = 1.0 + (dx_x - dx) / texel;
    let d_dz_dx = (dz_x - dz) / texel;
    let d_dx_dz = (dx_z - dx) / texel;
    let d_dz_dz = 1.0 + (dz_z - dz) / texel;
    let jacobian = d_dx_dx * d_dz_dz - d_dz_dx * d_dx_dz;

    let coord3 = here;
    textureStore(out_disp, coord3, layer, vec4<f32>(dx, h, dz, jacobian));
    textureStore(out_derivs, coord3, layer, vec4<f32>((h_x - h) / texel, (h_z - h) / texel, 0.0, 0.0));
}
