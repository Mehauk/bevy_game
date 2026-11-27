use bevy::{prelude::*, render::render_resource::ShaderType};

use crate::settings::OceanSettings;

pub const MAX_LODS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct LodTransform {
    pub origin: Vec3,
    pub texel_size: f32,
    pub extent: f32,
    pub lod_index: f32,
    pub pad: f32,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct LodTransforms {
    pub transforms: [LodTransform; MAX_LODS],
    pub count: usize,
}

impl LodTransforms {
    pub fn compute(settings: &OceanSettings, viewer: Vec3) -> Self {
        let mut transforms = [LodTransform::default(); MAX_LODS];
        let count = settings.lod_count.min(MAX_LODS);
        for lod in 0..count {
            let texel = settings.texel_size(lod);
            let extent = settings.cascade_extent(lod);
            let snapped = (viewer.xz() / texel).floor() * texel;
            transforms[lod] = LodTransform {
                origin: Vec3::new(snapped.x - extent * 0.5, viewer.y, snapped.y - extent * 0.5),
                texel_size: texel,
                extent,
                lod_index: lod as f32,
                pad: 0.0,
            };
        }
        Self { transforms, count }
    }

    pub fn lod_for_distance(&self, distance: f32) -> usize {
        for lod in 0..self.count {
            if distance <= self.transforms[lod].extent * 0.5 {
                return lod;
            }
        }
        self.count.saturating_sub(1)
    }
}

pub fn cascade_uv(transform: &LodTransform, world_xz: Vec2) -> Vec2 {
    (world_xz - transform.origin.xz()) / transform.extent
}

pub fn cascade_blend(transform: &LodTransform, world_xz: Vec2) -> f32 {
    let uv = cascade_uv(transform, world_xz);
    let edge = (uv - 0.5).abs();
    let max_edge = edge.x.max(edge.y);
    smoothstep(0.4, 0.5, max_edge)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
