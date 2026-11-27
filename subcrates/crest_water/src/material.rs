use bevy::{
    asset::embedded_asset,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin, StandardMaterial},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

use crate::cascades::{CascadeData, Cascades};
use crate::lod::LodTransforms;
use crate::ocean::{build_ocean_mesh, OceanRoot};
use crate::settings::OceanSettings;

pub const MAX_OCEAN_LODS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct OceanLodTransform {
    pub origin: Vec3,
    pub texel_size: f32,
    pub extent: f32,
    pub lod_index: f32,
    pub pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct OceanCascadeUniform {
    pub lods: [OceanLodTransform; MAX_OCEAN_LODS],
    pub lod_count: u32,
    pub resolution: u32,
    pub time: f32,
    pub water_level: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct OceanShading {
    pub deep_color: Vec3,
    pub shallow_color: Vec3,
    pub sun_direction: Vec3,
    pub sun_color: Vec3,
    pub sky_color: Vec3,
    pub depth_falloff: f32,
    pub foam_strength: f32,
    pub fresnel_strength: f32,
    pub specular_strength: f32,
    pub specular_shininess: f32,
    pub normal_strength: f32,
    pub flow_gain: f32,
    pub pad: f32,
}

impl Default for OceanShading {
    fn default() -> Self {
        Self {
            deep_color: Vec3::new(0.01, 0.06, 0.10),
            shallow_color: Vec3::new(0.10, 0.45, 0.45),
            sun_direction: Vec3::new(0.5, 0.6, 0.3).normalize(),
            sun_color: Vec3::splat(1.0),
            sky_color: Vec3::new(0.45, 0.60, 0.75),
            depth_falloff: 0.35,
            foam_strength: 1.0,
            fresnel_strength: 0.6,
            specular_strength: 0.8,
            specular_shininess: 250.0,
            normal_strength: 1.0,
            flow_gain: 0.5,
            pad: 0.0,
        }
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct OceanSurfaceExt {
    #[uniform(100)]
    pub cascades: OceanCascadeUniform,
    #[uniform(101)]
    pub shading: OceanShading,
    #[texture(102, dimension = "2d_array")]
    #[sampler(107)]
    pub animated_waves: Handle<Image>,
    #[texture(103, dimension = "2d_array")]
    pub animated_derivs: Handle<Image>,
    #[texture(104, dimension = "2d_array")]
    pub sea_floor_depth: Handle<Image>,
    #[texture(105, dimension = "2d_array")]
    pub foam: Handle<Image>,
    #[texture(106, dimension = "2d_array")]
    pub flow: Handle<Image>,
}

impl Default for OceanSurfaceExt {
    fn default() -> Self {
        Self {
            cascades: default(),
            shading: default(),
            animated_waves: default(),
            animated_derivs: default(),
            sea_floor_depth: default(),
            foam: default(),
            flow: default(),
        }
    }
}

impl MaterialExtension for OceanSurfaceExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://crest_water/shaders/ocean_surface.wesl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "embedded://crest_water/shaders/ocean_surface.wesl".into()
    }
}

pub type OceanSurfaceMaterial = ExtendedMaterial<StandardMaterial, OceanSurfaceExt>;

fn cascade_uniform(
    cascades: &Cascades,
    lods: &LodTransforms,
    settings: &OceanSettings,
    time: f32,
) -> OceanCascadeUniform {
    let count = cascades.lod_count.min(lods.count).min(MAX_OCEAN_LODS);
    let mut uniform = OceanCascadeUniform {
        lods: [OceanLodTransform::default(); MAX_OCEAN_LODS],
        lod_count: count as u32,
        resolution: cascades.resolution as u32,
        time,
        water_level: settings.water_level,
    };
    for index in 0..count {
        let transform = lods.transforms[index];
        uniform.lods[index] = OceanLodTransform {
            origin: transform.origin,
            texel_size: transform.texel_size,
            extent: transform.extent,
            lod_index: index as f32,
            pad: 0.0,
        };
    }
    uniform
}

fn sync_ocean_surface_materials(
    cascades: Res<Cascades>,
    lods: Res<LodTransforms>,
    settings: Res<OceanSettings>,
    time: Res<Time>,
    mut materials: ResMut<Assets<OceanSurfaceMaterial>>,
) {
    if cascades.lod_count == 0 {
        return;
    }
    let uniform = cascade_uniform(&cascades, &lods, &settings, time.elapsed_secs());
    let handle = |data: CascadeData| cascades.image(data).cloned().unwrap_or_default();
    for (_id, material) in materials.iter_mut() {
        let extension = &mut material.extension;
        extension.cascades = uniform;
        extension.animated_waves = handle(CascadeData::AnimatedWaves);
        extension.animated_derivs = handle(CascadeData::AnimatedDerivs);
        extension.sea_floor_depth = handle(CascadeData::SeaFloorDepth);
        extension.foam = handle(CascadeData::Foam);
        extension.flow = handle(CascadeData::Flow);
    }
}

pub fn spawn_ocean_surface(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<OceanSurfaceMaterial>,
) -> Entity {
    let mesh = meshes.add(build_ocean_mesh(OceanSettings::default().lod_count));
    commands
        .spawn((
            OceanRoot,
            Transform::default(),
            Mesh3d(mesh),
            MeshMaterial3d(material),
        ))
        .id()
}

pub struct OceanMaterialPlugin;

impl Plugin for OceanMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/ocean_surface.wesl");
        app.add_plugins(MaterialPlugin::<OceanSurfaceMaterial>::default())
            .add_systems(Update, sync_ocean_surface_materials);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_uniform_matches_the_shader_layout() {
        assert_eq!(OceanLodTransform::min_size().get(), 32);
        assert_eq!(
            OceanCascadeUniform::min_size().get(),
            32 * MAX_OCEAN_LODS as u64 + 16
        );
        let source = include_str!("shaders/ocean_surface.wesl");
        for field in [
            "origin:",
            "texel_size:",
            "extent:",
            "lod_index:",
            "lod_count:",
            "resolution:",
            "time:",
            "water_level:",
        ] {
            assert!(
                source.contains(field),
                "ocean_surface.wesl is missing the `{field}` field"
            );
        }
    }

    #[test]
    fn shading_uniform_matches_the_shader_layout() {
        assert_eq!(OceanShading::min_size().get(), 112);
        let source = include_str!("shaders/ocean_surface.wesl");
        for field in [
            "deep_color:",
            "shallow_color:",
            "sun_direction:",
            "sun_color:",
            "sky_color:",
            "depth_falloff:",
            "foam_strength:",
            "fresnel_strength:",
            "specular_strength:",
            "specular_shininess:",
            "normal_strength:",
            "flow_gain:",
        ] {
            assert!(
                source.contains(field),
                "ocean_surface.wesl is missing the `{field}` field"
            );
        }
        for binding in [
            "@binding(100)",
            "@binding(101)",
            "@binding(102)",
            "@binding(103)",
            "@binding(104)",
            "@binding(105)",
            "@binding(106)",
            "@binding(107)",
        ] {
            assert!(
                source.contains(binding),
                "ocean_surface.wesl is missing {binding}"
            );
        }
    }

    #[test]
    fn the_default_shading_is_finite() {
        let shading = OceanShading::default();
        assert!(shading.deep_color.is_finite());
        assert!(shading.shallow_color.is_finite());
        assert!(shading.sun_direction.is_finite());
    }
}
