use bevy::color::Mix;
use bevy::light::{CascadeShadowConfigBuilder, NotShadowCaster};
use bevy::mesh::{Mesh, VertexAttributeValues};
use bevy::prelude::*;

use crate::terrain::atmosphere;
use crate::terrain::gpu::{self, SkyRenderMaterial, TerrainRenderMaterial};
use crate::terrain::map::TerrainMap;
use crate::terrain::params::Temperature;
use crate::terrain::region::Region;
use crate::terrain::season::{self, SeasonClock};

pub const SKY_RADIUS: f32 = 600.0;

#[derive(Resource, Clone)]
pub struct TerrainFog(pub DistanceFog);

impl Default for TerrainFog {
    fn default() -> Self {
        Self(atmosphere::to_distance_fog(
            atmosphere::atmosphere_profile(Region::TemperateGrassland),
        ))
    }
}

#[derive(Resource, Clone, Debug)]
pub struct TerrainAtmosphere {
    pub profile: atmosphere::RegionAtmosphereProfile,
}

impl Default for TerrainAtmosphere {
    fn default() -> Self {
        Self {
            profile: *atmosphere::atmosphere_profile(Region::TemperateGrassland),
        }
    }
}

#[derive(Component)]
pub struct SkyBox;

pub fn sky_mesh(radius: f32) -> Mesh {
    let mut mesh = Sphere::new(radius).mesh().uv(24, 16);
    let mut colors = Vec::new();
    if let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    {
        for position in positions.iter() {
            let direction = Vec3::from(*position)
                .try_normalize()
                .unwrap_or(Vec3::Y);
            colors.push([direction.x, direction.y, direction.z, 1.0]);
        }
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

pub fn setup_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut sky_materials: ResMut<Assets<SkyRenderMaterial>>,
) {
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::default(),
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 25.0,
            maximum_distance: 200.0,
            ..default()
        }
        .build(),
    ));

    let profile = *atmosphere::atmosphere_profile(Region::TemperateGrassland);
    let sun = atmosphere::sun_direction(SeasonClock::default().cycle_progress());
    commands.spawn((
        SkyBox,
        Mesh3d(meshes.add(sky_mesh(SKY_RADIUS))),
        MeshMaterial3d(sky_materials.add(gpu::sky_render_material(gpu::sky_uniform(
            &profile,
            sun,
            atmosphere::sun_colour(&profile, sun.y),
            0.0,
        )))),
        Transform::default(),
        NotShadowCaster,
    ));
}

pub const ATMOSPHERE_EASE_SECONDS: f32 = 0.6;

pub fn atmosphere_ease_factor(dt_secs: f32) -> f32 {
    let dt = dt_secs.clamp(0.0, 0.25);
    1.0 - (-dt / ATMOSPHERE_EASE_SECONDS).exp()
}

pub fn smooth_profile(
    current: &mut atmosphere::RegionAtmosphereProfile,
    target: &atmosphere::RegionAtmosphereProfile,
    t: f32,
) {
    current.region = target.region;
    current.fog_colour = current.fog_colour.mix(&target.fog_colour, t);
    current.light_colour = current.light_colour.mix(&target.light_colour, t);
    current.sky_zenith = current.sky_zenith.mix(&target.sky_zenith, t);
    current.sky_horizon = current.sky_horizon.mix(&target.sky_horizon, t);
    current.visibility += (target.visibility - current.visibility) * t;
    current.light_exponent += (target.light_exponent - current.light_exponent) * t;
    current.ambient_boost += (target.ambient_boost - current.ambient_boost) * t;
}

pub fn sync_terrain_atmosphere(
    time: Res<Time>,
    map: Res<TerrainMap>,
    clock: Res<SeasonClock>,
    mut atmosphere_state: ResMut<TerrainAtmosphere>,
    mut fog: ResMut<TerrainFog>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    mut lights: Query<(&mut DirectionalLight, &mut Transform)>,
    sky: Query<&MeshMaterial3d<SkyRenderMaterial>>,
    mut sky_materials: ResMut<Assets<SkyRenderMaterial>>,
) {
    let Some(camera_transform) = cameras.iter().next() else {
        return;
    };
    let position = camera_transform.translation();
    let contributions: Vec<(f32, atmosphere::RegionAtmosphereProfile)> = map
        .cell_weights_at(position.x, position.z)
        .iter()
        .filter_map(|(id, weight)| {
            map.region_of_cell(*id)
                .map(|region| (*weight, *atmosphere::atmosphere_profile(region)))
        })
        .collect();

    let blended = atmosphere::blend_atmosphere(&contributions);
    let ease = atmosphere_ease_factor(time.delta_secs());
    smooth_profile(&mut atmosphere_state.profile, &blended, ease);
    let eased = atmosphere_state.profile;
    fog.0 = atmosphere::to_distance_fog(&eased);
    clear.0 = clear.0.mix(&blended.sky_horizon, ease);

    let sun = atmosphere::sun_direction(clock.cycle_progress());
    let sun_colour = atmosphere::sun_colour(&eased, sun.y);
    let rotation = Quat::from_rotation_arc(Vec3::Y, sun);
    for (mut light, mut transform) in &mut lights {
        transform.rotation = transform.rotation.slerp(rotation, ease);
        light.color = light.color.mix(&sun_colour, ease);
        light.illuminance +=
            (atmosphere::sun_illuminance(&eased, sun.y) - light.illuminance) * ease;
    }
    ambient.color = ambient.color.mix(&atmosphere::ambient_colour(&eased), ease);
    ambient.brightness +=
        (atmosphere::ambient_brightness(&eased, sun.y) - ambient.brightness) * ease;

    let snow = season::snow_coverage_blend(
        &clock,
        map.parameters_at(position.x, position.z)
            .map(|parameters| parameters.temperature)
            .unwrap_or(Temperature::Temperate),
        map.height_at(position.x, position.z),
        gpu::SNOWLINE_METRES,
    );
    let params = gpu::sky_uniform(&eased, sun, sun_colour, snow);
    for handle in &sky {
        if let Some(mut material) = sky_materials.get_mut(handle) {
            material.extension.params = params;
        }
    }
}

pub fn sync_season_uniforms(
    map: Res<TerrainMap>,
    clock: Res<SeasonClock>,
    mut materials: ResMut<Assets<TerrainRenderMaterial>>,
) {
    if materials.is_empty() {
        return;
    }
    let tint = Vec4::from(gpu::season_tint_blend(&clock));
    let snowline = season::snowline_for(&clock, gpu::SNOWLINE_METRES);
    let snow = season::snow_coverage_blend(
        &clock,
        Temperature::Temperate,
        0.0,
        gpu::SNOWLINE_METRES,
    );
    for (_, material) in materials.iter_mut() {
        let params = &mut material.extension.params;
        params.sea_level = map.sea_level();
        params.flood_low = map.flood_zone().low;
        params.flood_high = map.flood_zone().high;
        params.season_tint = tint;
        params.snow_height = snowline;
        params.snow_amount = snow;
    }
}

pub fn sync_terrain_fog(
    terrain_fog: Res<TerrainFog>,
    mut commands: Commands,
    mut cameras: Query<(Entity, Option<&mut DistanceFog>), With<Camera3d>>,
) {
    for (entity, existing_fog) in &mut cameras {
        if let Some(mut fog) = existing_fog {
            if terrain_fog.is_changed() {
                *fog = terrain_fog.0.clone();
            }
        } else {
            commands.entity(entity).insert(terrain_fog.0.clone());
        }
    }
}

const DEFAULT_FOG_ALPHA: f32 = 1.0;
const DEFAULT_FOG_LIGHT_ALPHA: f32 = 0.5;
const FOG_ON_THRESHOLD: f32 = 0.05;

fn toggle_fog_alpha(color: &mut Color, on_alpha: f32) {
    color.set_alpha(if color.alpha() > FOG_ON_THRESHOLD {
        0.0
    } else {
        on_alpha
    });
}

pub fn toggle_atmospheric_fog(mut terrain_fog: ResMut<TerrainFog>) {
    toggle_fog_alpha(&mut terrain_fog.0.color, DEFAULT_FOG_ALPHA);
}

pub fn toggle_fog_light_influence(mut terrain_fog: ResMut<TerrainFog>) {
    toggle_fog_alpha(&mut terrain_fog.0.directional_light_color, DEFAULT_FOG_LIGHT_ALPHA);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fog_toggle_round_trips() {
        let mut color = Color::srgba(0.1, 0.2, 0.3, DEFAULT_FOG_ALPHA);
        toggle_fog_alpha(&mut color, DEFAULT_FOG_ALPHA);
        assert_eq!(color.alpha(), 0.0, "on -> off");
        toggle_fog_alpha(&mut color, DEFAULT_FOG_ALPHA);
        assert_eq!(color.alpha(), DEFAULT_FOG_ALPHA, "off -> on");
    }

    #[test]
    fn a_faded_fog_counts_as_off() {
        let mut color = Color::srgba(0.1, 0.2, 0.3, FOG_ON_THRESHOLD / 2.0);
        toggle_fog_alpha(&mut color, DEFAULT_FOG_ALPHA);
        assert_eq!(
            color.alpha(),
            DEFAULT_FOG_ALPHA,
            "a near-zero alpha must toggle back on"
        );
    }

    #[test]
    fn sky_mesh_bakes_unit_directions_once() {
        let mesh = sky_mesh(100.0);
        assert!(mesh.indices().is_some(), "the sky must be an indexed sphere");
        let colors = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
            Some(VertexAttributeValues::Float32x4(values)) => values.clone(),
            other => panic!("the sky must carry unit directions as vertex colours, got {other:?}"),
        };
        assert!(!colors.is_empty());
        for colour in &colors {
            let length = (colour[0] * colour[0] + colour[1] * colour[1] + colour[2] * colour[2]).sqrt();
            assert!((length - 1.0).abs() < 1e-4, "baked sky directions must be unit length");
        }
    }

    #[test]
    fn atmosphere_easing_converges_without_oscillation() {
        let cold = *atmosphere::atmosphere_profile(Region::GlacierIcefield);
        let hot = *atmosphere::atmosphere_profile(Region::DesertBarren);
        let span = (hot.visibility - cold.visibility).abs();
        assert!(span > 1.0, "the biomes must differ enough to exercise easing");
        let direction = if hot.visibility > cold.visibility {
            1.0
        } else {
            -1.0
        };
        let mut current = cold;
        let mut previous = cold.visibility;
        for _ in 0..240 {
            smooth_profile(&mut current, &hot, atmosphere_ease_factor(1.0 / 60.0));
            assert!(
                (current.visibility - previous) * direction >= -1e-6,
                "easing must approach the target monotonically, never oscillate"
            );
            previous = current.visibility;
        }
        assert!(
            (current.visibility - hot.visibility).abs() < span * 0.02,
            "four seconds of easing must land on the target"
        );
        assert!(
            (current.ambient_boost - hot.ambient_boost).abs() < 0.02,
            "ambient must converge with the rest of the profile"
        );
    }

    #[test]
    fn atmosphere_easing_is_frame_rate_independent() {
        assert_eq!(atmosphere_ease_factor(0.0), 0.0);
        assert!(atmosphere_ease_factor(0.01) < atmosphere_ease_factor(0.1));
        let from = *atmosphere::atmosphere_profile(Region::DesertBarren);
        let to = *atmosphere::atmosphere_profile(Region::GlacierIcefield);
        let mut one_step = from;
        smooth_profile(&mut one_step, &to, atmosphere_ease_factor(0.2));
        let mut many_steps = from;
        for _ in 0..12 {
            smooth_profile(&mut many_steps, &to, atmosphere_ease_factor(1.0 / 60.0));
        }
        assert!(
            (one_step.visibility - many_steps.visibility).abs() < 1e-3,
            "0.2 s must ease the same amount whether stepped at 5 Hz or 60 Hz"
        );
    }

    #[test]
    fn the_sky_carries_only_uniform_state_no_per_frame_mesh() {
        let profile = *atmosphere::atmosphere_profile(Region::TemperateGrassland);
        let sun = atmosphere::sun_direction(0.25);
        let first = gpu::sky_uniform(&profile, sun, atmosphere::sun_colour(&profile, sun.y), 0.0);
        let second = gpu::sky_uniform(&profile, sun, atmosphere::sun_colour(&profile, sun.y), 0.0);
        assert_eq!(first.sun_direction, second.sun_direction);
        let mesh = sky_mesh(SKY_RADIUS);
        assert!(
            mesh.indices().is_some(),
            "the sky mesh is built once at startup and never rebuilt per frame"
        );
    }
}
