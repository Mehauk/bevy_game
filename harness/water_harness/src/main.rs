use bevy::{
    core_pipeline::prepass::DepthPrepass,
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    window::Window,
};
use crest_water::{
    CrestWaterPlugin, DynWaveEmitter, FlowSpline, OceanViewer, SeaFloorHeightGrid, WaterQueryPoint,
    material::{OceanShading, OceanSurfaceExt, OceanSurfaceMaterial, spawn_ocean_surface},
    underwater::UnderwaterFog,
};
use water_harness::{
    grid::seabed_grid_image,
    scenarios::{Scenario, ScenarioResource, scenarios},
};

#[derive(Resource, Default)]
struct CameraRig {
    yaw: f32,
    pitch: f32,
    speed: f32,
}

#[derive(Resource)]
struct Sandbox {
    scenarios: Vec<Scenario>,
    index: usize,
    wake_frame: u64,
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(16.0, 3.0, 24.0).looking_at(Vec3::new(16.0, 0.0, 16.0), Vec3::Y),
        DepthPrepass,
        UnderwaterFog::default(),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 30000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_rotation_x(-0.6).mul_quat(Quat::from_rotation_y(0.4))),
    ));
}

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<OceanSurfaceMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let material = materials.add(OceanSurfaceMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.05, 0.25, 0.35),
            perceptual_roughness: 0.1,
            ..default()
        },
        extension: OceanSurfaceExt {
            shading: OceanShading::default(),
            ..default()
        },
    });
    spawn_ocean_surface(&mut commands, &mut meshes, material);
    let scenario = scenarios().remove(0);
    apply_scenario(
        &scenario,
        &mut commands,
        &mut images,
    );
    commands.insert_resource(ScenarioResource(scenario));
    commands.insert_resource(Sandbox {
        scenarios: scenarios(),
        index: 0,
        wake_frame: 0,
    });
}

fn apply_scenario(
    scenario: &Scenario,
    commands: &mut Commands,
    images: &mut Assets<Image>,
) {
    let image = images.add(seabed_grid_image(&scenario.seabed));
    commands.insert_resource(SeaFloorHeightGrid {
        image,
        origin: Vec2::new(-64.0, -64.0),
        texel: 1.0,
        size: 129,
        present: true,
    });
    commands.insert_resource(scenario.settings.clone());
    commands.insert_resource(WaterQueryPoint {
        position: Some(Vec3::new(16.0, 0.0, 16.0)),
    });
}

fn switch_scenario(
    keys: Res<ButtonInput<KeyCode>>,
    mut sandbox: ResMut<Sandbox>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    old_splines: Query<Entity, With<FlowSpline>>,
) {
    let pressed = [
        (KeyCode::Digit1, 0),
        (KeyCode::Digit2, 1),
        (KeyCode::Digit3, 2),
        (KeyCode::Digit4, 3),
        (KeyCode::Digit5, 4),
        (KeyCode::Digit6, 5),
    ];
    let Some((_, index)) = pressed.iter().find(|(key, _)| keys.just_pressed(*key)) else {
        return;
    };
    sandbox.index = *index;
    sandbox.wake_frame = 0;
    let scenario = sandbox.scenarios[*index].clone();
    for entity in &old_splines {
        commands.entity(entity).despawn();
    }
    for spline in &scenario.splines {
        commands.spawn(FlowSpline {
            points: spline.points.clone(),
            width: spline.width,
            flow_speed: spline.flow_speed,
        });
    }
    apply_scenario(&scenario, &mut commands, &mut images);
    commands.insert_resource(ScenarioResource(scenario));
    info!("scenario {}", sandbox.index + 1);
}

fn fly_camera(
    keys: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    mut rig: ResMut<CameraRig>,
    mut cameras: Single<&mut Transform, With<Camera3d>>,
    mut viewer: ResMut<OceanViewer>,
    mut query_point: ResMut<WaterQueryPoint>,
) {
    let mouse_turn = |mouse: &mut MessageReader<MouseMotion>| {
        let mut delta = Vec2::ZERO;
        for event in mouse.read() {
            delta += event.delta;
        }
        delta
    };
    let turn = mouse_turn(&mut mouse_motion);
    rig.yaw -= turn.x * 0.0025;
    rig.pitch = (rig.pitch - turn.y * 0.0025).clamp(-1.45, 1.45);
    for event in mouse_wheel.read() {
        rig.speed = (rig.speed + event.y * 0.2).clamp(0.5, 40.0);
    }
    let forward = Vec3::new(rig.yaw.sin() * rig.pitch.cos(), rig.pitch.sin(), rig.yaw.cos() * rig.pitch.cos());
    let right = Vec3::new(forward.z, 0.0, -forward.x);
    let mut motion = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        motion += forward;
    }
    if keys.pressed(KeyCode::KeyS) {
        motion -= forward;
    }
    if keys.pressed(KeyCode::KeyD) {
        motion += right;
    }
    if keys.pressed(KeyCode::KeyA) {
        motion -= right;
    }
    if keys.pressed(KeyCode::KeyE) {
        motion += Vec3::Y;
    }
    if keys.pressed(KeyCode::KeyQ) {
        motion -= Vec3::Y;
    }
    if motion != Vec3::ZERO {
        cameras.translation += motion.normalize() * rig.speed * 0.016;
    }
    cameras.look_to(forward, Vec3::Y);
    viewer.position = cameras.translation;
    query_point.position = Some(cameras.translation);
}

fn drive_wake(
    mut sandbox: ResMut<Sandbox>,
    scenario: Res<ScenarioResource>,
    mut commands: Commands,
    existing: Query<Entity, With<DynWaveEmitter>>,
) {
    sandbox.wake_frame += 1;
    let frame = sandbox.wake_frame;
    let scenario = &scenario.0;
    let active = scenario
        .wake
        .as_ref()
        .filter(|wake| frame > wake.start_frame && frame <= scenario.frames);
    match active {
        Some(wake) => {
            let elapsed = (frame - wake.start_frame) as f32 / 60.0;
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

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "water harness".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CrestWaterPlugin)
        .init_resource::<CameraRig>()
        .init_resource::<OceanViewer>()
        .add_systems(Startup, (spawn_camera, spawn_world).chain())
        .add_systems(
            Update,
            (fly_camera, switch_scenario, drive_wake).chain(),
        )
        .run();
}
