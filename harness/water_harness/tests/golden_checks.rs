use std::time::Duration;

use bevy::{
    app::PluginsState, prelude::*, render::RenderPlugin,
    tasks::tick_global_task_pools_on_main_thread, time::TimeUpdateStrategy, winit::WinitPlugin,
};
use crest_water::{
    CrestWaterPlugin, DynWaveEmitter, FlowSpline, OceanViewer, SeaFloorHeightGrid, WaterQueryPoint,
    WaterSampler,
};
use water_harness::golden::read_golden;
use water_harness::grid::seabed_grid_image;
use water_harness::scenarios::{FREEZE_FRAMES, QUERY, SIM_DT, ScenarioResource, VIEWER, scenarios};

const HEIGHT_TOL: f32 = 0.02;
const FLOW_TOL: f32 = 0.02;
const DEPTH_TOL: f32 = 0.1;

#[derive(Resource, Deref, DerefMut, Default)]
struct SimFrame(u64);

fn setup(
    scenario: Res<ScenarioResource>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
) {
    let scenario = &scenario.0;
    let image = images.add(seabed_grid_image(&scenario.seabed));
    commands.insert_resource(SeaFloorHeightGrid {
        image,
        origin: Vec2::new(-64.0, -64.0),
        texel: 1.0,
        size: 129,
        present: true,
    });
    commands.insert_resource(WaterQueryPoint {
        position: Some(QUERY),
    });
    commands.insert_resource(OceanViewer { position: VIEWER });
    for spline in &scenario.splines {
        commands.spawn(FlowSpline {
            points: spline.points.clone(),
            width: spline.width,
            flow_speed: spline.flow_speed,
        });
    }
}

fn drive_wake(
    scenario: Res<ScenarioResource>,
    mut frame: ResMut<SimFrame>,
    mut commands: Commands,
    existing: Query<Entity, With<DynWaveEmitter>>,
) {
    frame.0 += 1;
    let frame_index = frame.0;
    let scenario = &scenario.0;
    let active = scenario
        .wake
        .as_ref()
        .filter(|wake| frame_index > wake.start_frame && frame_index <= scenario.frames);
    match active {
        Some(wake) => {
            let elapsed = (frame_index - wake.start_frame) as f32 * SIM_DT;
            let position = wake.start + wake.direction * (wake.speed * elapsed);
            let emitter = DynWaveEmitter {
                position: Vec3::new(position.x, 0.0, position.y),
                radius: wake.radius,
                strength: wake.strength,
                velocity: wake.direction * wake.velocity_strength,
            };
            match existing.single() {
                Ok(entity) => {
                    commands.entity(entity).insert(emitter);
                }
                Err(_) => {
                    commands.spawn(emitter);
                }
            }
        }
        None => {
            for entity in existing.iter() {
                commands.entity(entity).remove::<DynWaveEmitter>();
            }
        }
    }
}

fn run_scenario(name: &str) {
    let scenario = scenarios()
        .into_iter()
        .find(|scenario| scenario.name == name)
        .expect("scenario exists");
    let golden = read_golden(name)
        .expect("goldens missing: run cargo run -p water_harness --bin generate_goldens");

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    visible: false,
                    ..default()
                }),
                ..default()
            })
            .set(WinitPlugin {
                run_on_any_thread: true,
            })
            .set(RenderPlugin {
                synchronous_pipeline_compilation: true,
                ..default()
            }),
    )
    .add_plugins(CrestWaterPlugin)
    .insert_resource(scenario.settings.clone())
    .insert_resource(ScenarioResource(scenario.clone()))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        SIM_DT,
    )))
    .init_resource::<SimFrame>()
    .add_systems(Startup, setup)
    .add_systems(Update, drive_wake);

    while app.plugins_state() == PluginsState::Adding {
        tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();

    for _ in 0..scenario.frames {
        tick_global_task_pools_on_main_thread();
        app.update();
    }
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    for _ in 0..FREEZE_FRAMES as usize + 40 {
        tick_global_task_pools_on_main_thread();
        app.update();
    }

    let sampler = app.world().resource::<WaterSampler>();
    assert!(sampler.ready, "{name}: readback never became ready");
    for sample in &golden.samples {
        let probe = Vec2::from(golden.probes[sample.probe]);
        let got = sampler
            .sample(probe, scenario.settings.water_level)
            .expect("probe inside slice");
        let label = format!("{name} probe {} at ({}, {})", sample.probe, probe.x, probe.y);
        assert!(
            (got.height - sample.height).abs() < HEIGHT_TOL,
            "{label}: height {} vs golden {}",
            got.height,
            sample.height
        );
        assert!(
            (got.displacement.x - sample.disp_x).abs() < HEIGHT_TOL,
            "{label}: disp_x {} vs golden {}",
            got.displacement.x,
            sample.disp_x
        );
        assert!(
            (got.displacement.y - sample.disp_y).abs() < HEIGHT_TOL,
            "{label}: disp_y {} vs golden {}",
            got.displacement.y,
            sample.disp_y
        );
        assert!(
            (got.displacement.z - sample.disp_z).abs() < HEIGHT_TOL,
            "{label}: disp_z {} vs golden {}",
            got.displacement.z,
            sample.disp_z
        );
        assert!(
            (got.jacobian - sample.jacobian).abs() < 0.05 + 0.02 * sample.jacobian.abs(),
            "{label}: jacobian {} vs golden {}",
            got.jacobian,
            sample.jacobian
        );
        assert!(
            (got.flow.x - sample.flow_x).abs() < FLOW_TOL
                && (got.flow.y - sample.flow_z).abs() < FLOW_TOL,
            "{label}: flow {:?} vs golden ({}, {})",
            got.flow,
            sample.flow_x,
            sample.flow_z
        );
        assert!(
            (got.depth - sample.depth).abs() < DEPTH_TOL,
            "{label}: depth {} vs golden {}",
            got.depth,
            sample.depth
        );
    }
}

#[test]
fn calm_ocean_matches_golden() {
    run_scenario("calm_ocean");
}

#[test]
fn storm_swell_matches_golden() {
    run_scenario("storm_swell");
}

#[test]
fn shallow_shore_matches_golden() {
    run_scenario("shallow_shore");
}

#[test]
fn spline_river_matches_golden() {
    run_scenario("spline_river");
}

#[test]
fn boat_wakes_matches_golden() {
    run_scenario("boat_wakes");
}

#[test]
fn underwater_curtain_matches_golden() {
    run_scenario("underwater_curtain");
}
