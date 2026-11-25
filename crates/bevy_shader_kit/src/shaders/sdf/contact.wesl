struct ContactSurface {
    point: vec3<f32>,
    outward: vec2<f32>,
    emerged: f32,
}

fn project_onto_contact(
    p: vec3<f32>,
    gradient: vec3<f32>,
    distance: f32,
    range: f32,
    surface_height: f32,
) -> ContactSurface {
    var point = p;
    var outward = vec2<f32>(0.0);
    var emerged = 0.0;
    if (distance < range) {
        let grad_len = length(gradient);
        if (grad_len > 0.0) {
            let nrm = gradient / grad_len;
            let xz_len = max(length(vec2<f32>(nrm.x, nrm.z)), 1.0e-4);
            outward = vec2<f32>(nrm.x, nrm.z) / xz_len;
            point = p - nrm * min(distance, range);
        }
        emerged = 1.0 - smoothstep(0.0, 1.2, max(surface_height - point.y, 0.0));
    }
    return ContactSurface(point, outward, emerged);
}
