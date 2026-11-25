use std::fmt;

use avian3d::prelude::*;
use bevy::{prelude::*, transform::TransformSystems};
use bevy_enhanced_input::prelude::*;

use crate::{
    input::{GameplayContextBundle, GameplayInput, HotkeyLabel, MovementBinding},
    utils::{
        SpawnGroundHeight,
        registry::{
            commands::{CommandAppExt, CommandArgs, CommandOutput, command},
            debug_info::{DebugInfoAppExt, debug_info},
        },
    },
};

pub const CHARACTER_SPAWN_OFFSET_Y: f32 = 1.5;
pub const CAPSULE_RADIUS: f32 = 0.4;
pub const CAPSULE_HEIGHT: f32 = 1.0;
pub const FEET_OFFSET: f32 = CAPSULE_HEIGHT * 0.5 + CAPSULE_RADIUS;
pub const CAPSULE_TOTAL_HEIGHT: f32 = CAPSULE_HEIGHT + CAPSULE_RADIUS * 2.0;

const GROUND_PROBE_RADIUS: f32 = 0.35;
const GROUND_PROBE_OFFSET_Y: f32 = -(CAPSULE_HEIGHT * 0.5);
const GROUND_PROBE_MAX_DISTANCE: f32 = 0.5;
const GROUND_LAND_TOLERANCE: f32 = 0.04;
const GROUND_SNAP_DISTANCE: f32 = 0.35;
const SLOPE_SNAP_MIN_DISTANCE: f32 = 0.005;
const SLOPE_SNAP_SPEED: f32 = 10.0;
const SLOPE_SNAP_MAX_SPEED: f32 = 4.0;
const MIN_GROUND_NORMAL_Y: f32 = 0.01;
const HORIZ_VEL_EPSILON: f32 = 0.001;
const DEFAULT_SPEED_DELTA: f32 = 2.0;

const SWIM_ENTER_DEPTH: f32 = 1.15;
const SWIM_EXIT_DEPTH: f32 = 1.05;
const FLOAT_TARGET_DEPTH: f32 = 1.2;
const BREACH_MAX_DEPTH: f32 = 1.5;
const WADING_SLOWDOWN: f32 = 0.45;
const SWIM_SPRINT_SCALE: f32 = 1.6;
const DIVE_TILT: f32 = 0.5;
const SWIM_TILT: f32 = 0.25;
const BOB_TILT: f32 = 0.07;
const BOB_ROLL: f32 = 0.05;
const BOB_HEIGHT: f32 = 0.05;
const BOB_FREQ: f32 = 2.0;
const BOB_FOLLOW: f32 = 12.0;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CharacterSystems {
    Playing,
}

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_character)
            .add_systems(
                FixedUpdate,
                (character_ground_check, character_physics_move)
                    .chain()
                    .in_set(CharacterSystems::Playing),
            )
            .add_systems(Update, character_rotate_body.in_set(CharacterSystems::Playing))
            .add_systems(
                PostUpdate,
                sync_body_visibility.before(TransformSystems::Propagate),
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

        debug_info!(app, "Movement State", debug_movement_state);
        debug_info!(app, "Movement Speed", debug_movement_speed);
        debug_info!(app, "Water Depth", debug_water_depth);
        debug_info!(app, "Position", debug_position);
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
    pub water: WaterPhysics,
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
            water: WaterPhysics::default(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WaterPhysics {
    pub swim_speed: f32,
    pub swim_accel: f32,
    pub drag: f32,
    pub buoy_stiffness: f32,
    pub buoy_damping: f32,
    pub buoy_max_accel: f32,
    pub sink_accel: f32,
    pub dive_accel: f32,
    pub current: Vec3,
}

impl Default for WaterPhysics {
    fn default() -> Self {
        Self {
            swim_speed: 4.0,
            swim_accel: 10.0,
            drag: 0.15,
            buoy_stiffness: 24.0,
            buoy_damping: 6.0,
            buoy_max_accel: 24.0,
            sink_accel: 0.8,
            dive_accel: 3.0,
            current: Vec3::ZERO,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct SubmersionState {
    pub depth: f32,
    pub water_height: f32,
}

impl Default for SubmersionState {
    fn default() -> Self {
        Self {
            depth: 0.0,
            water_height: f32::NEG_INFINITY,
        }
    }
}

impl SubmersionState {
    pub fn measure(water_height: Option<f32>, center_y: f32) -> Self {
        let Some(surface) = water_height else {
            return Self::default();
        };
        Self {
            depth: surface - (center_y - FEET_OFFSET),
            water_height: surface,
        }
    }

    pub fn is_submerged(&self) -> bool {
        self.depth > 0.0
    }

    pub fn is_fully_submerged(&self) -> bool {
        self.depth >= CAPSULE_TOTAL_HEIGHT
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

#[derive(Component, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementState {
    #[default]
    Standing,
    Walking,
    Running,
    Falling,
    Jumping,
    Wading,
    Floating,
    Swimming,
    Diving,
}

impl MovementState {
    pub fn is_swimming(self) -> bool {
        matches!(self, Self::Floating | Self::Swimming | Self::Diving)
    }
}

impl fmt::Display for MovementState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            MovementState::Standing => "standing",
            MovementState::Walking => "walking",
            MovementState::Running => "running",
            MovementState::Falling => "falling",
            MovementState::Jumping => "jumping",
            MovementState::Wading => "wading",
            MovementState::Floating => "floating",
            MovementState::Swimming => "swimming",
            MovementState::Diving => "diving",
        };
        write!(f, "{name}")
    }
}

const MOVEMENT_SPEED_EPSILON: f32 = 0.05;

fn horizontal_speed(velocity: Vec3) -> f32 {
    Vec2::new(velocity.x, velocity.z).length()
}

fn classify_movement_state(
    is_grounded: bool,
    horiz_speed: f32,
    velocity_y: f32,
    sprinting: bool,
) -> MovementState {
    if !is_grounded {
        if velocity_y > 0.0 {
            MovementState::Jumping
        } else {
            MovementState::Falling
        }
    } else if horiz_speed <= MOVEMENT_SPEED_EPSILON {
        MovementState::Standing
    } else if sprinting {
        MovementState::Running
    } else {
        MovementState::Walking
    }
}

fn debug_movement_state(states: Query<&MovementState, With<Character>>) -> Option<String> {
    states.iter().next().map(|state| state.to_string())
}

fn debug_movement_speed(velocities: Query<&LinearVelocity, With<Character>>) -> Option<String> {
    velocities
        .iter()
        .next()
        .map(|velocity| format!("{:.2} m/s", horizontal_speed(velocity.0)))
}

fn debug_water_depth(submersion: Single<&SubmersionState, With<Character>>) -> Option<String> {
    Some(if submersion.is_submerged() {
        format!("{:.2} m", submersion.depth)
    } else {
        "dry".to_string()
    })
}

fn debug_position(transforms: Query<&Transform, With<Character>>) -> Option<String> {
    transforms.iter().next().map(|transform| {
        let p = transform.translation;
        format!("({:.1}, {:.1}, {:.1})", p.x, p.y, p.z)
    })
}

#[derive(Component)]
pub(crate) struct CharacterBody;

#[derive(Component, Default)]
pub(crate) struct CameraFacing {
    pub yaw: f32,
    pub pitch: f32,
    pub first_person: bool,
}

pub fn camera_relative_direction(move_vec: Vec2, cam_yaw: f32) -> Vec3 {
    let yaw_rot = Quat::from_rotation_y(cam_yaw);
    let forward = yaw_rot * Vec3::NEG_Z;
    let right = yaw_rot * Vec3::X;
    right * move_vec.x + forward * move_vec.y
}

pub fn camera_relative_direction_3d(move_vec: Vec2, cam_yaw: f32, cam_pitch: f32) -> Vec3 {
    let yaw_rot = Quat::from_rotation_y(cam_yaw);
    let forward = yaw_rot * Quat::from_rotation_x(cam_pitch) * Vec3::NEG_Z;
    let right = yaw_rot * Vec3::X;
    right * move_vec.x + forward * move_vec.y
}

#[derive(Clone, Copy)]
struct MovementInput {
    move_vec: Vec2,
    sprinting: bool,
    jumping: bool,
    crouching: bool,
    yaw: f32,
    pitch: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Medium {
    Land,
    Wading,
    Water,
}

fn ground_touch_distance(normal_y: f32) -> f32 {
    (CAPSULE_RADIUS - GROUND_PROBE_RADIUS) / normal_y.max(MIN_GROUND_NORMAL_Y)
}

fn ground_gap(hit_distance: f32, normal_y: f32) -> f32 {
    (hit_distance - ground_touch_distance(normal_y)).max(0.0)
}

fn is_walkable(normal: Vec3, max_slope_angle_rad: f32) -> bool {
    normal.angle_between(Vec3::Y) <= max_slope_angle_rad
}

fn accelerate_toward(current: Vec3, target: Vec3, accel: f32, dt: f32) -> Vec3 {
    let diff = target - current;
    let max_change = accel * dt;
    if diff.length_squared() <= max_change * max_change {
        target
    } else {
        current + diff.normalize() * max_change
    }
}

fn jump_fires(jumping: bool, held_last_frame: bool, grounded: bool, cooldown_left: f32) -> bool {
    jumping && !held_last_frame && grounded && cooldown_left <= 0.0
}

fn air_gravity_scale(velocity_y: f32, jumping: bool, character: &Character) -> f32 {
    if velocity_y < 0.0 || !jumping {
        character.fall_gravity
    } else {
        character.base_gravity
    }
}

fn slope_tangent_y(vx: f32, vz: f32, normal: Vec3) -> f32 {
    -(vx * normal.x + vz * normal.z) / normal.y
}

fn slope_snap_speed(gap: f32) -> f32 {
    if gap > SLOPE_SNAP_MIN_DISTANCE {
        ((gap - SLOPE_SNAP_MIN_DISTANCE) * SLOPE_SNAP_SPEED).min(SLOPE_SNAP_MAX_SPEED)
    } else {
        0.0
    }
}

fn spawn_translation(ground_height: f32) -> Vec3 {
    Vec3::new(0.0, ground_height + CHARACTER_SPAWN_OFFSET_Y, 0.0)
}

fn swim_latch(depth: f32, was_swimming: bool) -> bool {
    if was_swimming {
        depth > SWIM_EXIT_DEPTH
    } else {
        depth >= SWIM_ENTER_DEPTH
    }
}

fn classify_medium(depth: f32, was_swimming: bool) -> Medium {
    if swim_latch(depth, was_swimming) {
        Medium::Water
    } else if depth > 0.0 {
        Medium::Wading
    } else {
        Medium::Land
    }
}

fn wading_scale(depth: f32) -> f32 {
    let sunk = (depth / SWIM_ENTER_DEPTH).clamp(0.0, 1.0);
    1.0 - WADING_SLOWDOWN * sunk
}

fn breach_fires(
    jumping: bool,
    held_last_frame: bool,
    grounded: bool,
    cooldown_left: f32,
    depth: f32,
) -> bool {
    jumping && !held_last_frame && grounded && cooldown_left <= 0.0 && depth <= BREACH_MAX_DEPTH
}

fn buoyancy_accel(water: &WaterPhysics, depth: f32, vertical_velocity: f32) -> f32 {
    let lift = water.buoy_stiffness * (depth - FLOAT_TARGET_DEPTH).max(0.0);
    (lift - water.buoy_damping * vertical_velocity).clamp(-water.buoy_max_accel, water.buoy_max_accel)
}

fn drag_toward_zero(value: f32, drag: f32, dt: f32) -> f32 {
    let loss = drag * value * value * dt;
    if loss >= value.abs() {
        0.0
    } else {
        value - value.signum() * loss
    }
}

fn drag_toward(value: f32, target: f32, drag: f32, dt: f32) -> f32 {
    let relative = value - target;
    let loss = drag * relative * relative * dt;
    if loss >= relative.abs() {
        target
    } else {
        value - relative.signum() * loss
    }
}

fn sprint_scale(sprinting: bool) -> f32 {
    if sprinting {
        SWIM_SPRINT_SCALE
    } else {
        1.0
    }
}

fn classify_wading_state(state: MovementState) -> MovementState {
    match state {
        MovementState::Standing | MovementState::Walking | MovementState::Running => {
            MovementState::Wading
        }
        other => other,
    }
}

fn classify_water_state(diving: bool, ascending: bool) -> MovementState {
    if diving {
        MovementState::Diving
    } else if ascending {
        MovementState::Floating
    } else {
        MovementState::Swimming
    }
}

fn body_tilt(state: MovementState, horiz_speed: f32, swim_speed: f32, time: f32) -> (f32, f32) {
    let effort = (horiz_speed / swim_speed.max(HORIZ_VEL_EPSILON)).clamp(0.0, 1.0);
    match state {
        MovementState::Diving => (-DIVE_TILT * (0.4 + 0.6 * effort), 0.0),
        MovementState::Swimming => (-SWIM_TILT * effort, 0.0),
        MovementState::Floating => (
            (time * BOB_FREQ).sin() * BOB_TILT,
            (time * BOB_FREQ * 0.5).sin() * BOB_ROLL,
        ),
        _ => (0.0, 0.0),
    }
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
    let spawn_translation = spawn_translation(
        world
            .get_resource::<SpawnGroundHeight>()
            .map_or(0.0, |height| height.0),
    );

    let mut char_entity = world
        .spawn_scene(bsn! {
            Character
            Transform::from_translation(spawn_translation)
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
            CameraFacing::default(),
            GameplayContextBundle::default(),
            RigidBody::Dynamic,
            Collider::capsule(CAPSULE_RADIUS, CAPSULE_HEIGHT),
            LockedAxes::ROTATION_LOCKED,
            Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
            Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
            LinearVelocity::default(),
            GravityScale(Character::default().base_gravity),
            GroundState::default(),
            MovementState::default(),
            SubmersionState::default(),
            TransformInterpolation,
            ShapeCaster::new(
                Collider::sphere(GROUND_PROBE_RADIUS),
                Vec3::new(0.0, GROUND_PROBE_OFFSET_Y, 0.0),
                Quat::IDENTITY,
                Dir3::NEG_Y,
            )
            .with_max_distance(GROUND_PROBE_MAX_DISTANCE)
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
    mut query: Query<(&Character, &ShapeHits, &LinearVelocity, &mut GroundState)>,
) {
    let dt = time.delta_secs();
    for (character, hits, velocity, mut ground_state) in &mut query {
        ground_state.jump_cooldown_timer = (ground_state.jump_cooldown_timer - dt).max(0.0);

        let can_probe = ground_state.jump_cooldown_timer <= 0.0
            && (ground_state.is_grounded || velocity.y <= 0.0);
        let max_gap = if ground_state.is_grounded {
            GROUND_SNAP_DISTANCE
        } else {
            GROUND_LAND_TOLERANCE
        };

        let hit = can_probe
            .then(|| {
                hits.iter_sorted().find_map(|hit| {
                    let normal = hit.normal1.normalize();
                    if !is_walkable(normal, character.max_slope_angle_rad) {
                        return None;
                    }
                    let gap = ground_gap(hit.distance, normal.y);
                    (gap <= max_gap).then_some((normal, gap))
                })
            })
            .flatten();

        if let Some((normal, gap)) = hit {
            ground_state.is_grounded = true;
            ground_state.ground_normal = normal;
            ground_state.ground_distance = gap;
        } else {
            ground_state.is_grounded = false;
            ground_state.ground_normal = Vec3::Y;
            ground_state.ground_distance = 0.0;
        }
    }
}

fn character_physics_move(
    time: Res<Time>,
    move_action: Single<&Action<Move>>,
    sprint_action: Single<&Action<Sprint>>,
    jump_action: Single<&Action<Jump>>,
    crouch_action: Single<&Action<Crouch>>,
    facing: Single<&CameraFacing>,
    character_query: Single<(
        &Character,
        &mut GroundState,
        &SubmersionState,
        &mut LinearVelocity,
        &mut GravityScale,
        &mut MovementState,
    )>,
) {
    let (character, mut ground, submersion, mut velocity, mut gravity_scale, mut movement_state) =
        character_query.into_inner();
    let dt = time.delta_secs();

    let input = MovementInput {
        move_vec: ***move_action,
        sprinting: ***sprint_action,
        jumping: ***jump_action,
        crouching: ***crouch_action,
        yaw: facing.yaw,
        pitch: facing.pitch,
    };

    let next_state = match classify_medium(submersion.depth, movement_state.is_swimming()) {
        Medium::Water => move_in_water(
            character,
            &mut ground,
            submersion,
            &input,
            &mut velocity,
            &mut gravity_scale,
            dt,
        ),
        Medium::Wading => classify_wading_state(move_on_ground(
            character,
            &mut ground,
            &input,
            &mut velocity,
            &mut gravity_scale,
            dt,
            wading_scale(submersion.depth),
        )),
        Medium::Land => move_on_ground(
            character,
            &mut ground,
            &input,
            &mut velocity,
            &mut gravity_scale,
            dt,
            1.0,
        ),
    };

    ground.jump_held_last_frame = input.jumping;
    *movement_state = next_state;
}

fn move_on_ground(
    character: &Character,
    ground: &mut GroundState,
    input: &MovementInput,
    velocity: &mut LinearVelocity,
    gravity_scale: &mut GravityScale,
    dt: f32,
    speed_scale: f32,
) -> MovementState {
    let move_dir = camera_relative_direction(input.move_vec, input.yaw).normalize_or_zero();
    let base_speed = if input.sprinting {
        character.sprint_speed
    } else {
        character.walk_speed
    };
    let target_horiz = move_dir * base_speed * speed_scale;

    let accel = if ground.is_grounded {
        character.acceleration
    } else {
        character.air_acceleration
    };

    let current_horiz = Vec3::new(velocity.x, 0.0, velocity.z);
    let new_horiz = accelerate_toward(current_horiz, target_horiz, accel, dt);
    velocity.x = new_horiz.x;
    velocity.z = new_horiz.z;

    if jump_fires(
        input.jumping,
        ground.jump_held_last_frame,
        ground.is_grounded,
        ground.jump_cooldown_timer,
    ) {
        velocity.y = character.jump_speed * speed_scale;
        ground.is_grounded = false;
        ground.jump_cooldown_timer = character.jump_cooldown;
        gravity_scale.0 = character.base_gravity;
    } else if ground.is_grounded {
        gravity_scale.0 = 0.0;

        let has_horiz_vel =
            velocity.x.abs() > HORIZ_VEL_EPSILON || velocity.z.abs() > HORIZ_VEL_EPSILON;
        if has_horiz_vel && ground.ground_normal.y > MIN_GROUND_NORMAL_Y {
            velocity.y = slope_tangent_y(velocity.x, velocity.z, ground.ground_normal)
                - slope_snap_speed(ground.ground_distance);
        } else {
            velocity.y = 0.0;
        }
    } else {
        gravity_scale.0 = air_gravity_scale(velocity.y, input.jumping, character);
    }

    classify_movement_state(
        ground.is_grounded,
        horizontal_speed(velocity.0),
        velocity.y,
        input.sprinting,
    )
}

fn move_in_water(
    character: &Character,
    ground: &mut GroundState,
    submersion: &SubmersionState,
    input: &MovementInput,
    velocity: &mut LinearVelocity,
    gravity_scale: &mut GravityScale,
    dt: f32,
) -> MovementState {
    let water = &character.water;
    gravity_scale.0 = 0.0;

    if breach_fires(
        input.jumping,
        ground.jump_held_last_frame,
        ground.is_grounded,
        ground.jump_cooldown_timer,
        submersion.depth,
    ) {
        velocity.y = character.jump_speed;
        ground.is_grounded = false;
        ground.jump_cooldown_timer = character.jump_cooldown;
        return MovementState::Jumping;
    }

    let diving = input.crouching || submersion.is_fully_submerged();
    let steer = if diving {
        camera_relative_direction_3d(input.move_vec, input.yaw, input.pitch)
    } else {
        camera_relative_direction(input.move_vec, input.yaw)
    }
    .normalize_or_zero();

    let speed = water.swim_speed * sprint_scale(input.sprinting);
    let moved = accelerate_toward(
        Vec3::new(velocity.x, 0.0, velocity.z),
        Vec3::new(steer.x, 0.0, steer.z) * speed,
        water.swim_accel,
        dt,
    );
    velocity.x = moved.x;
    velocity.z = moved.z;

    if diving {
        velocity.y += steer.y * water.swim_accel * dt;
    }

    if input.jumping {
        velocity.y += buoyancy_accel(water, submersion.depth, velocity.y) * dt;
    } else if input.crouching {
        velocity.y -= water.dive_accel * sprint_scale(input.sprinting) * dt;
    } else {
        velocity.y -= water.sink_accel * dt;
    }

    velocity.0 = Vec3::new(
        drag_toward(velocity.x, water.current.x, water.drag, dt),
        drag_toward_zero(velocity.y, water.drag, dt),
        drag_toward(velocity.z, water.current.z, water.drag, dt),
    );

    if ground.is_grounded && !input.jumping && velocity.y < 0.0 {
        velocity.y = 0.0;
    }

    classify_water_state(diving, input.jumping)
}

fn character_rotate_body(
    time: Res<Time>,
    move_action: Single<&Action<Move>>,
    align_action: Single<&Action<AlignToCamera>>,
    facing: Single<&CameraFacing>,
    character: Single<&Character>,
    velocity: Single<&LinearVelocity, With<Character>>,
    movement_state: Single<&MovementState, With<Character>>,
    mut body_transform: Single<&mut Transform, (With<CharacterBody>, Without<Character>)>,
) {
    let dt = time.delta_secs();
    let turn_alpha = (character.turn_speed * dt).min(1.0);
    let cam_yaw = facing.yaw;
    let yaw_rot = Quat::from_rotation_y(cam_yaw);

    if facing.first_person {
        body_transform.rotation = yaw_rot;
        body_transform.translation = Vec3::ZERO;
        return;
    }

    let current_yaw = Quat::from_rotation_y(body_transform.rotation.to_euler(EulerRot::YXZ).0);
    let direction = camera_relative_direction(***move_action, cam_yaw);

    let heading = if ***align_action {
        yaw_rot
    } else if direction != Vec3::ZERO {
        let move_dir = direction.normalize();
        Quat::from_rotation_y((-move_dir.x).atan2(-move_dir.z))
    } else {
        current_yaw
    };

    let (pitch_tilt, roll_tilt) = body_tilt(
        **movement_state,
        horizontal_speed(velocity.0),
        character.water.swim_speed,
        time.elapsed_secs(),
    );
    let target = current_yaw
        .slerp(heading, turn_alpha)
        * Quat::from_rotation_x(pitch_tilt)
        * Quat::from_rotation_z(roll_tilt);
    body_transform.rotation = body_transform.rotation.slerp(target, turn_alpha);

    let bob = if movement_state.is_swimming() {
        (time.elapsed_secs() * BOB_FREQ).sin() * BOB_HEIGHT
    } else {
        0.0
    };
    body_transform.translation.y = body_transform.translation.y.lerp(bob, (BOB_FOLLOW * dt).min(1.0));
}

fn sync_body_visibility(
    facing: Single<&CameraFacing>,
    mut body_visibility: Single<&mut Visibility, With<CharacterBody>>,
) {
    let desired = if facing.first_person {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    if **body_visibility != desired {
        **body_visibility = desired;
    }
}

fn respawn_character(
    spawn_ground: Res<SpawnGroundHeight>,
    query: Single<(
        &mut Transform,
        &mut LinearVelocity,
        &mut GroundState,
        &mut GravityScale,
        &mut MovementState,
        &Character,
    )>,
) {
    let (mut transform, mut velocity, mut ground, mut gravity, mut movement_state, character) =
        query.into_inner();
    transform.translation = spawn_translation(spawn_ground.0);
    velocity.0 = Vec3::ZERO;
    *ground = GroundState::default();
    *movement_state = MovementState::default();
    gravity.0 = character.base_gravity;
}

fn with_player_character(query: &mut Query<&mut Character>, edit: impl FnOnce(&mut Character)) -> bool {
    let Ok(mut character) = query.single_mut() else {
        warn!("No character found for speed command");
        return false;
    };
    edit(&mut character);
    true
}

fn set_character_speed(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut character_query: Query<&mut Character>,
) {
    if !with_player_character(&mut character_query, |character| {
        if args.args.is_empty() {
            output.set(format!(
                "Current speed: walk={:.1}, sprint={:.1}. Usage: speed <walk> [sprint]",
                character.walk_speed, character.sprint_speed
            ));
            return;
        }

        let Some(walk) = args.get_f32(0) else {
            warn!("Invalid speed parameter: {:?}", args.args);
            output.set(format!("Invalid speed parameter: {:?}", args.args));
            return;
        };
        character.walk_speed = walk;
        character.sprint_speed = args.get_f32(1).unwrap_or(walk * 2.0);
        output.set(format!(
            "Set character speed -> walk: {:.1}, sprint: {:.1}",
            character.walk_speed, character.sprint_speed
        ));
    }) {
        output.set("error: no character found");
    }
}

fn increase_character_speed(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut character_query: Query<&mut Character>,
) {
    if !with_player_character(&mut character_query, |character| {
        let walk_add = args.get_f32(0).unwrap_or(DEFAULT_SPEED_DELTA);
        let sprint_add = args.get_f32(1).unwrap_or(walk_add * 2.0);

        character.walk_speed += walk_add;
        character.sprint_speed += sprint_add;

        output.set(format!(
            "Increased character speed (+{:.1}, +{:.1}) -> walk: {:.1}, sprint: {:.1}",
            walk_add, sprint_add, character.walk_speed, character.sprint_speed
        ));
    }) {
        output.set("error: no character found");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOPE_45: Vec3 = Vec3::new(0.707_106_8, 0.707_106_8, 0.0);
    const SLOPE_60: Vec3 = Vec3::new(0.866_025_4, 0.5, 0.0);

    #[test]
    fn forward_follows_the_camera() {
        let dir = camera_relative_direction(Vec2::new(0.0, 1.0), 0.0);
        assert!(dir.z < -0.99, "expected -Z, got {dir}");
        assert!(dir.x.abs() < 1e-6);

        let turned = camera_relative_direction(Vec2::new(0.0, 1.0), 90f32.to_radians());
        assert!(turned.x < -0.99, "expected -X, got {turned}");
    }

    #[test]
    fn acceleration_is_capped_per_step() {
        let out = accelerate_toward(Vec3::ZERO, Vec3::X * 6.0, 60.0, 1.0 / 60.0);
        assert!(
            (out.x - 1.0).abs() < 1e-5,
            "should gain at most accel*dt this step, got {out}"
        );
    }

    #[test]
    fn acceleration_lands_exactly_on_target_when_close() {
        let out = accelerate_toward(Vec3::X * 5.5, Vec3::X * 6.0, 60.0, 1.0 / 60.0);
        assert_eq!(out, Vec3::X * 6.0);
    }

    #[test]
    fn acceleration_with_no_accel_is_a_no_op() {
        assert_eq!(accelerate_toward(Vec3::X * 3.0, Vec3::X * 6.0, 0.0, 1.0), Vec3::X * 3.0);
    }

    #[test]
    fn jump_requires_a_press_edge() {
        assert!(jump_fires(true, false, true, 0.0));
        assert!(!jump_fires(true, true, true, 0.0), "holding the key must not bunny-hop");
        assert!(!jump_fires(false, false, true, 0.0));
    }

    #[test]
    fn jump_requires_ground_and_a_spent_cooldown() {
        assert!(!jump_fires(true, false, false, 0.0), "airborne");
        assert!(!jump_fires(true, false, true, 0.1), "cooldown pending");
    }

    #[test]
    fn gravity_is_heavier_once_falling_or_released() {
        let c = Character::default();
        assert_eq!(air_gravity_scale(1.0, true, &c), c.base_gravity);
        assert_eq!(air_gravity_scale(1.0, false, &c), c.fall_gravity);
        assert_eq!(air_gravity_scale(-1.0, true, &c), c.fall_gravity);
    }

    #[test]
    fn ground_gap_matches_the_probe_offset_on_flat_ground() {
        let touch = ground_touch_distance(1.0);
        assert!((touch - (CAPSULE_RADIUS - GROUND_PROBE_RADIUS)).abs() < 1e-6);
        assert!(ground_gap(touch, 1.0).abs() < 1e-6, "touching means zero gap");
        assert!((ground_gap(touch + 0.2, 1.0) - 0.2).abs() < 1e-6);
        assert_eq!(ground_gap(0.0, 1.0), 0.0, "gap must never go negative");
    }

    #[test]
    fn walkability_respects_the_slope_limit() {
        let max = Character::default().max_slope_angle_rad;
        assert!(is_walkable(Vec3::Y, max), "flat ground");
        assert!(is_walkable(SLOPE_45, max), "45deg is under the 50deg limit");
        assert!(!is_walkable(SLOPE_60, max), "60deg is over the limit");
        assert!(!is_walkable(Vec3::X, max), "vertical wall");
    }

    #[test]
    fn slope_tangent_holds_velocity_on_the_surface() {
        assert!(slope_tangent_y(1.0, 0.0, Vec3::Y).abs() < 1e-9, "flat ground has no vertical term");
        let v = slope_tangent_y(1.0, 0.0, SLOPE_45);
        assert!((v + 1.0).abs() < 1e-4, "moving +X up a 45deg slope should give -1, got {v}");
    }

    #[test]
    fn horizontal_speed_ignores_vertical_motion() {
        assert!((horizontal_speed(Vec3::new(3.0, -40.0, 4.0)) - 5.0).abs() < 1e-6);
        assert_eq!(horizontal_speed(Vec3::new(0.0, 9.0, 0.0)), 0.0);
    }

    #[test]
    fn airborne_rises_fall_by_vertical_velocity() {
        assert_eq!(classify_movement_state(false, 0.0, 1.0, false), MovementState::Jumping);
        assert_eq!(classify_movement_state(false, 8.0, 0.1, true), MovementState::Jumping);
        assert_eq!(classify_movement_state(false, 8.0, -0.1, false), MovementState::Falling);
        assert_eq!(classify_movement_state(false, 0.0, 0.0, false), MovementState::Falling);
    }

    #[test]
    fn grounded_walk_down_a_slope_is_not_falling() {
        assert_eq!(classify_movement_state(true, 6.0, -2.0, false), MovementState::Walking);
    }

    #[test]
    fn grounded_speed_and_sprint_split_the_states() {
        assert_eq!(classify_movement_state(true, 0.0, 0.0, false), MovementState::Standing);
        assert_eq!(
            classify_movement_state(true, MOVEMENT_SPEED_EPSILON, 0.0, true),
            MovementState::Standing,
            "at the epsilon it still counts as standing"
        );
        assert_eq!(classify_movement_state(true, 6.0, 0.0, false), MovementState::Walking);
        assert_eq!(classify_movement_state(true, 12.0, 0.0, true), MovementState::Running);
    }

    #[test]
    fn movement_state_displays_lowercase() {
        assert_eq!(MovementState::Jumping.to_string(), "jumping");
        assert_eq!(MovementState::default(), MovementState::Standing);
    }

    #[test]
    fn slope_snap_ramps_and_caps() {
        assert_eq!(slope_snap_speed(SLOPE_SNAP_MIN_DISTANCE), 0.0);
        assert_eq!(slope_snap_speed(SLOPE_SNAP_MIN_DISTANCE / 2.0), 0.0);
        let mid = slope_snap_speed(0.1);
        assert!(mid > 0.0 && mid < SLOPE_SNAP_MAX_SPEED, "should ramp between the bounds");
        assert_eq!(slope_snap_speed(100.0), SLOPE_SNAP_MAX_SPEED, "must cap, or steps slam");
    }

    #[test]
    fn the_capsule_is_one_metre_eight_tall() {
        assert!((CAPSULE_TOTAL_HEIGHT - 1.8).abs() < 1e-6);
        assert!((FEET_OFFSET - 0.9).abs() < 1e-6);
    }

    #[test]
    fn swim_latch_needs_more_depth_to_leave_than_to_enter() {
        assert!(!swim_latch(SWIM_ENTER_DEPTH - 0.01, false));
        assert!(swim_latch(SWIM_ENTER_DEPTH, false));
        assert!(
            swim_latch(SWIM_EXIT_DEPTH + 0.01, true),
            "draining through the band keeps you swimming"
        );
        assert!(
            !swim_latch(SWIM_EXIT_DEPTH, true),
            "once shallow enough you stand again"
        );
    }

    #[test]
    fn medium_splits_land_wading_and_water() {
        assert_eq!(classify_medium(-0.5, false), Medium::Land);
        assert_eq!(classify_medium(0.0, false), Medium::Land);
        assert_eq!(classify_medium(0.5, false), Medium::Wading);
        assert_eq!(classify_medium(SWIM_ENTER_DEPTH, false), Medium::Water);
        assert_eq!(classify_medium(1.1, false), Medium::Wading);
        assert_eq!(classify_medium(1.1, true), Medium::Water);
    }

    #[test]
    fn wading_slows_the_deeper_you_are() {
        assert_eq!(wading_scale(0.0), 1.0, "dry shoes are unbothered");
        let knee = wading_scale(0.5);
        let chest = wading_scale(1.0);
        assert!(knee > chest, "deeper is slower");
        assert!(chest > 1.0 - WADING_SLOWDOWN);
        assert!(
            (wading_scale(99.0) - (1.0 - WADING_SLOWDOWN)).abs() < 1e-6,
            "the ramp bottoms out instead of going negative"
        );
    }

    #[test]
    fn buoyancy_lifts_only_from_below_the_float_line() {
        let water = WaterPhysics::default();
        assert_eq!(buoyancy_accel(&water, FLOAT_TARGET_DEPTH, 0.0), 0.0);
        assert!(
            buoyancy_accel(&water, FLOAT_TARGET_DEPTH - 0.3, 0.0) <= 0.0,
            "buoyancy never pulls you under"
        );
        assert!(buoyancy_accel(&water, 2.0, 0.0) > 0.0, "deep means lifted");
        assert_eq!(
            buoyancy_accel(&water, 100.0, 0.0),
            water.buoy_max_accel,
            "a hard clamp stops the spring launching you"
        );
        assert!(
            buoyancy_accel(&water, FLOAT_TARGET_DEPTH, 5.0) < 0.0,
            "damping opposes a rise"
        );
    }

    #[test]
    fn drag_bleeds_speed_without_crossing_zero() {
        assert_eq!(drag_toward_zero(0.0, 0.15, 0.016), 0.0);

        let plunge = drag_toward_zero(10.0, 0.15, 1.0 / 64.0);
        assert!(plunge > 0.0 && plunge < 9.9, "a fast entry bleeds hard");

        let glide = drag_toward_zero(0.2, 0.15, 1.0 / 64.0);
        assert!(glide > 0.0 && glide < 0.2, "a slow glide still loses something");
        assert!(
            plunge / 10.0 < glide / 0.2,
            "drag bites harder the faster you go"
        );

        assert!(drag_toward_zero(-10.0, 0.15, 1.0 / 64.0) > -10.0);
        assert_eq!(
            drag_toward_zero(0.01, 0.15, 1000.0),
            0.0,
            "a step bigger than the speed stops at zero, never past it"
        );
    }

    #[test]
    fn breach_needs_ground_shallow_water_and_a_fresh_press() {
        assert!(breach_fires(true, false, true, 0.0, 1.0));
        assert!(!breach_fires(true, true, true, 0.0, 1.0), "held is not a press");
        assert!(
            !breach_fires(true, false, false, 0.0, 1.0),
            "nothing to push off"
        );
        assert!(!breach_fires(true, false, true, 0.1, 1.0), "cooldown pending");
        assert!(
            !breach_fires(true, false, true, 0.0, BREACH_MAX_DEPTH + 0.1),
            "too deep to breach"
        );
    }

    #[test]
    fn water_states_split_on_input_and_depth() {
        assert_eq!(classify_water_state(true, true), MovementState::Diving);
        assert_eq!(classify_water_state(false, true), MovementState::Floating);
        assert_eq!(classify_water_state(false, false), MovementState::Swimming);
    }

    #[test]
    fn submersion_measures_from_the_capsule_feet() {
        let waist = SubmersionState::measure(Some(0.0), 0.0);
        assert!((waist.depth - FEET_OFFSET).abs() < 1e-6);
        assert_eq!(waist.water_height, 0.0);
        assert!(waist.is_submerged());
        assert!(!waist.is_fully_submerged());

        let sunk = SubmersionState::measure(Some(0.0), -CAPSULE_TOTAL_HEIGHT);
        assert!(sunk.is_fully_submerged());

        let dry = SubmersionState::measure(None, -20.0);
        assert!(!dry.is_submerged());
        assert_eq!(dry.water_height, f32::NEG_INFINITY);
    }

    #[test]
    fn diving_tilts_the_body_nose_down() {
        let (dive, _) = body_tilt(MovementState::Diving, 4.0, 4.0, 0.0);
        let (swim, _) = body_tilt(MovementState::Swimming, 4.0, 4.0, 0.0);
        let (idle_swim, _) = body_tilt(MovementState::Swimming, 0.0, 4.0, 0.0);
        let (stand, roll) = body_tilt(MovementState::Standing, 0.0, 4.0, 0.0);
        assert!(dive < 0.0, "diving pitches forward");
        assert!(swim < 0.0, "swimming pitches forward");
        assert!(dive < swim, "diving tilts harder than swimming");
        assert_eq!(idle_swim, 0.0, "treading does not pitch");
        assert_eq!((stand, roll), (0.0, 0.0), "standing stays flat");
    }

    #[test]
    fn floating_bobs_instead_of_holding_still() {
        let (low, _) = body_tilt(MovementState::Floating, 0.0, 4.0, 0.0);
        let (high, _) = body_tilt(MovementState::Floating, 0.0, 4.0, std::f32::consts::FRAC_PI_4 / BOB_FREQ);
        assert!(high > low, "the bob swings through its cycle");
    }

    #[test]
    fn pitch_steers_the_dive_vector() {
        let level = camera_relative_direction_3d(Vec2::new(0.0, 1.0), 0.0, 0.0);
        assert!(level.y.abs() < 1e-6, "level camera stays horizontal");

        let down = camera_relative_direction_3d(Vec2::new(0.0, 1.0), 0.0, -0.5);
        assert!(down.y < 0.0 && down.z < 0.0, "looking down drives you down");

        let up = camera_relative_direction_3d(Vec2::new(0.0, 1.0), 0.0, 0.5);
        assert!(up.y > 0.0, "looking up drives you up");

        let turned = camera_relative_direction_3d(Vec2::new(0.0, 1.0), 90f32.to_radians(), -0.5);
        assert!(turned.x < -0.5 && turned.y < 0.0, "yaw still steers");
    }

    #[test]
    fn water_movement_states_display_lowercase() {
        assert_eq!(MovementState::Wading.to_string(), "wading");
        assert_eq!(MovementState::Floating.to_string(), "floating");
        assert_eq!(MovementState::Swimming.to_string(), "swimming");
        assert_eq!(MovementState::Diving.to_string(), "diving");
    }

    #[test]
    fn only_the_fluid_states_count_as_swimming() {
        assert!(MovementState::Floating.is_swimming());
        assert!(MovementState::Swimming.is_swimming());
        assert!(MovementState::Diving.is_swimming());
        assert!(!MovementState::Wading.is_swimming(), "wading keeps its footing");
        assert!(!MovementState::Walking.is_swimming());
        assert!(!MovementState::Jumping.is_swimming());
    }

    #[test]
    fn wading_relabels_only_the_grounded_states() {
        assert_eq!(classify_wading_state(MovementState::Standing), MovementState::Wading);
        assert_eq!(classify_wading_state(MovementState::Walking), MovementState::Wading);
        assert_eq!(classify_wading_state(MovementState::Running), MovementState::Wading);
        assert_eq!(classify_wading_state(MovementState::Jumping), MovementState::Jumping);
        assert_eq!(classify_wading_state(MovementState::Falling), MovementState::Falling);
    }
}
