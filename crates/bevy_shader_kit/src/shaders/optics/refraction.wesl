fn snell_refract(v: vec3<f32>, n: vec3<f32>, eta: f32) -> vec3<f32> {
    let cos_i = dot(v, n);
    let cos_t2 = 1.0 - eta * eta * (1.0 - cos_i * cos_i);
    if (cos_t2 < 0.0) {
        return reflect(-v, n);
    }
    let cos_t = -(select(1.0, -1.0, cos_i >= 0.0)) * sqrt(cos_t2);
    return (eta * cos_i + cos_t) * n - eta * v;
}
