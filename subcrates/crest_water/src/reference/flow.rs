use bevy::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct SplineSpec {
    pub points: Vec<[f32; 3]>,
    pub width: f32,
    pub flow_speed: f32,
    pub wave_multiplier: f32,
    pub foam_amount: f32,
}

pub fn flow_from_spline(spline: &SplineSpec, x: f32, z: f32) -> Vec2 {
    if spline.points.len() < 2 || spline.width <= 0.0 {
        return Vec2::ZERO;
    }

    let mut best_dist_sq = f32::MAX;
    let mut best_dir = Vec2::ZERO;

    for segment in spline.points.windows(2) {
        let ax = segment[0][0];
        let az = segment[0][2];
        let bx = segment[1][0];
        let bz = segment[1][2];
        let seg = Vec2::new(bx - ax, bz - az);
        let seg_len_sq = seg.length_squared();
        let t = if seg_len_sq > 0.0 {
            (((x - ax) * seg.x + (z - az) * seg.y) / seg_len_sq).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let closest = Vec2::new(ax + seg.x * t, az + seg.y * t);
        let to_point = Vec2::new(x - closest.x, z - closest.y);
        let dist_sq = to_point.length_squared();
        if dist_sq < best_dist_sq {
            best_dist_sq = dist_sq;
            best_dir = if seg_len_sq > 0.0 {
                seg / seg_len_sq.sqrt()
            } else {
                Vec2::ZERO
            };
        }
    }

    let dist = best_dist_sq.sqrt();
    let edge = (dist / spline.width).clamp(0.0, 1.0);
    let falloff = 1.0 - edge * edge * (3.0 - 2.0 * edge);
    best_dir * (spline.flow_speed * falloff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight_spline() -> SplineSpec {
        SplineSpec {
            points: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
            width: 4.0,
            flow_speed: 2.5,
            wave_multiplier: 1.0,
            foam_amount: 1.0,
        }
    }

    #[test]
    fn centerline_flows_at_flow_speed() {
        let spline = straight_spline();
        let flow = flow_from_spline(&spline, 5.0, 0.0);
        assert!((flow.length() - 2.5).abs() < 1e-5);
        assert!((flow.x - 2.5).abs() < 1e-5);
        assert!(flow.y.abs() < 1e-5);
    }

    #[test]
    fn zero_beyond_width() {
        let spline = straight_spline();
        let flow = flow_from_spline(&spline, 5.0, 6.0);
        assert_eq!(flow, Vec2::ZERO);
    }

    #[test]
    fn falls_off_smoothly_with_distance() {
        let spline = straight_spline();
        let near = flow_from_spline(&spline, 5.0, 0.5).length();
        let mid = flow_from_spline(&spline, 5.0, 2.0).length();
        assert!(near > mid);
        assert!(mid > 0.0);
        assert!(mid < 2.5);
    }

    #[test]
    fn direction_follows_polyline() {
        let spline = SplineSpec {
            points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0]],
            width: 3.0,
            flow_speed: 1.0,
            wave_multiplier: 1.0,
            foam_amount: 1.0,
        };
        let flow = flow_from_spline(&spline, 0.0, 5.0);
        assert!((flow.y - 1.0).abs() < 1e-5);
        assert!(flow.x.abs() < 1e-5);
    }
}
