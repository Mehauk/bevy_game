use std::f32::consts::TAU;

use bevy::{
    asset::{AssetId, uuid::Uuid},
    mesh::MeshVertexBufferLayoutRef,
    pbr::{
        ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
        StandardMaterial,
    },
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::{Shader, ShaderDefVal, ShaderRef},
};

use crate::field::SdfVolume;

pub const MAX_RIPPLES: usize = 32;
pub const MAX_SWELLS: usize = 4;
pub const SHOAL_DEPTH: f32 = 4.0;
pub const SHORE_ZONE: f32 = 6.0;
pub const REFRACTION_STRENGTH: f32 = 0.8;
pub const CREST_SKEW: f32 = 0.5;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType, Reflect)]
pub struct RippleWave {
    pub origin: Vec2,
    pub delta: Vec2,
    pub birth: f32,
    pub birth_span: f32,
    pub strength: f32,
    pub frequency: f32,
    pub speed: f32,
    pub decay: f32,
    pub spread: f32,
    pub radius: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType, Reflect)]
pub struct RippleSet {
    pub count: u32,
    pub pad0: u32,
    pub pad1: u32,
    pub pad2: u32,
    pub waves: [RippleWave; MAX_RIPPLES],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Reflect)]
pub struct WaterOptics {
    pub scattering: Vec3,
    pub absorption: Vec3,
    pub sun_direction: Vec3,
    pub sun_illuminance: Vec3,
    pub ambient_illuminance: Vec3,
    pub max_depth: f32,
    pub phase_g: f32,
    pub pad0: f32,
    pub pad1: f32,
}

impl Default for WaterOptics {
    fn default() -> Self {
        Self {
            scattering: Vec3::new(0.03, 0.045, 0.05),
            absorption: Vec3::new(0.35, 0.12, 0.06),
            sun_direction: Vec3::new(0.707, 0.0, 0.707),
            sun_illuminance: Vec3::splat(3.0),
            ambient_illuminance: Vec3::new(0.15, 0.2, 0.25),
            max_depth: 8.0,
            phase_g: 0.6,
            pad0: 0.0,
            pad1: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Reflect)]
pub struct WaterParams {
    pub time: f32,
    pub water_height: f32,
    pub foam_width: f32,
    pub foam_strength: f32,
    pub ripple_normal: f32,
    pub ripple_foam: f32,
    pub wave_energy: f32,
    pub refraction: f32,
    pub optics: WaterOptics,
}

impl Default for WaterParams {
    fn default() -> Self {
        Self {
            time: 0.0,
            water_height: 0.0,
            foam_width: 0.8,
            foam_strength: 0.25,
            ripple_normal: 0.55,
            ripple_foam: 0.9,
            wave_energy: 0.35,
            refraction: 0.35,
            optics: WaterOptics::default(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Reflect)]
pub struct WaterDetail {
    pub drift: Vec2,
    pub detail_scale: f32,
    pub flow_scale: f32,
    pub flow_speed: f32,
    pub flow_strength: f32,
    pub layer_ratio: f32,
    pub gradient_step: f32,
    pub blend_a: f32,
    pub blend_b: f32,
    pub normal_gain: f32,
    pub foam_gain: f32,
    pub pad0: f32,
}

impl Default for WaterDetail {
    fn default() -> Self {
        Self {
            drift: Vec2::new(0.05, 0.04),
            detail_scale: 0.25,
            flow_scale: 0.25,
            flow_speed: 0.06,
            flow_strength: 0.15,
            layer_ratio: 2.3,
            gradient_step: 0.25,
            blend_a: 0.75,
            blend_b: 0.25,
            normal_gain: 0.1,
            foam_gain: 0.03,
            pad0: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Reflect)]
pub struct WaterDye {
    pub shallow: Vec3,
    pub deep: Vec3,
    pub beers_law: f32,
    pub depth_offset: f32,
    pub tint_exponent: f32,
    pub pad0: f32,
}

impl Default for WaterDye {
    fn default() -> Self {
        Self {
            shallow: Vec3::new(0.35, 0.75, 0.72),
            deep: Vec3::new(0.02, 0.13, 0.2),
            beers_law: 2.0,
            depth_offset: -0.75,
            tint_exponent: 2.5,
            pad0: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Reflect)]
pub struct WaterCaustics {
    pub strength: f32,
    pub scale: f32,
    pub speed: f32,
    pub sharpness: f32,
}

impl Default for WaterCaustics {
    fn default() -> Self {
        Self {
            strength: 0.45,
            scale: 0.9,
            speed: 0.5,
            sharpness: 6.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType, Reflect)]
pub struct SwellWave {
    pub direction: Vec2,
    pub amplitude: f32,
    pub wavelength: f32,
    pub speed: f32,
    pub steepness: f32,
    pub pad0: f32,
    pub pad1: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType, Reflect)]
pub struct SwellSet {
    pub count: u32,
    pub pad0: u32,
    pub pad1: u32,
    pub pad2: u32,
    pub waves: [SwellWave; MAX_SWELLS],
}

impl SwellSet {
    pub fn amplitude_span(&self) -> f32 {
        self.waves
            .iter()
            .take(self.count as usize)
            .map(|wave| wave.amplitude)
            .sum()
    }

    pub fn offset_at(&self, x: f32, z: f32, time: f32) -> Vec3 {
        let point = Vec2::new(x, z);
        let mut offset = Vec3::ZERO;
        for wave in self.waves.iter().take(self.count as usize) {
            let phase = (TAU / wave.wavelength) * (wave.direction.dot(point) - wave.speed * time);
            let (s, c) = phase.sin_cos();
            let horizontal = wave.steepness * wave.amplitude * c;
            offset += Vec3::new(
                wave.direction.x * horizontal,
                wave.amplitude * s,
                wave.direction.y * horizontal,
            );
        }
        offset
    }

    pub fn height_at(&self, x: f32, z: f32, time: f32) -> f32 {
        let drift = self.offset_at(x, z, time);
        self.offset_at(x - drift.x, z - drift.z, time).y
    }

    pub fn offset_at_shored(&self, x: f32, z: f32, time: f32, shore: ShoreContext) -> Vec3 {
        let point = Vec2::new(x, z);
        let mut offset = Vec3::ZERO;
        let vert = shore_vert_damp(shore.rest_depth);
        let horiz = shore_horiz_damp(shore.rest_depth);
        for wave in self.waves.iter().take(self.count as usize) {
            let shaped = shore.shaped_wave(wave);
            let k = TAU / shaped.wavelength;
            let phase = k * (shaped.direction.dot(point) - wave.speed * time);
            let (s, c) = phase.sin_cos();
            let horizontal = wave.steepness * shaped.amplitude * c * horiz;
            offset += Vec3::new(
                shaped.direction.x * horizontal,
                shaped.amplitude * s * vert,
                shaped.direction.y * horizontal,
            );
            if shore.shore_factor > 0.0 {
                let skew = shore.shore_factor * wave.steepness * shaped.amplitude * c * CREST_SKEW * horiz;
                offset += Vec3::new(shore.shore_dir.x * skew, 0.0, shore.shore_dir.y * skew);
            }
        }
        offset
    }

    pub fn height_at_shored(&self, x: f32, z: f32, time: f32, shore: ShoreContext) -> f32 {
        let drift = self.offset_at_shored(x, z, time, shore);
        self.offset_at_shored(x - drift.x, z - drift.z, time, shore).y
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ShoreContext {
    pub rest_depth: f32,
    pub shore_dir: Vec2,
    pub shore_factor: f32,
    pub energy: f32,
}

impl Default for ShoreContext {
    fn default() -> Self {
        Self {
            rest_depth: f32::INFINITY,
            shore_dir: Vec2::ZERO,
            shore_factor: 0.0,
            energy: 0.35,
        }
    }
}

impl ShoreContext {
    fn shaped_wave(&self, wave: &SwellWave) -> ShapedWave {
        let mut direction = wave.direction;
        if self.shore_factor > 0.0 {
            let bent = direction.lerp(self.shore_dir, self.shore_factor * REFRACTION_STRENGTH);
            if bent.length_squared() > 1.0e-6 {
                direction = bent.normalize();
            }
        }
        let depth = self.rest_depth.max(0.25);
        let shoal = (SHOAL_DEPTH / depth).powf(0.25).clamp(1.0, 2.5);
        let squeeze = (depth / SHOAL_DEPTH).sqrt().clamp(0.4, 1.0);
        ShapedWave {
            direction,
            amplitude: wave.amplitude * shoal * (0.5 + self.energy),
            wavelength: wave.wavelength * squeeze,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ShapedWave {
    direction: Vec2,
    amplitude: f32,
    wavelength: f32,
}

pub fn shore_vert_damp(rest_depth: f32) -> f32 {
    smoothstep(0.0, SHOAL_DEPTH, rest_depth)
}

pub fn shore_horiz_damp(rest_depth: f32) -> f32 {
    smoothstep(0.5, SHOAL_DEPTH, rest_depth)
}

pub fn shore_factor(rest_depth: f32) -> f32 {
    1.0 - smoothstep(0.0, SHORE_ZONE, rest_depth)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct WaterMaterialKey {
    pub foam: bool,
    pub optics: bool,
    pub ripples: bool,
    pub body_foam: bool,
    pub detail: bool,
    pub dye: bool,
    pub caustics: bool,
    pub refract: bool,
    pub override_shader: Option<Uuid>,
}

impl From<&WaterMaterialExt> for WaterMaterialKey {
    fn from(ext: &WaterMaterialExt) -> Self {
        Self {
            foam: ext.foam,
            optics: ext.optics,
            ripples: ext.ripples,
            body_foam: ext.body_foam,
            detail: ext.detail,
            dye: ext.dye,
            caustics: ext.caustics,
            refract: ext.refract,
            override_shader: ext.override_shader.as_ref().and_then(|shader| match shader.id() {
                AssetId::Uuid { uuid } => Some(uuid),
                AssetId::Index { .. } => None,
            }),
        }
    }
}

pub fn water_shader_defs(key: &WaterMaterialKey) -> Vec<ShaderDefVal> {
    vec![
        ShaderDefVal::Bool("WATER_FOAM".into(), key.foam),
        ShaderDefVal::Bool("WATER_OPTICS".into(), key.optics),
        ShaderDefVal::Bool("WATER_RIPPLES".into(), key.ripples),
        ShaderDefVal::Bool("WATER_BODY_FOAM".into(), key.body_foam),
        ShaderDefVal::Bool("WATER_DETAIL".into(), key.detail),
        ShaderDefVal::Bool("WATER_DYE".into(), key.dye),
        ShaderDefVal::Bool("WATER_CAUSTICS".into(), key.caustics),
        ShaderDefVal::Bool("WATER_REFRACT".into(), key.refract),
    ]
}

pub fn apply_water_specialization(descriptor: &mut RenderPipelineDescriptor, key: &WaterMaterialKey) {
    descriptor.vertex.shader_defs.extend(water_shader_defs(key));
    if let Some(fragment) = descriptor.fragment.as_mut() {
        fragment.shader_defs.extend(water_shader_defs(key));
        if let Some(uuid) = key.override_shader {
            fragment.shader = Handle::from(uuid);
        }
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[bind_group_data(WaterMaterialKey)]
pub struct WaterMaterialExt {
    #[uniform(100)]
    pub params: WaterParams,
    #[uniform(101)]
    pub waves: RippleSet,
    #[uniform(102)]
    pub swells: SwellSet,
    #[uniform(103)]
    pub body_volume: SdfVolume,
    #[texture(104, dimension = "3d", sample_type = "float", filterable = false)]
    pub body_sdf: Handle<Image>,
    #[uniform(105)]
    pub detail_settings: WaterDetail,
    #[uniform(106)]
    pub dye_settings: WaterDye,
    #[uniform(107)]
    pub caustics_settings: WaterCaustics,
    pub foam: bool,
    pub optics: bool,
    pub ripples: bool,
    pub body_foam: bool,
    pub detail: bool,
    pub dye: bool,
    pub caustics: bool,
    pub refract: bool,
    pub override_shader: Option<Handle<Shader>>,
}

impl Default for WaterMaterialExt {
    fn default() -> Self {
        Self {
            params: default(),
            waves: default(),
            swells: default(),
            body_volume: default(),
            body_sdf: default(),
            detail_settings: default(),
            dye_settings: default(),
            caustics_settings: default(),
            foam: true,
            optics: true,
            ripples: true,
            body_foam: false,
            detail: true,
            dye: true,
            caustics: true,
            refract: true,
            override_shader: None,
        }
    }
}

impl MaterialExtension for WaterMaterialExt {
    fn vertex_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/water/water_vertex.wesl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/water/water.wesl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/water/water.wesl".into()
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        apply_water_specialization(descriptor, &key.bind_group_data);
        Ok(())
    }
}

pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterMaterialExt>;

#[cfg(test)]
mod tests {
    use super::*;

    fn key_ext() -> WaterMaterialExt {
        WaterMaterialExt {
            optics: false,
            ..default()
        }
    }

    #[test]
    fn key_mirrors_extension_flags() {
        let key = WaterMaterialKey::from(&key_ext());
        assert!(key.foam);
        assert!(!key.optics);
        assert!(key.ripples);
        assert_eq!(key.override_shader, None);
    }

    #[test]
    fn ripple_uniform_matches_the_shader_layout() {
        assert_eq!(RippleWave::min_size().get(), 48);
        assert_eq!(RippleSet::min_size().get(), 16 + 48 * MAX_RIPPLES as u64);
    }

    #[test]
    fn optics_uniform_matches_the_shader_layout() {
        assert_eq!(WaterOptics::min_size().get(), 96);
        let source = include_str!("shaders/water/optics_types.wesl");
        for field in [
            "scattering",
            "absorption",
            "sun_direction",
            "sun_illuminance",
            "ambient_illuminance",
            "max_depth",
            "phase_g",
        ] {
            assert!(
                source.contains(&format!("{field}:")),
                "optics_types.wesl is missing the `{field}` field"
            );
        }
    }

    #[test]
    fn params_uniform_embeds_the_shared_optics() {
        assert_eq!(WaterParams::min_size().get(), 128);
        let source = include_str!("shaders/water/types.wesl");
        assert!(source.contains("optics: WaterOptics,"));
    }

    #[test]
    fn shader_declares_the_same_ripple_capacity() {
        let source = include_str!("shaders/water/ripples.wesl");
        assert!(source.contains(&format!("array<RippleWave, {MAX_RIPPLES}>")));
        assert!(source.contains(&format!("min(ripples.count, {MAX_RIPPLES}u)")));
        assert!(source.contains("@binding(101)"));
    }

    #[test]
    fn swell_uniform_matches_the_shader_layout() {
        assert_eq!(SwellWave::min_size().get(), 32);
        assert_eq!(SwellSet::min_size().get(), 16 + 32 * MAX_SWELLS as u64);
        let types = include_str!("shaders/water/swell_types.wesl");
        assert!(types.contains(&format!("array<SwellWave, {MAX_SWELLS}>")));
        assert!(types.contains(&format!("min(swell_set.count, {MAX_SWELLS}u)")));
        let source = include_str!("shaders/water/swells.wesl");
        assert!(source.contains("@binding(102)"));
    }

    #[test]
    fn flat_set_rests_on_the_surface() {
        assert_eq!(SwellSet::default().height_at(3.0, -7.0, 2.0), 0.0);
    }

    #[test]
    fn a_single_wave_traces_its_swell_profile() {
        let set = SwellSet {
            count: 1,
            waves: [
                SwellWave {
                    direction: Vec2::X,
                    amplitude: 0.2,
                    wavelength: 8.0,
                    speed: 0.0,
                    steepness: 0.0,
                    pad0: 0.0,
                    pad1: 0.0,
                },
                default(),
                default(),
                default(),
            ],
            ..default()
        };
        assert!(set.height_at(0.0, 0.0, 0.0).abs() < 1e-6, "phase zero sits flat");
        let crest = set.height_at(2.0, 0.0, 0.0);
        assert!((crest - 0.2).abs() < 1e-5, "quarter wavelength is the crest, got {crest}");
        assert!((set.height_at(-2.0, 0.0, 0.0) + 0.2).abs() < 1e-5, "opposite side is the trough");
    }

    #[test]
    fn deep_water_keeps_the_full_swell() {
        let set = SwellSet {
            count: 1,
            waves: [
                SwellWave {
                    direction: Vec2::X,
                    amplitude: 0.2,
                    wavelength: 8.0,
                    speed: 0.0,
                    steepness: 0.0,
                    pad0: 0.0,
                    pad1: 0.0,
                },
                default(),
                default(),
                default(),
            ],
            ..default()
        };
        let deep = ShoreContext {
            energy: 0.5,
            ..default()
        };
        let parity = set.height_at_shored(2.0, 0.0, 0.0, deep);
        assert!((parity - set.height_at(2.0, 0.0, 0.0)).abs() < 1e-5);
    }

    #[test]
    fn shallow_water_damps_the_swell_away() {
        let set = SwellSet {
            count: 1,
            waves: [
                SwellWave {
                    direction: Vec2::X,
                    amplitude: 0.2,
                    wavelength: 8.0,
                    speed: 0.0,
                    steepness: 0.0,
                    pad0: 0.0,
                    pad1: 0.0,
                },
                default(),
                default(),
                default(),
            ],
            ..default()
        };
        let beach = ShoreContext {
            rest_depth: 0.0,
            energy: 0.5,
            ..default()
        };
        assert!(set.height_at_shored(2.0, 0.0, 0.0, beach).abs() < 1e-6);
        assert!(shore_vert_damp(0.0) < shore_vert_damp(2.0));
        assert!(shore_vert_damp(2.0) < shore_vert_damp(8.0));
        assert!(shore_factor(0.0) > shore_factor(8.0));
    }

    #[test]
    fn refraction_bends_waves_toward_the_shore() {
        let set = SwellSet {
            count: 1,
            waves: [
                SwellWave {
                    direction: Vec2::X,
                    amplitude: 0.2,
                    wavelength: 8.0,
                    speed: 0.0,
                    steepness: 0.0,
                    pad0: 0.0,
                    pad1: 0.0,
                },
                default(),
                default(),
                default(),
            ],
            ..default()
        };
        let onshore = ShoreContext {
            rest_depth: 2.0,
            shore_dir: Vec2::NEG_X,
            shore_factor: 1.0,
            energy: 0.5,
        };
        let bent = set.height_at_shored(2.0, 0.0, 0.0, onshore);
        let straight = set.height_at_shored(2.0, 0.0, 0.0, ShoreContext { rest_depth: 2.0, energy: 0.5, ..default() });
        assert!(bent * straight < 0.0, "a fully refracted wave runs back the way it came");
    }

    #[test]
    fn steep_waves_drift_the_surface_horizontally() {
        let steep = SwellWave {
            direction: Vec2::X,
            amplitude: 0.3,
            wavelength: 6.0,
            speed: 1.0,
            steepness: 0.8,
            pad0: 0.0,
            pad1: 0.0,
        };
        let set = SwellSet {
            count: 1,
            waves: [steep, default(), default(), default()],
            ..default()
        };
        let offset = set.offset_at(1.0, 0.0, 0.0);
        assert!(offset.x.abs() > 1e-3, "steepness slides the surface along the wave");
        assert!(offset.z.abs() < 1e-6, "a x-direction swell never slides sideways");
    }

    #[test]
    fn key_tracks_override_shader_identity() {
        let uuid = Uuid::new_v4();
        let ext = WaterMaterialExt {
            override_shader: Some(Handle::from(uuid)),
            ..default()
        };
        let key = WaterMaterialKey::from(&ext);
        assert_eq!(key.override_shader, Some(uuid));
    }

    #[test]
    fn defs_carry_feature_flags() {
        let defs = water_shader_defs(&WaterMaterialKey::from(&key_ext()));
        assert!(defs.contains(&ShaderDefVal::Bool("WATER_FOAM".into(), true)));
        assert!(defs.contains(&ShaderDefVal::Bool("WATER_OPTICS".into(), false)));
        assert!(defs.contains(&ShaderDefVal::Bool("WATER_RIPPLES".into(), true)));
    }

    #[test]
    fn defs_reach_both_shader_stages() {
        let key = WaterMaterialKey::from(&WaterMaterialExt {
            body_foam: true,
            ..default()
        });
        let mut descriptor = RenderPipelineDescriptor::default();
        descriptor.fragment = Some(Default::default());
        apply_water_specialization(&mut descriptor, &key);
        for def in water_shader_defs(&key) {
            assert!(
                descriptor.vertex.shader_defs.contains(&def),
                "the vertex stage is missing {def:?}"
            );
            assert!(
                descriptor
                    .fragment
                    .as_ref()
                    .unwrap()
                    .shader_defs
                    .contains(&def),
                "the fragment stage is missing {def:?}"
            );
        }
    }

    #[test]
    fn detail_uniform_matches_the_shader_layout() {
        assert_eq!(WaterDetail::min_size().get(), 48);
        let source = include_str!("shaders/water/detail.wesl");
        for field in [
            "drift",
            "detail_scale",
            "flow_scale",
            "flow_speed",
            "flow_strength",
            "layer_ratio",
            "gradient_step",
            "blend_a",
            "blend_b",
            "normal_gain",
            "foam_gain",
        ] {
            assert!(
                source.contains(&format!("{field}:")),
                "detail.wesl is missing the `{field}` field"
            );
        }
        assert!(source.contains("@binding(105)"));
    }

    #[test]
    fn dye_uniform_matches_the_shader_layout() {
        assert_eq!(WaterDye::min_size().get(), 48);
        let source = include_str!("shaders/water/dye.wesl");
        for field in [
            "shallow",
            "deep",
            "beers_law",
            "depth_offset",
            "tint_exponent",
        ] {
            assert!(
                source.contains(&format!("{field}:")),
                "dye.wesl is missing the `{field}` field"
            );
        }
        assert!(source.contains("@binding(106)"));
    }

    #[test]
    fn caustics_uniform_matches_the_shader_layout() {
        assert_eq!(WaterCaustics::min_size().get(), 16);
        let source = include_str!("shaders/water/caustics.wesl");
        for field in ["strength", "scale", "speed", "sharpness"] {
            assert!(
                source.contains(&format!("{field}:")),
                "caustics.wesl is missing the `{field}` field"
            );
        }
        assert!(source.contains("@binding(107)"));
    }

    #[test]
    fn params_carry_the_refraction_strength() {
        let source = include_str!("shaders/water/types.wesl");
        assert!(source.contains("refraction: f32,"));
        assert_eq!(WaterParams::min_size().get(), 128);
    }

    #[test]
    fn defs_carry_the_ported_feature_flags() {
        let defs = water_shader_defs(&WaterMaterialKey::from(&WaterMaterialExt::default()));
        for name in [
            "WATER_DETAIL",
            "WATER_DYE",
            "WATER_CAUSTICS",
            "WATER_REFRACT",
        ] {
            assert!(
                defs.contains(&ShaderDefVal::Bool(name.into(), true)),
                "{name} must default on"
            );
        }
    }

    #[test]
    fn the_override_shader_replaces_only_the_fragment() {
        let uuid = Uuid::new_v4();
        let key = WaterMaterialKey::from(&WaterMaterialExt {
            override_shader: Some(Handle::from(uuid)),
            ..default()
        });
        let mut descriptor = RenderPipelineDescriptor::default();
        descriptor.fragment = Some(Default::default());
        let vertex_shader = descriptor.vertex.shader.clone();
        apply_water_specialization(&mut descriptor, &key);
        assert_eq!(descriptor.fragment.as_ref().unwrap().shader, Handle::from(uuid));
        assert_eq!(descriptor.vertex.shader, vertex_shader);
    }
}
