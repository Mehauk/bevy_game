use bevy::{prelude::*, render::gpu_readback::{Readback, ReadbackComplete}};

use crate::cascades::{QueryPointTextures, QuerySlices, QUERY_SLICE_SIZE};
use crate::compute::MAX_QUERY_POINTS;
use crate::inputs::{WaterQueryPoint, WaterQueryPoints};
use crate::lod::LodTransforms;
use crate::reference::query::{fpi_surface_height, sample_bilinear};

#[derive(Clone, Copy, Debug, Default)]
pub struct WaterSample {
    pub height: f32,
    pub displacement: Vec3,
    pub normal: Vec3,
    pub jacobian: f32,
    pub flow: Vec2,
    pub depth: f32,
}

#[derive(Component)]
struct ReadAnimated;

#[derive(Component)]
struct ReadFlow;

#[derive(Component)]
struct ReadDepth;

#[derive(Resource, Default)]
pub struct WaterSampler {
    pub point: Option<Vec3>,
    pub origin: Vec2,
    pub texel: f32,
    pub height: Vec<f32>,
    pub disp_x: Vec<f32>,
    pub disp_z: Vec<f32>,
    pub jacobian: Vec<f32>,
    pub flow_x: Vec<f32>,
    pub flow_z: Vec<f32>,
    pub depth: Vec<f32>,
    pub ready: bool,
}

fn f32s_from(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("chunk of four")))
        .collect()
}

fn ensure_readback_entities(mut commands: Commands, slices: Res<QuerySlices>, existing: Query<Entity, With<ReadAnimated>>) {
    if !existing.is_empty() || slices.animated == Handle::default() {
        return;
    }
    commands.spawn((Readback::texture(slices.animated.clone()), ReadAnimated));
    commands.spawn((Readback::texture(slices.flow.clone()), ReadFlow));
    commands.spawn((Readback::texture(slices.depth.clone()), ReadDepth));
}

fn track_query_point(
    query_point: Res<WaterQueryPoint>,
    lods: Res<LodTransforms>,
    mut sampler: ResMut<WaterSampler>,
) {
    let Some(point) = query_point.position else {
        sampler.ready = false;
        sampler.point = None;
        return;
    };
    let texel = lods.transforms.first().map(|t| t.texel_size).unwrap_or(0.0);
    if texel <= 0.0 {
        return;
    }
    if sampler.point != Some(point) || sampler.texel != texel {
        sampler.point = Some(point);
        sampler.texel = texel;
        sampler.origin = point.xz() - Vec2::splat(QUERY_SLICE_SIZE as f32 * texel * 0.5);
        sampler.ready = false;
    }
}

fn collect_slices(
    trigger: On<ReadbackComplete>,
    animated: Query<(), With<ReadAnimated>>,
    flow: Query<(), With<ReadFlow>>,
    depth: Query<(), With<ReadDepth>>,
    mut sampler: ResMut<WaterSampler>,
) {
    let pixels = QUERY_SLICE_SIZE * QUERY_SLICE_SIZE;
    let entity = trigger.entity;
    if animated.contains(entity) {
        let values = f32s_from(&trigger.data);
        if values.len() >= pixels * 4 {
            sampler.disp_x = values.chunks_exact(4).map(|t| t[0]).collect();
            sampler.height = values.chunks_exact(4).map(|t| t[1]).collect();
            sampler.disp_z = values.chunks_exact(4).map(|t| t[2]).collect();
            sampler.jacobian = values.chunks_exact(4).map(|t| t[3]).collect();
        }
    } else if flow.contains(entity) {
        let values = f32s_from(&trigger.data);
        if values.len() >= pixels * 4 {
            sampler.flow_x = values.chunks_exact(4).map(|t| t[0]).collect();
            sampler.flow_z = values.chunks_exact(4).map(|t| t[1]).collect();
        }
    } else if depth.contains(entity) {
        let values = f32s_from(&trigger.data);
        if values.len() >= pixels * 4 {
            sampler.depth = values.chunks_exact(4).map(|t| t[0]).collect();
        }
    }
    sampler.ready = sampler.height.len() == pixels
        && sampler.flow_x.len() == pixels
        && sampler.depth.len() == pixels
        && sampler.texel > 0.0;
}

impl WaterSampler {
    pub fn sample(&self, world_xz: Vec2, water_level: f32) -> Option<WaterSample> {
        if !self.ready {
            return None;
        }
        let size = QUERY_SLICE_SIZE;
        let extent = size as f32 * self.texel;
        let uv = (world_xz - self.origin) / extent;
        if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
            return None;
        }
        let height = sample_bilinear(&self.height, size, uv.x, uv.y);
        let step = self.texel / extent;
        let dhdx = (sample_bilinear(&self.height, size, uv.x + step, uv.y)
            - sample_bilinear(&self.height, size, uv.x - step, uv.y))
            / (2.0 * self.texel);
        let dhdz = (sample_bilinear(&self.height, size, uv.x, uv.y + step)
            - sample_bilinear(&self.height, size, uv.x, uv.y - step))
            / (2.0 * self.texel);
        Some(WaterSample {
            height: water_level + height,
            displacement: Vec3::new(
                sample_bilinear(&self.disp_x, size, uv.x, uv.y),
                height,
                sample_bilinear(&self.disp_z, size, uv.x, uv.y),
            ),
            normal: Vec3::new(-dhdx, 1.0, -dhdz).normalize(),
            jacobian: sample_bilinear(&self.jacobian, size, uv.x, uv.y),
            flow: Vec2::new(
                sample_bilinear(&self.flow_x, size, uv.x, uv.y),
                sample_bilinear(&self.flow_z, size, uv.x, uv.y),
            ),
            depth: sample_bilinear(&self.depth, size, uv.x, uv.y),
        })
    }

    pub fn ray_hit(
        &self,
        water_level: f32,
        ray_origin: Vec3,
        ray_dir: Vec3,
        max_distance: f32,
    ) -> Option<f32> {
        let height_at = |x: f32, z: f32| {
            self.sample(Vec2::new(x, z), water_level)
                .map(|s| s.height)
                .unwrap_or(water_level)
        };
        fpi_surface_height(height_at, ray_origin, ray_dir, max_distance)
    }
}

#[derive(Component)]
struct ReadQueryAnimated;

#[derive(Component)]
struct ReadQueryFlow;

#[derive(Component)]
struct ReadQueryDepth;

#[derive(Resource, Default)]
pub struct WaterQuery {
    pub count: usize,
    pub animated: Vec<[f32; 4]>,
    pub flow: Vec<[f32; 2]>,
    pub depth: Vec<[f32; 3]>,
    pub ready: bool,
}

impl WaterQuery {
    pub fn sample(&self, slot: usize, water_level: f32) -> Option<WaterSample> {
        if !self.ready || slot >= self.count {
            return None;
        }
        let animated = *self.animated.get(slot)?;
        let flow = *self.flow.get(slot)?;
        let depth = *self.depth.get(slot)?;
        let height = animated[1];
        Some(WaterSample {
            height: water_level + height,
            displacement: Vec3::new(animated[0], height, animated[2]),
            normal: Vec3::new(-depth[1], 1.0, -depth[2]).normalize(),
            jacobian: animated[3],
            flow: Vec2::new(flow[0], flow[1]),
            depth: depth[0],
        })
    }
}

fn ensure_query_readback_entities(
    mut commands: Commands,
    textures: Res<QueryPointTextures>,
    existing: Query<Entity, With<ReadQueryAnimated>>,
) {
    if !existing.is_empty() || textures.animated == Handle::default() {
        return;
    }
    commands.spawn((Readback::texture(textures.animated.clone()), ReadQueryAnimated));
    commands.spawn((Readback::texture(textures.flow.clone()), ReadQueryFlow));
    commands.spawn((Readback::texture(textures.depth.clone()), ReadQueryDepth));
}

fn track_query_points(points: Res<WaterQueryPoints>, mut query: ResMut<WaterQuery>) {
    query.count = points.points.len().min(MAX_QUERY_POINTS);
}

fn collect_query_points(
    trigger: On<ReadbackComplete>,
    animated: Query<(), With<ReadQueryAnimated>>,
    flow: Query<(), With<ReadQueryFlow>>,
    depth: Query<(), With<ReadQueryDepth>>,
    mut query: ResMut<WaterQuery>,
) {
    let entity = trigger.entity;
    let values = f32s_from(&trigger.data);
    if animated.contains(entity) {
        query.animated = values.chunks_exact(4).map(|t| [t[0], t[1], t[2], t[3]]).collect();
    } else if flow.contains(entity) {
        query.flow = values.chunks_exact(4).map(|t| [t[0], t[1]]).collect();
    } else if depth.contains(entity) {
        query.depth = values.chunks_exact(4).map(|t| [t[0], t[1], t[2]]).collect();
    }
    query.ready = query.animated.len() == MAX_QUERY_POINTS
        && query.flow.len() == MAX_QUERY_POINTS
        && query.depth.len() == MAX_QUERY_POINTS;
}

pub struct QueryPlugin;

impl Plugin for QueryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaterSampler>()
            .init_resource::<WaterQuery>()
            .add_systems(PreUpdate, (track_query_point, ensure_readback_entities).chain())
            .add_systems(
                PreUpdate,
                (track_query_points, ensure_query_readback_entities).chain(),
            )
            .add_observer(collect_slices)
            .add_observer(collect_query_points);
    }
}
