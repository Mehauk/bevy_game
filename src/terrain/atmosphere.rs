use bevy::{
    light::{CascadeShadowConfigBuilder, NotShadowCaster},
    prelude::*,
};
use crate::distance_field::DistanceFieldDimmedColor;

const SKYBOX_VIEW_COLOR: Color = Color::srgb(0.015, 0.015, 0.018);

#[derive(Resource, Clone)]
pub struct TerrainFog(pub DistanceFog);

impl Default for TerrainFog {
    fn default() -> Self {
        Self(DistanceFog {
            color: Color::srgba(0.35, 0.48, 0.66, 1.0),
            directional_light_color: Color::srgba(1.0, 0.95, 0.85, 0.5),
            directional_light_exponent: 30.0,
            falloff: FogFalloff::from_visibility_colors(
                110.0,
                Color::srgb(0.35, 0.5, 0.66),
                Color::srgb(0.8, 0.844, 1.0),
            ),
        })
    }
}

#[derive(Component)]
pub(crate) struct SkyBox;

pub(super) fn setup_atmosphere(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cascade_shadow_config = CascadeShadowConfigBuilder {
        first_cascade_far_bound: 25.0,
        maximum_distance: 200.0,
        ..default()
    }
    .build();

    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.98, 0.95, 0.82),
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0).looking_at(Vec3::new(-0.15, -0.05, 0.25), Vec3::Y),
        cascade_shadow_config,
    ));

    commands.spawn((
        SkyBox,
        DistanceFieldDimmedColor(SKYBOX_VIEW_COLOR),
        Mesh3d(meshes.add(Cuboid::new(2.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Srgba::hex("888888").unwrap().into(),
            unlit: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_scale(Vec3::splat(600.0)),
        NotShadowCaster,
    ));
}

pub(super) fn sync_terrain_fog(
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

pub(super) fn toggle_atmospheric_fog(mut terrain_fog: ResMut<TerrainFog>) {
    toggle_fog_alpha(&mut terrain_fog.0.color, DEFAULT_FOG_ALPHA);
}

pub(super) fn toggle_fog_light_influence(mut terrain_fog: ResMut<TerrainFog>) {
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
        assert_eq!(color.alpha(), DEFAULT_FOG_ALPHA, "a near-zero alpha must toggle back on");
    }
}
