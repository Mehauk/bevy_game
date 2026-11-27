use bevy::{
    asset::embedded_asset,
    prelude::*,
    render::{
        RenderApp, extract_component::ExtractComponent, render_resource::ShaderType,
    },
    shader::ShaderRef,
};
use bevy_shader_kit::{PostProcessEffect, PostProcessPlugin};

use crate::query::WaterSampler;
use crate::settings::OceanSettings;

#[repr(C)]
#[derive(Component, ExtractComponent, Clone, Copy, Debug, ShaderType, Reflect)]
#[extract_app(RenderApp)]
pub struct UnderwaterFog {
    pub water_level: f32,
    pub time: f32,
    pub enabled: u32,
    pub pad: f32,
    pub tint: Vec3,
    pub deep_color: Vec3,
    pub depth_density: f32,
    pub darkening: f32,
    pub distortion_strength: f32,
    pub distortion_scale: f32,
}

impl Default for UnderwaterFog {
    fn default() -> Self {
        Self {
            water_level: 0.0,
            time: 0.0,
            enabled: 0,
            pad: 0.0,
            tint: Vec3::new(0.55, 0.32, 0.22),
            deep_color: Vec3::new(0.02, 0.10, 0.14),
            depth_density: 0.12,
            darkening: 0.05,
            distortion_strength: 0.01,
            distortion_scale: 1.0,
        }
    }
}

impl PostProcessEffect for UnderwaterFog {
    fn fragment_shader() -> ShaderRef {
        "embedded://crest_water/shaders/underwater_fog.wesl".into()
    }

    fn is_enabled(&self) -> bool {
        self.enabled != 0
    }
}

fn sync_underwater_fog(
    time: Res<Time>,
    settings: Res<OceanSettings>,
    sampler: Res<WaterSampler>,
    mut lenses: Query<(&GlobalTransform, &mut UnderwaterFog)>,
) {
    let now = time.elapsed_secs();
    for (lens, mut fog) in &mut lenses {
        let eye = lens.translation();
        let surface = sampler
            .sample(eye.xz(), settings.water_level)
            .map(|sample| sample.height)
            .unwrap_or(settings.water_level);
        fog.time = now;
        fog.water_level = settings.water_level;
        fog.enabled = u32::from(eye.y < surface);
    }
}

pub struct UnderwaterPlugin;

impl Plugin for UnderwaterPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/underwater_fog.wesl");
        app.add_plugins(PostProcessPlugin::<UnderwaterFog>::default())
            .add_systems(Update, sync_underwater_fog);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_matches_the_shader_layout() {
        assert_eq!(UnderwaterFog::min_size().get(), 64);
        let source = include_str!("shaders/underwater_fog.wesl");
        for field in [
            "water_level:",
            "time:",
            "enabled:",
            "tint:",
            "deep_color:",
            "depth_density:",
            "darkening:",
            "distortion_strength:",
            "distortion_scale:",
        ] {
            assert!(
                source.contains(field),
                "underwater_fog.wesl is missing the `{field}` uniform field"
            );
        }
        assert!(source.contains("@binding(4)"));
    }

    #[test]
    fn disabled_by_default() {
        assert_eq!(UnderwaterFog::default().enabled, 0);
        assert!(!UnderwaterFog::default().is_enabled());
    }
}
