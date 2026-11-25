import package::shaders::sdf::volume::{
    SDF_OUTSIDE, SdfVolume, volume_box_distance, volume_gradient, volume_sample_at, volume_uvw,
};

struct FieldContact {
    gradient: vec3<f32>,
    distance: f32,
    boundary: f32,
}

fn field_distance(volume: SdfVolume, tex: texture_3d<f32>, p: vec3<f32>) -> f32 {
    let uvw = volume_uvw(volume, p);
    return volume_sample_at(volume, tex, uvw, volume_box_distance(volume, uvw));
}

fn field_array_min_distance(
    fields: ptr<function, array<SdfVolume, 4>>,
    count: i32,
    t0: texture_3d<f32>,
    t1: texture_3d<f32>,
    t2: texture_3d<f32>,
    t3: texture_3d<f32>,
    p: vec3<f32>,
) -> f32 {
    var d = SDF_OUTSIDE;
    if (count > 0) {
        d = min(d, field_distance((*fields)[0], t0, p));
    }
    if (count > 1) {
        d = min(d, field_distance((*fields)[1], t1, p));
    }
    if (count > 2) {
        d = min(d, field_distance((*fields)[2], t2, p));
    }
    if (count > 3) {
        d = min(d, field_distance((*fields)[3], t3, p));
    }
    return d;
}

fn field_array_contact(
    fields: ptr<function, array<SdfVolume, 4>>,
    count: i32,
    t0: texture_3d<f32>,
    t1: texture_3d<f32>,
    t2: texture_3d<f32>,
    t3: texture_3d<f32>,
    p: vec3<f32>,
    range: f32,
) -> FieldContact {
    var d = SDF_OUTSIDE;
    var winner = -1;
    var winner_to_box = 0.0;
    if (count > 0) {
        let uvw0 = volume_uvw((*fields)[0], p);
        let tb0 = volume_box_distance((*fields)[0], uvw0);
        let d0 = volume_sample_at((*fields)[0], t0, uvw0, tb0);
        if (d0 < d) {
            d = d0;
            winner = 0;
            winner_to_box = tb0;
        }
    }
    if (count > 1) {
        let uvw1 = volume_uvw((*fields)[1], p);
        let tb1 = volume_box_distance((*fields)[1], uvw1);
        let d1 = volume_sample_at((*fields)[1], t1, uvw1, tb1);
        if (d1 < d) {
            d = d1;
            winner = 1;
            winner_to_box = tb1;
        }
    }
    if (count > 2) {
        let uvw2 = volume_uvw((*fields)[2], p);
        let tb2 = volume_box_distance((*fields)[2], uvw2);
        let d2 = volume_sample_at((*fields)[2], t2, uvw2, tb2);
        if (d2 < d) {
            d = d2;
            winner = 2;
            winner_to_box = tb2;
        }
    }
    if (count > 3) {
        let uvw3 = volume_uvw((*fields)[3], p);
        let tb3 = volume_box_distance((*fields)[3], uvw3);
        let d3 = volume_sample_at((*fields)[3], t3, uvw3, tb3);
        if (d3 < d) {
            d = d3;
            winner = 3;
            winner_to_box = tb3;
        }
    }
    var g = vec3<f32>(0.0);
    var boundary = 0.0;
    if (d < range) {
        if (winner == 0) {
            g = volume_gradient((*fields)[0], t0, volume_uvw((*fields)[0], p));
        } else if (winner == 1) {
            g = volume_gradient((*fields)[1], t1, volume_uvw((*fields)[1], p));
        } else if (winner == 2) {
            g = volume_gradient((*fields)[2], t2, volume_uvw((*fields)[2], p));
        } else if (winner == 3) {
            g = volume_gradient((*fields)[3], t3, volume_uvw((*fields)[3], p));
        }
        boundary = smoothstep(0.0, 1.0, -winner_to_box);
    }
    return FieldContact(g, d, boundary);
}

fn field_array_raymarch_depth(
    fields: ptr<function, array<SdfVolume, 4>>,
    count: i32,
    t0: texture_3d<f32>,
    t1: texture_3d<f32>,
    t2: texture_3d<f32>,
    t3: texture_3d<f32>,
    p: vec3<f32>,
    max_depth: f32,
    steps: i32,
) -> f32 {
    let d = field_array_min_distance(fields, count, t0, t1, t2, t3, p);
    if (d >= max_depth) {
        return max_depth;
    }
    var t = max(d, 0.0);
    for (var i: i32 = 0; i < steps; i = i + 1) {
        let step = field_array_min_distance(
            fields,
            count,
            t0,
            t1,
            t2,
            t3,
            p - vec3<f32>(0.0, t, 0.0),
        );
        if (step <= 0.0) {
            break;
        }
        t = t + max(step, 0.02);
        if (t >= max_depth) {
            return max_depth;
        }
    }
    return min(t, max_depth);
}
