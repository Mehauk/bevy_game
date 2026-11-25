import package::shaders::optics::fresnel::fresnel_schlick;
import package::shaders::optics::refraction::snell_refract;
import package::shaders::optics::scattering::{beer_lambert_in_scatter, schlick_phase};

const WATER_IOR: f32 = 1.333;
const WATER_F0: f32 = 0.02;

fn water_volume_in_scatter(
    view_dir: vec3<f32>,
    normal: vec3<f32>,
    depth: f32,
    scattering: vec3<f32>,
    absorption: vec3<f32>,
    sun_direction: vec3<f32>,
    sun_illuminance: vec3<f32>,
    ambient_illuminance: vec3<f32>,
    phase_g: f32,
) -> vec3<f32> {
    let under_dir = snell_refract(view_dir, normal, 1.0 / WATER_IOR);
    let phase_value = schlick_phase(phase_g, dot(-sun_direction, under_dir));
    let ndv = max(dot(normal, view_dir), 0.0);
    let fresnel = fresnel_schlick(WATER_F0, ndv);
    return beer_lambert_in_scatter(
        scattering,
        absorption,
        depth,
        sun_illuminance,
        ambient_illuminance,
        phase_value,
        fresnel,
    );
}
