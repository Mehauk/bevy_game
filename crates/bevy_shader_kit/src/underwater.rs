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
use crate::water::{SwellSet, WaterOptics};

#[repr(C)]
#[derive(Component, ExtractComponent, Clone, Copy, Debug, ShaderType, Reflect)]
#[extract_app(RenderApp)]
pub struct UnderwaterPostProcess {
    pub optics: WaterOptics,
    pub swells: SwellSet,
    pub water_height: f32,
    pub time: f32,
    pub meniscus_thickness: f32,
    pub distortion_strength: f32,
    pub enabled: u32,
    pub fft_swell_height: f32,
    pub fft_on: f32,
    pub pad2: f32,
    pub meniscus_color: Vec3,
}

impl Default for UnderwaterPostProcess {
    fn default() -> Self {
        Self {
            optics: WaterOptics::default(),
            swells: SwellSet::default(),
            water_height: 0.0,
            time: 0.0,
            meniscus_thickness: 0.06,
            distortion_strength: 0.02,
            enabled: 0,
            fft_swell_height: 0.0,
            fft_on: 0.0,
            pad2: 0.0,
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
        assert_eq!(UnderwaterPostProcess::min_size().get(), 288);
    }

    #[test]
    fn shader_declares_the_same_uniform_fields() {
        let source = include_str!("shaders/post_process/underwater.wesl");
        for field in [
            "optics: WaterOptics,",
            "swells: SwellSet,",
            "water_height:",
            "time:",
            "meniscus_thickness:",
            "distortion_strength:",
            "enabled:",
            "fft_swell_height:",
            "fft_on:",
            "meniscus_color:",
        ] {
            assert!(
                source.contains(field),
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
