use avian3d::prelude::*;
use bevy::{prelude::*, transform::TransformSystems};
use bevy_enhanced_input::prelude::*;

use crate::{
    camera::{CameraRig, CameraSystems, CameraTarget},
    input::{ContextActive, GameplayInput, HotkeyLabel},
    terrain::TerrainMap,
    utils::commands::{CommandAppExt, command},
};

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

#[derive(Component, Clone)]
pub struct Character {
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub jump_speed: f32,
    pub turn_speed: f32,
    pub acceleration: f32,
    pub air_acceleration: f32,
    pub jump_cooldown: f32,
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
            jump_cooldown: 0.0,
        }
    }
}

#[derive(Component, Default)]
pub struct GroundState {
    pub is_grounded: bool,
    pub ground_normal: Vec3,
    pub hit_distance: f32,
    pub jump_cooldown_timer: f32,
    pub jump_held_last_frame: bool, 
}

#[derive(Component)]
struct CharacterBody;

fn setup_character(world: &mut World) {
    let capsule_mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Capsule3d::new(0.4, 1.0));
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
        + 1.5;

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
                    ~SwizzleAxis::YXZ
                    --
                    Binding(KeyCode::KeyS)
                    HotkeyLabel("Move Back")
                    ~Negate::all()
                    ~SwizzleAxis::YXZ
                    --
                    Binding(KeyCode::KeyD)
                    HotkeyLabel("Move Right")
                    --
                    Binding(KeyCode::KeyA)
                    HotkeyLabel("Move Left")
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
            Collider::capsule(0.4, 1.0),
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
    mut query: Query<(&ShapeHits, &mut GroundState), With<Character>>,
) {
    let dt = time.delta_secs();
    for (hits, mut ground_state) in &mut query {
        ground_state.jump_cooldown_timer = (ground_state.jump_cooldown_timer - dt).max(0.0);

        // While jumping upward during cooldown, stay ungrounded
        if ground_state.jump_cooldown_timer > 0.0 {
            ground_state.is_grounded = false;
            ground_state.ground_normal = Vec3::Y;
            ground_state.hit_distance = f32::MAX;
            continue;
        }

        if let Some(hit) = hits.iter_sorted().next() {
            // Only consider it grounded if hit is close enough and slope isn't a wall
            if hit.distance <= 0.45 && hit.normal1.angle_between(Vec3::Y) <= 50.0_f32.to_radians() {
                ground_state.is_grounded = true;
                ground_state.ground_normal = hit.normal1.normalize();
                ground_state.hit_distance = hit.distance;
                continue;
            }
        }

        ground_state.is_grounded = false;
        ground_state.ground_normal = Vec3::Y;
        ground_state.hit_distance = f32::MAX;
    }
}

fn character_physics_move(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    move_action: Single<&Action<Move>>,
    sprint_action: Single<&Action<Sprint>>,
    jump_action: Single<&Action<Jump>>,
    cam_rig: Option<Single<&CameraRig>>,
    character_query: Single<(
        &Character,
        &mut GroundState,
        &mut LinearVelocity,
        &mut GravityScale, // <--- Add GravityScale here
    )>,
) {
    let (character, mut ground, mut velocity, mut gravity_scale) = character_query.into_inner();
    let dt = time.delta_secs();

    let move_vec = ***move_action;
    let sprinting = ***sprint_action;
    let jumping = ***jump_action;
    let cam_yaw = cam_rig.as_ref().map_or(0.0, |rig| rig.yaw);

    let yaw_rot = Quat::from_rotation_y(cam_yaw);
    let forward = yaw_rot * Vec3::NEG_Z;
    let right = yaw_rot * Vec3::X;
    let move_dir = (right * move_vec.x + forward * move_vec.y).normalize_or_zero();

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

    let ctrl_held = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let jump_just_pressed = jumping && !ground.jump_held_last_frame;

    if jump_just_pressed && ground.is_grounded && !ctrl_held && ground.jump_cooldown_timer <= 0.0 {
        // Immediate launch only on a fresh press
        velocity.y = character.jump_speed;
        ground.is_grounded = false;
        ground.jump_cooldown_timer = 0.2;
        gravity_scale.0 = 2.5;
    } else if ground.is_grounded {
        gravity_scale.0 = 2.5;

        // Anti-bounce: prevent upward rebound when grounded
        if velocity.y > 0.0 {
            velocity.y = 0.0;
        }

        // Stick to downslopes
        if move_dir != Vec3::ZERO && ground.ground_normal.y > 0.01 {
            let slope_tangent_y = -(velocity.x * ground.ground_normal.x + velocity.z * ground.ground_normal.z)
                / ground.ground_normal.y;
            if slope_tangent_y < 0.0 {
                velocity.y = slope_tangent_y;
            }
        }
    } else {
        // Airborne: asymmetric gravity
        if velocity.y < 0.0 {
            // Fall faster (3.5x gravity)
            gravity_scale.0 = 3.5;
        } else if !jumping {
            // Cut jump short if player releases Space early
            gravity_scale.0 = 3.5;
        } else {
            // Rising with Space held
            gravity_scale.0 = 2.5;
        }
    }

    // Remember whether Space was held this frame
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

    let forward = yaw_rot * Vec3::NEG_Z;
    let right = yaw_rot * Vec3::X;
    let direction = right * move_vec.x + forward * move_vec.y;
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
    mut query: Single<(&mut Transform, &mut LinearVelocity), With<Character>>,
) {
    let spawn_y = terrain.as_ref().map_or(0.0, |t| t.height_at(0.0, 0.0)) + 2.0;
    query.0.translation = Vec3::new(0.0, spawn_y, 0.0);
    query.1.0 = Vec3::ZERO;
}