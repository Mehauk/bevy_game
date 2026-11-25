fn phase_derivative_aa(phase: f32) -> f32 {
    let delta = max(abs(dpdx(phase)), abs(dpdy(phase)));
    return 1.0 - clamp(delta * 0.33, 0.0, 1.0);
}
