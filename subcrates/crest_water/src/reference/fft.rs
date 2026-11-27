pub struct FftBaked {
    pub disp: Vec<[f32; 4]>,
    pub derivs: Vec<[f32; 2]>,
}

fn twiddle_table(size: usize) -> Vec<[f32; 2]> {
    let log2_size = size.trailing_zeros();
    let mut table = vec![[0.0f32; 2]; size * log2_size as usize];
    let mut offset = 1usize;
    let mut num_iterations = size >> 1;
    for row in 0..log2_size {
        let row_offset = row as usize * size;
        let mut start = 0usize;
        let mut end = 2 * offset;
        for _iteration in 0..num_iterations {
            let mut big_k = 0.0f32;
            let mut k = start;
            while k < end {
                let phase =
                    2.0f32 * std::f32::consts::PI * big_k * (num_iterations as f32) / (size as f32);
                let cos = phase.cos();
                let sin = phase.sin();
                table[row_offset + k / 2] = [cos, -sin];
                table[row_offset + k / 2 + offset] = [-cos, sin];
                big_k += 1.0;
                k += 2;
            }
            start += 4 * offset;
            end = start + 2 * offset;
        }
        num_iterations >>= 1;
        offset <<= 1;
    }
    table
}

fn fft_1d(input: &[[f32; 2]], table: &[[f32; 2]], log2_size: u32, output: &mut [[f32; 2]]) {
    let size = input.len();
    let mut intermediates: Vec<[f32; 2]> = input.to_vec();
    let mut scratch: Vec<[f32; 2]> = vec![[0.0f32; 2]; size];
    for pass in 0..log2_size {
        let offset = 1usize << pass;
        let pingpong = pass % 2 == 0;
        for coord in 0..size {
            let (mut index_a, mut index_b) = if (coord / offset) % 2 == 1 {
                (coord - offset, coord)
            } else {
                (coord, coord + offset)
            };
            if pass == 0 {
                index_a = ((index_a as u32).reverse_bits() >> (32 - log2_size)) as usize;
                index_b = ((index_b as u32).reverse_bits() >> (32 - log2_size)) as usize;
            }
            let w = table[pass as usize * size + coord];
            let (value_a, value_b) = if pingpong {
                (intermediates[index_a], intermediates[index_b])
            } else {
                (scratch[index_a], scratch[index_b])
            };
            let weighted = [
                w[0] * value_b[0] - w[1] * value_b[1],
                w[1] * value_b[0] + w[0] * value_b[1],
            ];
            let result = [value_a[0] + weighted[0], value_a[1] + weighted[1]];
            if pingpong {
                scratch[coord] = result;
            } else {
                intermediates[coord] = result;
            }
        }
    }
    let source = if log2_size.is_multiple_of(2) {
        &intermediates
    } else {
        &scratch
    };
    output.copy_from_slice(source);
}

#[allow(clippy::ptr_arg)]
pub fn fft_2d(data: &mut Vec<[f32; 2]>, size: usize, inverse: bool) {
    let log2_size = size.trailing_zeros();
    let table = twiddle_table(size);
    let mut line = vec![[0.0f32; 2]; size];
    let mut out_line = vec![[0.0f32; 2]; size];
    if inverse {
        for value in data.iter_mut() {
            value[1] = -value[1];
        }
    }
    for y in 0..size {
        let row = &data[y * size..(y + 1) * size];
        fft_1d(row, &table, log2_size, &mut line);
        data[y * size..(y + 1) * size].copy_from_slice(&line);
    }
    for x in 0..size {
        for y in 0..size {
            line[y] = data[y * size + x];
        }
        fft_1d(&line, &table, log2_size, &mut out_line);
        for y in 0..size {
            data[y * size + x] = out_line[y];
        }
    }
    if inverse {
        let scale = 1.0 / (size * size) as f32;
        for value in data.iter_mut() {
            *value = [value[0] * scale, -value[1] * scale];
        }
    }
}

fn conj(input: &[[f32; 2]]) -> Vec<[f32; 2]> {
    input.iter().map(|v| [v[0], -v[1]]).collect()
}

pub fn bake_frame(
    height: &[[f32; 2]],
    disp_x: &[[f32; 2]],
    disp_z: &[[f32; 2]],
    size: usize,
    texel: f32,
) -> FftBaked {
    let mut h = conj(height);
    let mut dx = conj(disp_x);
    let mut dz = conj(disp_z);
    fft_2d(&mut h, size, false);
    fft_2d(&mut dx, size, false);
    fft_2d(&mut dz, size, false);

    let mut disp = vec![[0.0f32; 4]; size * size];
    for y in 0..size {
        for x in 0..size {
            let i = y * size + x;
            let sign = if (x + y) % 2 == 1 { -1.0 } else { 1.0 };
            disp[i] = [
                sign * dx[i][0],
                sign * h[i][0],
                sign * dz[i][0],
                0.0,
            ];
        }
    }

    let mut derivs = vec![[0.0f32; 2]; size * size];
    for y in 0..size {
        for x in 0..size {
            let i = y * size + x;
            let ix = y * size + (x + 1) % size;
            let iz = ((y + 1) % size) * size + x;
            let d_dx_dx = 1.0 + (disp[ix][0] - disp[i][0]) / texel;
            let d_dz_dx = (disp[ix][2] - disp[i][2]) / texel;
            let d_dx_dz = (disp[iz][0] - disp[i][0]) / texel;
            let d_dz_dz = 1.0 + (disp[iz][2] - disp[i][2]) / texel;
            disp[i][3] = d_dx_dx * d_dz_dz - d_dz_dx * d_dx_dz;
            derivs[i] = [
                (disp[ix][1] - disp[i][1]) / texel,
                (disp[iz][1] - disp[i][1]) / texel,
            ];
        }
    }

    FftBaked { disp, derivs }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_then_inverse_roundtrip_is_identity() {
        let size = 8;
        let mut data: Vec<[f32; 2]> = (0..size * size)
            .map(|i| [((i * 7) % 13) as f32 - 6.0, ((i * 3) % 11) as f32 - 5.0])
            .collect();
        let original = data.clone();
        fft_2d(&mut data, size, false);
        fft_2d(&mut data, size, true);
        for i in 0..data.len() {
            assert!((data[i][0] - original[i][0]).abs() < 1e-4);
            assert!((data[i][1] - original[i][1]).abs() < 1e-4);
        }
    }

    #[test]
    fn inverse_of_impulse_is_flat_at_one_over_n_squared() {
        let size = 8;
        let mut data = vec![[0.0f32; 2]; size * size];
        data[0] = [1.0, 0.0];
        fft_2d(&mut data, size, true);
        let expected = 1.0 / (size * size) as f32;
        for value in &data {
            assert!((value[0] - expected).abs() < 1e-6);
            assert!(value[1].abs() < 1e-6);
        }
    }

    #[test]
    fn forward_of_impulse_is_flat_at_one() {
        let size = 8;
        let mut data = vec![[0.0f32; 2]; size * size];
        data[0] = [1.0, 0.0];
        fft_2d(&mut data, size, false);
        for value in &data {
            assert!((value[0] - 1.0).abs() < 1e-5);
            assert!(value[1].abs() < 1e-5);
        }
    }

    #[test]
    fn forward_transform_matches_direct_dft() {
        let size = 8;
        let data: Vec<[f32; 2]> = (0..size * size)
            .map(|i| [((i * 5) % 9) as f32, ((i * 2) % 7) as f32 - 3.0])
            .collect();
        let mut copy = data.clone();
        fft_2d(&mut copy, size, false);
        let mut reference = vec![[0.0f32; 2]; size * size];
        for ky in 0..size {
            for kx in 0..size {
                let mut sum = [0.0f32; 2];
                for ny in 0..size {
                    for nx in 0..size {
                        let angle = -2.0 * std::f32::consts::PI
                            * ((kx * nx + ky * ny) as f32) / (size as f32);
                        let tw = [angle.cos(), angle.sin()];
                        let v = data[ny * size + nx];
                        sum[0] += v[0] * tw[0] - v[1] * tw[1];
                        sum[1] += v[0] * tw[1] + v[1] * tw[0];
                    }
                }
                reference[ky * size + kx] = sum;
            }
        }
        for i in 0..reference.len() {
            assert!((copy[i][0] - reference[i][0]).abs() < 1e-2);
            assert!((copy[i][1] - reference[i][1]).abs() < 1e-2);
        }
    }

    #[test]
    fn bake_frame_of_zero_input_is_flat_with_unit_jacobian() {
        let size = 8;
        let zero = vec![[0.0f32; 2]; size * size];
        let baked = bake_frame(&zero, &zero, &zero, size, 0.5);
        assert!(baked
            .disp
            .iter()
            .all(|d| d[0] == 0.0 && d[1] == 0.0 && d[2] == 0.0 && (d[3] - 1.0).abs() < 1e-6));
        assert!(baked.derivs.iter().all(|d| d.iter().all(|c| *c == 0.0)));
    }

    #[test]
    fn bake_frame_jacobian_matches_finite_difference_determinant() {
        let size = 8;
        let mut height = vec![[0.0f32; 2]; size * size];
        let mut disp_x = vec![[0.0f32; 2]; size * size];
        let mut disp_z = vec![[0.0f32; 2]; size * size];
        for y in 0..size {
            for x in 0..size {
                height[y * size + x] = [((x + y) % 5) as f32, 0.0];
                disp_x[y * size + x] = [(x as f32 * 0.3).sin(), 0.0];
                disp_z[y * size + x] = [(y as f32 * 0.4).cos(), 0.0];
            }
        }
        let texel = 0.5;
        let baked = bake_frame(&height, &disp_x, &disp_z, size, texel);
        for y in 0..size {
            for x in 0..size {
                let i = y * size + x;
                let ix = y * size + (x + 1) % size;
                let iz = ((y + 1) % size) * size + x;
                let a = 1.0 + (baked.disp[ix][0] - baked.disp[i][0]) / texel;
                let b = (baked.disp[ix][2] - baked.disp[i][2]) / texel;
                let c = (baked.disp[iz][0] - baked.disp[i][0]) / texel;
                let d = 1.0 + (baked.disp[iz][2] - baked.disp[i][2]) / texel;
                assert!((baked.disp[i][3] - (a * d - b * c)).abs() < 1e-5);
            }
        }
    }
}
