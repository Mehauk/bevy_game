use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension, StandardMaterial},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

pub const MAX_SURFACE_DEBUG_CELLS: usize = 64;
const SURFACE_DEBUG_CELL_GROUPS: usize = MAX_SURFACE_DEBUG_CELLS / 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Reflect)]
pub struct SurfaceDebug {
    pub origin: Vec2,
    pub cell: Vec2,
    pub count: Vec2,
    pub line_width: f32,
    pub tint_alpha: f32,
    pub mode: u32,
    pub use_values: u32,
    pub cell_values: [UVec4; SURFACE_DEBUG_CELL_GROUPS],
}

impl Default for SurfaceDebug {
    fn default() -> Self {
        Self {
            origin: Vec2::ZERO,
            cell: Vec2::ONE,
            count: Vec2::ZERO,
            line_width: 1.5,
            tint_alpha: 0.3,
            mode: 0,
            use_values: 0,
            cell_values: [UVec4::ZERO; SURFACE_DEBUG_CELL_GROUPS],
        }
    }
}

impl SurfaceDebug {
    pub fn set_cell(&mut self, index: usize, value: u32) {
        if let Some(group) = self.cell_values.get_mut(index / 4) {
            group[index % 4] = value;
        }
    }
}

pub fn region_color(id: u32) -> [f32; 3] {
    let mut h = id.wrapping_mul(0x9E37_79B9);
    h = (h ^ (h >> 15)).wrapping_mul(0x85EB_CA6B);
    h = (h ^ (h >> 13)).wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    [
        0.35 + 0.65 * (h & 0xFF) as f32 / 255.0,
        0.35 + 0.65 * ((h >> 8) & 0xFF) as f32 / 255.0,
        0.35 + 0.65 * ((h >> 16) & 0xFF) as f32 / 255.0,
    ]
}

#[repr(C)]
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct DebugCurtainParams {
    pub alpha: f32,
}

impl Default for DebugCurtainParams {
    fn default() -> Self {
        Self { alpha: 0.35 }
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct DebugCurtainExt {
    #[uniform(100)]
    pub params: DebugCurtainParams,
}

impl MaterialExtension for DebugCurtainExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/debug/curtain.wesl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/debug/curtain.wesl".into()
    }
}

pub type DebugCurtainMaterial = ExtendedMaterial<StandardMaterial, DebugCurtainExt>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_surface_debug_draws_nothing() {
        let debug = SurfaceDebug::default();
        assert_eq!(debug.mode, 0);
        assert_eq!(debug.cell_values, [UVec4::ZERO; SURFACE_DEBUG_CELL_GROUPS]);
    }

    #[test]
    fn set_cell_addresses_the_flat_cell_index() {
        let mut debug = SurfaceDebug::default();
        debug.set_cell(0, 7);
        debug.set_cell(5, 9);
        debug.set_cell(MAX_SURFACE_DEBUG_CELLS, 1);
        assert_eq!(debug.cell_values[0], UVec4::new(7, 0, 0, 0));
        assert_eq!(debug.cell_values[1], UVec4::new(0, 0, 9, 0));
        assert_eq!(debug.cell_values[15], UVec4::ZERO, "out of range writes nothing");
    }

    #[test]
    fn region_colors_stay_pastel_and_distinct() {
        let first = region_color(1);
        let second = region_color(2);
        for channel in first.into_iter().chain(second) {
            assert!((0.35..=1.0).contains(&channel));
        }
        assert_ne!(first, second);
        assert_eq!(region_color(1), first, "the palette is deterministic");
    }

    #[test]
    fn region_zero_is_finite() {
        assert!(region_color(0).into_iter().all(|c| c.is_finite()));
    }
}
