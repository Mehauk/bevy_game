use bevy::{
    prelude::*,
    render::{
        RenderApp,
        extract_component::ExtractComponent,
        render_resource::ShaderType,
    },
    shader::ShaderRef,
};

use crate::post_process::PostProcessEffect;

#[repr(C)]
#[derive(Component, ExtractComponent, Clone, Copy, Debug, ShaderType, Reflect)]
#[extract_app(RenderApp)]
pub struct UnderwaterPostProcess {
    pub water_height: f32,
    pub time: f32,
    pub max_depth: f32,
    pub phase_g: f32,
    pub scattering: Vec3,
    pub absorption: Vec3,
    pub sun_direction: Vec3,
    pub sun_illuminance: Vec3,
    pub ambient_illuminance: Vec3,
    pub meniscus_thickness: f32,
    pub distortion_strength: f32,
    pub enabled: u32,
    pub pad0: f32,
    pub meniscus_color: Vec3,
}

impl Default for UnderwaterPostProcess {
    fn default() -> Self {
        Self {
            water_height: 0.0,
            time: 0.0,
            max_depth: 8.0,
            phase_g: 0.6,
            scattering: Vec3::new(0.03, 0.045, 0.05),
            absorption: Vec3::new(0.35, 0.12, 0.06),
            sun_direction: Vec3::new(0.707, 0.0, 0.707),
            sun_illuminance: Vec3::splat(3.0),
            ambient_illuminance: Vec3::new(0.15, 0.2, 0.25),
            meniscus_thickness: 0.06,
            distortion_strength: 0.02,
            enabled: 0,
            pad0: 0.0,
            meniscus_color: Vec3::new(0.55, 0.75, 0.72),
        }
    }
}

impl PostProcessEffect for UnderwaterPostProcess {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/post_process/underwater.wesl".into()
    }

    fn is_enabled(&self) -> bool {
        self.enabled != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_matches_the_shader_layout() {
        assert_eq!(UnderwaterPostProcess::min_size().get(), 128);
    }

    #[test]
    fn shader_declares_the_same_uniform_fields() {
        let source = include_str!("shaders/post_process/underwater.wesl");
        for field in [
            "water_height",
            "time",
            "max_depth",
            "phase_g",
            "scattering",
            "absorption",
            "sun_direction",
            "sun_illuminance",
            "ambient_illuminance",
            "meniscus_thickness",
            "distortion_strength",
            "enabled",
            "meniscus_color",
        ] {
            assert!(
                source.contains(&format!("{field}:")),
                "underwater.wesl is missing the `{field}` uniform field"
            );
        }
        assert!(source.contains("@binding(4)"));
    }

    #[test]
    fn disabled_by_default() {
        assert_eq!(UnderwaterPostProcess::default().enabled, 0);
    }
}
