use bevy::{
    prelude::*,
    render::{Extract, ExtractSchedule, RenderApp},
};

use crate::compute::{MAX_EMITTERS, MAX_QUERY_POINTS, MAX_SPLINES, MAX_SPLINE_POINTS};

#[derive(Resource, Clone, Debug, Default)]
pub struct SeaFloorHeightGrid {
    pub image: Handle<Image>,
    pub origin: Vec2,
    pub texel: f32,
    pub size: u32,
    pub present: bool,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct DynWaveEmitter {
    pub position: Vec3,
    pub radius: f32,
    pub strength: f32,
    pub velocity: Vec2,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct FoamEmitter {
    pub position: Vec3,
    pub radius: f32,
    pub strength: f32,
}

#[derive(Component, Clone, Debug)]
pub struct FlowSpline {
    pub points: Vec<[f32; 3]>,
    pub width: f32,
    pub flow_speed: f32,
}

#[derive(Component, Clone, Debug)]
pub struct FoamSpline {
    pub points: Vec<[f32; 3]>,
    pub width: f32,
    pub foam_amount: f32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ClipCarve {
    pub center: Vec2,
    pub radius: f32,
}

#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct WaterQueryPoint {
    pub position: Option<Vec3>,
}

#[derive(Resource, Default, Debug)]
pub struct WaterQueryPoints {
    pub points: Vec<Vec3>,
}

#[derive(Resource, Default, Debug)]
pub struct ExtractedInputs {
    pub emitters: Vec<[f32; 4]>,
    pub emitter_velocities: Vec<[f32; 4]>,
    pub foam_emitters: Vec<[f32; 4]>,
    pub flow_spline_headers: Vec<[f32; 4]>,
    pub foam_spline_headers: Vec<[f32; 4]>,
    pub spline_points: Vec<[f32; 4]>,
    pub carves: Vec<[f32; 4]>,
    pub query_points: Vec<[f32; 4]>,
    pub grid: SeaFloorHeightGrid,
}

fn pack_splines(
    splines: &[&FlowSpline],
    points: &mut Vec<[f32; 4]>,
    headers: &mut Vec<[f32; 4]>,
) {
    for spline in splines.iter().take(MAX_SPLINES) {
        if points.len() + spline.points.len() > MAX_SPLINE_POINTS {
            break;
        }
        headers.push([
            points.len() as f32,
            spline.points.len() as f32,
            spline.width,
            spline.flow_speed,
        ]);
        points.extend(
            spline
                .points
                .iter()
                .map(|p| [p[0], p[1], p[2], 0.0]),
        );
    }
}

fn extract_inputs(
    mut extracted: ResMut<ExtractedInputs>,
    grid: Extract<Res<SeaFloorHeightGrid>>,
    emitters: Extract<Query<&DynWaveEmitter>>,
    foam_emitters: Extract<Query<&FoamEmitter>>,
    flow_splines: Extract<Query<&FlowSpline>>,
    foam_splines: Extract<Query<&FoamSpline>>,
    carves: Extract<Query<&ClipCarve>>,
    query_points: Extract<Res<WaterQueryPoints>>,
) {
    extracted.emitters.clear();
    extracted.emitter_velocities.clear();
    for emitter in &emitters {
        if extracted.emitters.len() >= MAX_EMITTERS {
            break;
        }
        extracted.emitters.push([
            emitter.position.x,
            emitter.position.z,
            emitter.radius,
            emitter.strength,
        ]);
        extracted.emitter_velocities.push([
            emitter.velocity.x,
            emitter.velocity.y,
            emitter.radius,
            emitter.velocity.length(),
        ]);
    }
    extracted.foam_emitters.clear();
    for emitter in &foam_emitters {
        if extracted.foam_emitters.len() >= MAX_EMITTERS {
            break;
        }
        extracted.foam_emitters.push([
            emitter.position.x,
            emitter.position.z,
            emitter.radius,
            emitter.strength,
        ]);
    }
    extracted.flow_spline_headers.clear();
    extracted.foam_spline_headers.clear();
    extracted.spline_points.clear();
    let extracted = extracted.bypass_change_detection();
    let flow_refs: Vec<&FlowSpline> = flow_splines.iter().collect();
    pack_splines(&flow_refs, &mut extracted.spline_points, &mut extracted.flow_spline_headers);
    for spline in foam_splines.iter().take(MAX_SPLINES) {
        if extracted.spline_points.len() + spline.points.len() > MAX_SPLINE_POINTS {
            break;
        }
        let offset = extracted.spline_points.len() as f32;
        extracted.foam_spline_headers.push([
            offset,
            spline.points.len() as f32,
            spline.width,
            spline.foam_amount,
        ]);
        extracted
            .spline_points
            .extend(spline.points.iter().map(|p| [p[0], p[1], p[2], 0.0]));
    }
    extracted.carves.clear();
    for carve in &carves {
        extracted.carves.push([carve.center.x, carve.center.y, carve.radius, 0.0]);
    }
    extracted.query_points.clear();
    for point in query_points.points.iter().take(MAX_QUERY_POINTS) {
        extracted.query_points.push([point.x, point.z, 0.0, 0.0]);
    }
    extracted.grid = grid.clone();
}

pub struct InputsPlugin;

impl Plugin for InputsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SeaFloorHeightGrid>();
        app.init_resource::<WaterQueryPoint>();
        app.init_resource::<WaterQueryPoints>();

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<ExtractedInputs>()
            .add_systems(ExtractSchedule, extract_inputs);
    }
}

pub fn f32_bytes(values: &[[f32; 4]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 16);
    for value in values {
        for component in value {
            bytes.extend_from_slice(&component.to_le_bytes());
        }
    }
    bytes
}
