use avian3d::prelude::*;
use bevy::{prelude::*, transform::TransformSystems};
use bevy_enhanced_input::prelude::*;

use crate::{
    input::{ContextActive, GameplayInput, HotkeyLabel, MovementBinding},
    utils::registry::commands::{CommandAppExt, CommandArgs, CommandOutput, command},
    utils::{SpawnGroundHeight, WaterImmersion},
};

pub const CHARACTER_SPAWN_OFFSET_Y: f32 = 100.5;
pub const CAPSULE_RADIUS: f32 = 0.4;
pub const CAPSULE_HEIGHT: f32 = 1.0;

const GROUND_PROBE_RADIUS: f32 = 0.35;
const GROUND_PROBE_OFFSET_Y: f32 = -(CAPSULE_HEIGHT * 0.5);
const GROUND_PROBE_MAX_DISTANCE: f32 = 0.5;
const GROUND_LAND_TOLERANCE: f32 = 0.04;
const GROUND_SNAP_DISTANCE: f32 = 0.35;
const SLOPE_SNAP_MIN_DISTANCE: f32 = 0.005;
const SLOPE_SNAP_MAX_SPEED: f32 = 4.0;
const MIN_GROUND_NORMAL_Y: f32 = 0.01;
const HORIZ_VEL_EPSILON: f32 = 0.001;
const DEFAULT_SPEED_DELTA: f32 = 2.0;

const SWIM_BUOYANCY_SPRING: f32 = 6.0;
const SWIM_BUOYANCY_DAMP: f32 = 2.0;
const SWIM_DRAG: f32 = 2.5;
const SWIM_CURRENT: f32 = 0.8;
const SWIM_UP_SPEED: f32 = 3.0;
const SWIM_DIVE_SPEED: f32 = 2.5;
const SWIM_TARGET_SCALE: f32 = 0.6;

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
                character_physics_move.in_set(CharacterSystems::Playing),
            )
            .add_systems(
                FixedPostUpdate,
                character_ground_check
                    .in_set(CharacterSystems::Playing)
                    .after(SpatialQuerySystems),
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

#[derive(Component, Default)]
pub(crate) struct CameraFacing {
    pub yaw: f32,
    pub first_person: bool,
}

pub fn camera_relative_direction(move_vec: Vec2, cam_yaw: f32) -> Vec3 {
    let yaw_rot = Quat::from_rotation_y(cam_yaw);
    let forward = yaw_rot * Vec3::NEG_Z;
    let right = yaw_rot * Vec3::X;
    right * move_vec.x + forward * move_vec.y
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

fn is_swimming(immersion: &WaterImmersion) -> bool {
    immersion.submerged && !immersion.frozen && immersion.depth > CAPSULE_HEIGHT * 0.5
}

fn buoyancy_acceleration(surface_y: f32, y: f32, vy: f32) -> f32 {
    SWIM_BUOYANCY_SPRING * (surface_y - y) - SWIM_BUOYANCY_DAMP * vy
}

fn swim_vertical_velocity(
    surface_y: f32,
    y: f32,
    vy: f32,
    dt: f32,
    swim_up: bool,
    diving: bool,
) -> f32 {
    let mut next = vy + buoyancy_acceleration(surface_y, y, vy) * dt;
    if swim_up {
        next = next.max(SWIM_UP_SPEED);
    }
    if diving {
        next -= SWIM_DIVE_SPEED * dt;
    }
    next
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

fn slope_snap_cap(horiz_speed: f32) -> f32 {
    SLOPE_SNAP_MAX_SPEED.max(horiz_speed)
}

fn slope_snap_speed(gap: f32, horiz_speed: f32) -> f32 {
    if gap <= SLOPE_SNAP_MIN_DISTANCE {
        return 0.0;
    }
    let cap = slope_snap_cap(horiz_speed);
    (cap * (gap - SLOPE_SNAP_MIN_DISTANCE) / GROUND_SNAP_DISTANCE).min(cap)
}

fn spawn_translation(ground_height: f32) -> Vec3 {
    Vec3::new(0.0, ground_height + CHARACTER_SPAWN_OFFSET_Y, 0.0)
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
    let spawn_translation =
        spawn_translation(world.get_resource::<SpawnGroundHeight>().map_or(0.0, |h| h.0));

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
            ContextActive(true),
            ContextActivity::<GameplayInput>::ACTIVE,
            RigidBody::Dynamic,
            Collider::capsule(CAPSULE_RADIUS, CAPSULE_HEIGHT),
            LockedAxes::ROTATION_LOCKED,
            Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
            Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
            LinearVelocity::default(),
            GravityScale(Character::default().base_gravity),
            GroundState::default(),
            WaterImmersion::default(),
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

pub(crate) fn character_ground_check(
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
        &Transform,
        &WaterImmersion,
        &mut GroundState,
        &mut LinearVelocity,
        &mut GravityScale,
    )>,
) {
    let (character, transform, immersion, mut ground, mut velocity, mut gravity_scale) =
        character_query.into_inner();
    let dt = time.delta_secs();

    let move_vec = ***move_action;
    let sprinting = ***sprint_action;
    let jumping = ***jump_action;
    let crouching = ***crouch_action;
    let cam_yaw = facing.yaw;

    let move_dir = camera_relative_direction(move_vec, cam_yaw).normalize_or_zero();

    let target_speed = if sprinting {
        character.sprint_speed
    } else {
        character.walk_speed
    };

    if is_swimming(immersion) {
        gravity_scale.0 = 0.0;
        ground.is_grounded = false;

        let swim_up = jumping && !ground.jump_held_last_frame;
        let current = Vec3::new(immersion.flow.x, 0.0, immersion.flow.y);
        let target_horiz = move_dir * target_speed * SWIM_TARGET_SCALE + current * SWIM_CURRENT;

        let current_horiz = Vec3::new(velocity.x, 0.0, velocity.z);
        let mut new_horiz =
            accelerate_toward(current_horiz, target_horiz, character.air_acceleration, dt);
        new_horiz *= 1.0 - (SWIM_DRAG * dt).min(1.0);
        velocity.x = new_horiz.x;
        velocity.z = new_horiz.z;

        velocity.y = swim_vertical_velocity(
            immersion.surface_y,
            transform.translation.y,
            velocity.y,
            dt,
            swim_up,
            crouching,
        );

        ground.jump_held_last_frame = jumping;
        return;
    }

    let target_horiz = move_dir * target_speed;

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
        jumping,
        ground.jump_held_last_frame,
        ground.is_grounded,
        ground.jump_cooldown_timer,
    ) {
        velocity.y = character.jump_speed;
        ground.is_grounded = false;
        ground.jump_cooldown_timer = character.jump_cooldown;
        gravity_scale.0 = character.base_gravity;
    } else if ground.is_grounded {
        gravity_scale.0 = 0.0;

        let has_horiz_vel =
            velocity.x.abs() > HORIZ_VEL_EPSILON || velocity.z.abs() > HORIZ_VEL_EPSILON;
        if has_horiz_vel && ground.ground_normal.y > MIN_GROUND_NORMAL_Y {
            let horiz_speed = Vec3::new(velocity.x, 0.0, velocity.z).length();
            velocity.y = slope_tangent_y(velocity.x, velocity.z, ground.ground_normal)
                - slope_snap_speed(ground.ground_distance, horiz_speed);
        } else {
            velocity.y = 0.0;
        }
    } else {
        gravity_scale.0 = air_gravity_scale(velocity.y, jumping, character);
    }

    ground.jump_held_last_frame = jumping;
}

fn character_rotate_body(
    time: Res<Time>,
    move_action: Single<&Action<Move>>,
    align_action: Single<&Action<AlignToCamera>>,
    facing: Single<&CameraFacing>,
    character: Single<&Character>,
    mut body_transform: Single<&mut Transform, (With<CharacterBody>, Without<Character>)>,
) {
    let move_vec = ***move_action;
    let alt_held = ***align_action;
    let cam_yaw = facing.yaw;
    let yaw_rot = Quat::from_rotation_y(cam_yaw);

    let first_person = facing.first_person;
    if first_person {
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

pub(crate) fn respawn_character_to(
    character: &Character,
    ground_height: f32,
    transform: &mut Transform,
    velocity: &mut LinearVelocity,
    ground: &mut GroundState,
    gravity: &mut GravityScale,
) {
    transform.translation = spawn_translation(ground_height);
    velocity.0 = Vec3::ZERO;
    *ground = GroundState::default();
    gravity.0 = character.base_gravity;
}

fn respawn_character(
    ground_height: Res<SpawnGroundHeight>,
    query: Single<(
        &mut Transform,
        &mut LinearVelocity,
        &mut GroundState,
        &mut GravityScale,
        &Character,
    )>,
) {
    let (mut transform, mut velocity, mut ground, mut gravity, character) = query.into_inner();
    respawn_character_to(
        character,
        ground_height.0,
        &mut transform,
        &mut velocity,
        &mut ground,
        &mut gravity,
    );
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
    fn slope_snap_ramps_and_caps() {
        assert_eq!(slope_snap_speed(SLOPE_SNAP_MIN_DISTANCE, 0.0), 0.0);
        assert_eq!(slope_snap_speed(SLOPE_SNAP_MIN_DISTANCE / 2.0, 0.0), 0.0);
        let mid = slope_snap_speed(0.1, 0.0);
        assert!(mid > 0.0 && mid < SLOPE_SNAP_MAX_SPEED, "should ramp between the bounds");
        assert_eq!(slope_snap_speed(100.0, 0.0), SLOPE_SNAP_MAX_SPEED, "must cap, or steps slam");
    }

    #[test]
    fn downhill_snap_scales_with_horizontal_speed() {
        assert_eq!(slope_snap_cap(0.0), SLOPE_SNAP_MAX_SPEED);
        assert_eq!(slope_snap_cap(2.0), SLOPE_SNAP_MAX_SPEED);
        assert_eq!(slope_snap_cap(12.0), 12.0);

        let slow = slope_snap_speed(0.3, 0.0);
        let fast = slope_snap_speed(0.3, 12.0);
        assert!(fast > slow, "sprinting downhill must snap down harder than walking");
        assert!(fast <= 12.0 + 1e-4, "the snap must stay under the speed-scaled cap");
        assert!(
            fast >= 12.0 * (0.3 - SLOPE_SNAP_MIN_DISTANCE) / GROUND_SNAP_DISTANCE - 1e-3,
            "the snap must keep up with a 45 degree crest at sprint speed"
        );
        assert_eq!(slope_snap_speed(100.0, 12.0), 12.0, "the cap still binds at speed");
    }

    #[test]
    fn swimming_needs_deep_liquid_water() {
        let deep = WaterImmersion {
            submerged: true,
            depth: CAPSULE_HEIGHT * 0.5 + 0.1,
            surface_y: 0.0,
            flow: Vec2::ZERO,
            frozen: false,
        };
        assert!(is_swimming(&deep), "chest-deep liquid water means swimming");

        let shallow = WaterImmersion { depth: CAPSULE_HEIGHT * 0.5, ..deep };
        assert!(!is_swimming(&shallow), "ankle-deep water must stay walking");

        let icy = WaterImmersion { frozen: true, ..deep };
        assert!(!is_swimming(&icy), "frozen water must not swim");

        let dry = WaterImmersion { submerged: false, ..deep };
        assert!(!is_swimming(&dry), "no immersion, no swim");
    }

    #[test]
    fn buoyancy_springs_toward_the_surface_and_damps() {
        assert!(buoyancy_acceleration(0.0, -1.0, 0.0) > 0.0, "below the surface: pushed up");
        assert!(buoyancy_acceleration(0.0, 1.0, 0.0) < 0.0, "above the surface: pulled down");
        assert_eq!(buoyancy_acceleration(0.0, 0.0, 0.0), 0.0, "at the surface at rest: nothing");
        assert!(
            buoyancy_acceleration(0.0, 0.0, 2.0) < 0.0,
            "rising at the surface must be damped"
        );
    }

    #[test]
    fn swim_vertical_velocity_respects_swim_and_dive() {
        let coast = swim_vertical_velocity(0.0, -1.0, 0.0, 1.0 / 60.0, false, false);
        assert!(coast > 0.0, "buoyancy alone rises toward the surface");

        let up = swim_vertical_velocity(0.0, -1.0, 0.0, 1.0 / 60.0, true, false);
        assert!(up >= SWIM_UP_SPEED, "swim-up must reach the stroke speed");
        assert!(up > coast);

        let down = swim_vertical_velocity(0.0, -1.0, 0.0, 1.0 / 60.0, false, true);
        assert!(down < coast, "diving pushes down");
    }
}
