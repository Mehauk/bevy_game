use crate::terrain::field::ScalarField;

pub(super) fn in_bounds(field: &ScalarField, gx: isize, gz: isize) -> bool {
    if field.topology.wraps() {
        return true;
    }
    let dim = field.dim as isize;
    gx >= 0 && gz >= 0 && gx < dim && gz < dim
}

pub(super) fn step_distance(heights: &ScalarField, a: usize, b: usize, spacing: f32) -> f32 {
    let (ax, az) = heights.coords(a);
    let (bx, bz) = heights.coords(b);
    let dim = heights.dim as isize;
    let cell_delta = |a: usize, b: usize| -> f32 {
        let raw = (b as isize - a as isize).abs();
        if heights.topology.wraps() {
            raw.min(dim - raw) as f32
        } else {
            raw as f32
        }
    };
    let dx = cell_delta(ax, bx);
    let dz = cell_delta(az, bz);
    (dx * dx + dz * dz).sqrt().max(1.0) * spacing
}

#[inline(always)]
pub(super) fn sample_values(field: &ScalarField, values: &[f32], fx: f32, fz: f32) -> f32 {
    let x0 = fx.floor();
    let z0 = fz.floor();
    let tx = fx - x0;
    let tz = fz - z0;
    let x0i = field.wrap(x0 as isize);
    let x1i = field.wrap(x0 as isize + 1);
    let z0i = field.wrap(z0 as isize);
    let z1i = field.wrap(z0 as isize + 1);
    let v00 = values[field.index(x0i, z0i)];
    let v10 = values[field.index(x1i, z0i)];
    let v01 = values[field.index(x0i, z1i)];
    let v11 = values[field.index(x1i, z1i)];
    let top = v00 + (v10 - v00) * tx;
    let bottom = v01 + (v11 - v01) * tx;
    top + (bottom - top) * tz
}

pub(super) fn deposit_bilinear(
    field: &ScalarField,
    values: &mut [f32],
    fx: f32,
    fz: f32,
    amount: f32,
) {
    let x0 = fx.floor();
    let z0 = fz.floor();
    let tx = fx - x0;
    let tz = fz - z0;
    let x0i = field.wrap(x0 as isize);
    let x1i = field.wrap(x0 as isize + 1);
    let z0i = field.wrap(z0 as isize);
    let z1i = field.wrap(z0 as isize + 1);
    values[field.index(x0i, z0i)] += amount * (1.0 - tx) * (1.0 - tz);
    values[field.index(x1i, z0i)] += amount * tx * (1.0 - tz);
    values[field.index(x0i, z1i)] += amount * (1.0 - tx) * tz;
    values[field.index(x1i, z1i)] += amount * tx * tz;
}
