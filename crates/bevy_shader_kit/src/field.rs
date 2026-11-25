use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension, StandardMaterial},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct SdfVolume {
    pub field_from_world: Mat4,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
    pub inv_bounds_size: Vec3,
    pub resolution: Vec3,
}

impl Default for SdfVolume {
    fn default() -> Self {
        Self {
            field_from_world: Mat4::IDENTITY,
            bounds_min: Vec3::ZERO,
            bounds_max: Vec3::ZERO,
            inv_bounds_size: Vec3::ZERO,
            resolution: Vec3::ZERO,
        }
    }
}

impl SdfVolume {
    pub fn from_bounds(bounds_min: Vec3, bounds_max: Vec3, resolution: Vec3) -> Self {
        let span = (bounds_max - bounds_min).max(Vec3::splat(1e-3));
        Self {
            bounds_min,
            bounds_max,
            inv_bounds_size: Vec3::new(1.0 / span.x, 1.0 / span.y, 1.0 / span.z),
            resolution,
            ..default()
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct DistanceFieldViewUniform {
    pub volume: SdfVolume,
    pub time: f32,
    pub glow_range: f32,
    pub clay_alpha: f32,
}

impl Default for DistanceFieldViewUniform {
    fn default() -> Self {
        Self {
            volume: SdfVolume::default(),
            time: 0.0,
            glow_range: 0.0,
            clay_alpha: 1.0,
        }
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct DistanceFieldViewExt {
    #[texture(101, dimension = "3d", sample_type = "float", filterable = false)]
    pub sdf: Handle<Image>,
    #[uniform(100)]
    pub view: DistanceFieldViewUniform,
}

impl MaterialExtension for DistanceFieldViewExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/sdf/view_mode.wesl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/sdf/view_mode.wesl".into()
    }
}

pub type DistanceFieldViewMaterial = ExtendedMaterial<StandardMaterial, DistanceFieldViewExt>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_derive_the_inverse_scale() {
        let volume = SdfVolume::from_bounds(
            Vec3::new(-2.0, -4.0, -8.0),
            Vec3::new(2.0, 4.0, 8.0),
            Vec3::new(4.0, 8.0, 16.0),
        );
        assert_eq!(volume.inv_bounds_size, Vec3::new(0.25, 0.125, 0.0625));
        assert_eq!(volume.resolution, Vec3::new(4.0, 8.0, 16.0));
        assert_eq!(volume.field_from_world, Mat4::IDENTITY);
    }

    #[test]
    fn degenerate_bounds_stay_finite() {
        let volume = SdfVolume::from_bounds(Vec3::ZERO, Vec3::ZERO, Vec3::splat(2.0));
        assert!(volume.inv_bounds_size.is_finite());
    }
}
