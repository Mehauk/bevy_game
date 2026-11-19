use avian3d::prelude::*;
use bevy::{prelude::*, transform::TransformSystems};
use bevy_enhanced_input::prelude::*;

use crate::{
    camera::{CameraRig, CameraSystems, CameraTarget},
    input::{ContextActive, GameplayInput, HotkeyLabel, MovementBinding},
    terrain::TerrainMap,
    utils::commands::{CommandAppExt, CommandArgs, command},
};

pub const CHARACTER_SPAWN_OFFSET_Y: f32 = 1.5;
pub const CAPSULE_RADIUS: f32 = 0.4;
pub const CAPSULE_HEIGHT: f32 = 1.0;

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_character)
            .add_systems(FixedUpdate, (character_ground_check, character_physics_move).chain())
            .add_systems(Update, character_rotate_body)
            .add_systems(
                PostUpdate,
                sync_body_visibility
                    .after(CameraSystems::UpdateTransform)
                    .before(TransformSystems::Propagate),
            );

        command!(app, "Respawn character", respawn_character);
        command!(
            app,
            "Set walk and sprint speed: <walk> [sprint] (e.g. 'speed 10 20')",
            set_character_speed
        );
        command!(
            app,
            "Increase character speed: <walk_add> [sprint_add] (e.g. 'increase_speed 2 4')",
            increase_character_speed
        );
    }
}

#[derive(InputAction)]
#[action_output(Vec2)]
struct Move;

#[derive(InputAction)]
#[action_output(bool)]
struct Sprint;

#[derive(InputAction)]
#[action_output(bool)]
struct AlignToCamera;

#[derive(InputAction)]
#[action_output(bool)]
struct Jump;

#[derive(InputAction)]
#[action_output(bool)]
struct Crouch;

#[derive(Component, Clone)]
pub struct Character {
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub jump_speed: f32,
    pub turn_speed: f32,
    pub acceleration: f32,
    pub air_acceleration: f32,
    pub jump_cooldown: f32,
    pub base_gravity: f32,
    pub fall_gravity: f32,
    pub max_slope_angle_rad: f32,
}

impl Default for Character {
    fn default() -> Self {
        Self {
            walk_speed: 6.0,
            sprint_speed: 12.0,
            jump_speed: 11.0,
            turn_speed: 12.0,
            acceleration: 60.0,
            air_acceleration: 15.0,
            jump_cooldown: 0.2,
            base_gravity: 2.5,
            fall_gravity: 3.5,
            max_slope_angle_rad: 50.0_f32.to_radians(),
        }
    }
}

#[derive(Component, Default)]
pub struct GroundState {
    pub is_grounded: bool,
    pub ground_normal: Vec3,
    pub ground_distance: f32,
    pub jump_cooldown_timer: f32,
    pub jump_held_last_frame: bool,
}

#[derive(Component)]
struct CharacterBody;

pub fn camera_relative_direction(move_vec: Vec2, cam_yaw: f32) -> Vec3 {
    let yaw_rot = Quat::from_rotation_y(cam_yaw);
    let forward = yaw_rot * Vec3::NEG_Z;
    let right = yaw_rot * Vec3::X;
    right * move_vec.x + forward * move_vec.y
}

fn setup_character(world: &mut World) {
    let capsule_mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Capsule3d::new(CAPSULE_RADIUS, CAPSULE_HEIGHT));
    let visor_mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::new(0.45, 0.18, 0.2));
    let body_mat = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(Color::srgb(0.2, 0.45, 0.85));
    let visor_mat = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(Color::srgb(0.1, 0.12, 0.18));

    let reset_settings = ActionSettings {
        require_reset: true,
        ..default()
    };
    let spawn_y = world
        .get_resource::<TerrainMap>()
        .map_or(0.0, |t| t.height_at(0.0, 0.0))
        + CHARACTER_SPAWN_OFFSET_Y;

    let mut char_entity = world
        .spawn_scene(bsn! {
            Character
            Transform::from_xyz(0.0, spawn_y, 0.0)
            Visibility::default()
            GameplayInput
            Actions<GameplayInput> [
                Action::<Move>::new()
                reset_settings
                Bindings [
                    Binding(KeyCode::KeyW)
                    HotkeyLabel("Move Forward")
                    MovementBinding
                    ~SwizzleAxis::YXZ
                    --
                    Binding(KeyCode::KeyS)
                    HotkeyLabel("Move Back")
                    MovementBinding
                    ~Negate::all()
                    ~SwizzleAxis::YXZ
                    --
                    Binding(KeyCode::KeyD)
                    HotkeyLabel("Move Right")
                    MovementBinding
                    --
                    Binding(KeyCode::KeyA)
                    HotkeyLabel("Move Left")
                    MovementBinding
                    ~Negate::all()
                ]
                --
                Action::<Sprint>::new()
                reset_settings
                HotkeyLabel("Sprint")
                Bindings [
                    Binding(KeyCode::ShiftLeft)
                    --
                    Binding(KeyCode::ShiftRight)
                ]
                --
                Action::<AlignToCamera>::new()
                reset_settings
                HotkeyLabel("Align Camera")
                Bindings [
                    Binding(KeyCode::AltLeft)
                    --
                    Binding(KeyCode::AltRight)
                ]
                --
                Action::<Jump>::new()
                reset_settings
                HotkeyLabel("Jump")
                Bindings [
                    Binding(KeyCode::Space)
                    MovementBinding
                ]
                --
                Action::<Crouch>::new()
                reset_settings
                HotkeyLabel("Crouch")
                Bindings [
                    Binding(KeyCode::ControlLeft)
                    --
                    Binding(KeyCode::ControlRight)
                ]
            ]
        })
        .unwrap();

    let entity_id = char_entity.id();

    char_entity
        .insert((
            CameraTarget::default(),
            ContextActive(true),
            ContextActivity::<GameplayInput>::ACTIVE,
            RigidBody::Dynamic,
            Collider::capsule(CAPSULE_RADIUS, CAPSULE_HEIGHT),
            LockedAxes::ROTATION_LOCKED,
            Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
            Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
            LinearVelocity::default(),
            GravityScale(2.5),
            GroundState::default(),
            TransformInterpolation,
            ShapeCaster::new(
                Collider::sphere(0.35),
                Vec3::new(0.0, -0.6, 0.0),
                Quat::IDENTITY,
                Dir3::NEG_Y,
            )
            .with_max_distance(0.5)
            .with_query_filter(SpatialQueryFilter::from_excluded_entities([entity_id])),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    CharacterBody,
                    Mesh3d(capsule_mesh),
                    MeshMaterial3d(body_mat),
                    Transform::default(),
                    Visibility::default(),
                ))
                .with_children(|body| {
                    body.spawn((
                        Mesh3d(visor_mesh),
                        MeshMaterial3d(visor_mat),
                        Transform::from_xyz(0.0, 0.45, -0.35),
                    ));
                });
        });
}

fn character_ground_check(
    time: Res<Time>,
    mut query: Query<(&Character, &ShapeHits, &mut GroundState)>,
) {
    let dt = time.delta_secs();
    for (character, hits, mut ground_state) in &mut query {
        ground_state.jump_cooldown_timer = (ground_state.jump_cooldown_timer - dt).max(0.0);

        if ground_state.jump_cooldown_timer > 0.0 {
            ground_state.is_grounded = false;
            ground_state.ground_normal = Vec3::Y;
            ground_state.ground_distance = 0.0;
            continue;
        }

        if let Some(hit) = hits.iter_sorted().next() {
            if hit.distance <= 0.45 && hit.normal1.angle_between(Vec3::Y) <= character.max_slope_angle_rad {
                ground_state.is_grounded = true;
                ground_state.ground_normal = hit.normal1.normalize();
                ground_state.ground_distance = hit.distance;
                continue;
            }
        }

        ground_state.is_grounded = false;
        ground_state.ground_normal = Vec3::Y;
        ground_state.ground_distance = 0.0;
    }
}

fn character_physics_move(
    time: Res<Time>,
    move_action: Single<&Action<Move>>,
    sprint_action: Single<&Action<Sprint>>,
    jump_action: Single<&Action<Jump>>,
    _crouch_action: Single<&Action<Crouch>>,
    cam_rig: Option<Single<&CameraRig>>,
    character_query: Single<(
        &Character,
        &mut GroundState,
        &mut LinearVelocity,
        &mut GravityScale,
    )>,
) {
    let (character, mut ground, mut velocity, mut gravity_scale) = character_query.into_inner();
    let dt = time.delta_secs();

    let move_vec = ***move_action;
    let sprinting = ***sprint_action;
    let jumping = ***jump_action;
    let cam_yaw = cam_rig.as_ref().map_or(0.0, |rig| rig.yaw);

    let move_dir = camera_relative_direction(move_vec, cam_yaw).normalize_or_zero();

    let target_speed = if sprinting {
        character.sprint_speed
    } else {
        character.walk_speed
    };
    let target_horiz = move_dir * target_speed;

    let accel = if ground.is_grounded {
        character.acceleration
    } else {
        character.air_acceleration
    };

    let mut current_horiz = Vec3::new(velocity.x, 0.0, velocity.z);
    let diff = target_horiz - current_horiz;
    let max_change = accel * dt;
    if diff.length_squared() <= max_change * max_change {
        current_horiz = target_horiz;
    } else {
        current_horiz += diff.normalize() * max_change;
    }

    velocity.x = current_horiz.x;
    velocity.z = current_horiz.z;

    let jump_just_pressed = jumping && !ground.jump_held_last_frame;

    if jump_just_pressed && ground.is_grounded && ground.jump_cooldown_timer <= 0.0 {
        velocity.y = character.jump_speed;
        ground.is_grounded = false;
        ground.jump_cooldown_timer = character.jump_cooldown;
        gravity_scale.0 = character.base_gravity;
    } else if ground.is_grounded {
        gravity_scale.0 = 0.0;

        let has_horiz_vel = velocity.x.abs() > 0.001 || velocity.z.abs() > 0.001;
        if has_horiz_vel && ground.ground_normal.y > 0.01 {
            let slope_tangent_y = -(velocity.x * ground.ground_normal.x + velocity.z * ground.ground_normal.z)
                / ground.ground_normal.y;
            let snap = if ground.ground_distance > 0.05 {
                ((ground.ground_distance - 0.05) * 10.0).min(4.0)
            } else {
                0.0
            };
            velocity.y = slope_tangent_y - snap;
        } else {
            velocity.y = 0.0;
        }
    } else {
        if velocity.y < 0.0 {
            gravity_scale.0 = character.fall_gravity;
        } else if !jumping {
            gravity_scale.0 = character.fall_gravity;
        } else {
            gravity_scale.0 = character.base_gravity;
        }
    }

    ground.jump_held_last_frame = jumping;
}

fn character_rotate_body(
    time: Res<Time>,
    move_action: Single<&Action<Move>>,
    align_action: Single<&Action<AlignToCamera>>,
    cam_rig: Option<Single<&CameraRig>>,
    cam_target: Single<&CameraTarget, With<Character>>,
    character: Single<&Character>,
    mut body_transform: Single<&mut Transform, (With<CharacterBody>, Without<Character>)>,
) {
    let move_vec = ***move_action;
    let alt_held = ***align_action;
    let cam_yaw = cam_rig.as_ref().map_or(0.0, |rig| rig.yaw);
    let yaw_rot = Quat::from_rotation_y(cam_yaw);

    if cam_target.first_person {
        body_transform.rotation = yaw_rot;
        return;
    }

    if alt_held {
        let turn_alpha = (character.turn_speed * time.delta_secs()).min(1.0);
        body_transform.rotation = body_transform.rotation.slerp(yaw_rot, turn_alpha);
        return;
    }

    let direction = camera_relative_direction(move_vec, cam_yaw);
    if direction != Vec3::ZERO {
        let move_dir = direction.normalize();
        let target_yaw = (-move_dir.x).atan2(-move_dir.z);
        let target_rot = Quat::from_rotation_y(target_yaw);
        let turn_alpha = (character.turn_speed * time.delta_secs()).min(1.0);
        body_transform.rotation = body_transform.rotation.slerp(target_rot, turn_alpha);
    }
}

fn sync_body_visibility(
    target: Single<&CameraTarget, With<Character>>,
    mut body_visibility: Single<&mut Visibility, With<CharacterBody>>,
) {
    let desired = if target.first_person {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    if **body_visibility != desired {
        **body_visibility = desired;
    }
}

fn respawn_character(
    terrain: Option<Res<TerrainMap>>,
    query: Single<(
        &mut Transform,
        &mut LinearVelocity,
        &mut GroundState,
        &mut GravityScale,
        &Character,
    )>,
) {
    let (mut transform, mut velocity, mut ground, mut gravity, character) = query.into_inner();
    let spawn_y = terrain.as_ref().map_or(0.0, |t| t.height_at(0.0, 0.0)) + CHARACTER_SPAWN_OFFSET_Y;
    transform.translation = Vec3::new(0.0, spawn_y, 0.0);
    velocity.0 = Vec3::ZERO;
    *ground = GroundState::default();
    gravity.0 = character.base_gravity;
}

fn set_character_speed(
    args: Res<CommandArgs>,
    mut character_query: Query<&mut Character>,
) {
    let Ok(mut character) = character_query.single_mut() else {
        return;
    };

    if args.args.is_empty() {
        info!(
            "Current speed: walk={:.1}, sprint={:.1}. Usage: speed <walk> [sprint]",
            character.walk_speed, character.sprint_speed
        );
        return;
    }

    if let Some(walk) = args.get_f32(0) {
        character.walk_speed = walk;
        if let Some(sprint) = args.get_f32(1) {
            character.sprint_speed = sprint;
        } else {
            character.sprint_speed = walk * 2.0;
        }
        info!(
            "Set character speed -> walk: {:.1}, sprint: {:.1}",
            character.walk_speed, character.sprint_speed
        );
    } else {
        warn!("Invalid speed parameter: {:?}", args.args);
    }
}

fn increase_character_speed(
    args: Res<CommandArgs>,
    mut character_query: Query<&mut Character>,
) {
    let Ok(mut character) = character_query.single_mut() else {
        return;
    };

    let walk_add = args.get_f32(0).unwrap_or(2.0);
    let sprint_add = args.get_f32(1).unwrap_or(walk_add * 2.0);

    character.walk_speed += walk_add;
    character.sprint_speed += sprint_add;

    info!(
        "Increased character speed (+{:.1}, +{:.1}) -> walk: {:.1}, sprint: {:.1}",
        walk_add, sprint_add, character.walk_speed, character.sprint_speed
    );
}