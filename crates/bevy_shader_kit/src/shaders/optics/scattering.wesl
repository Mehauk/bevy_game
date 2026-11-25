fn schlick_phase(g: f32, cos_theta: f32) -> f32 {
    let d = max(1.0 + g * g - 2.0 * g * cos_theta, 1.0e-4);
    return (1.0 - g * g) / pow(d, 1.5);
}

fn beer_lambert_in_scatter(
    scattering: vec3<f32>,
    absorption: vec3<f32>,
    depth: f32,
    sun_illuminance: vec3<f32>,
    ambient_illuminance: vec3<f32>,
    phase_value: f32,
    fresnel: f32,
) -> vec3<f32> {
    let extinction = scattering + absorption;
    let transmittance = exp(-extinction * depth);
    let amount = clamp(
        scattering * (1.0 - transmittance) / max(extinction, vec3<f32>(1.0e-5)),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
    let incoming = sun_illuminance * phase_value + ambient_illuminance;
    return incoming * amount * (1.0 - fresnel);
}
