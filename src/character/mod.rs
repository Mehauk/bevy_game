mod commands;
mod ground;
mod locomotion;
mod presentation;
mod types;

use avian3d::prelude::*;
use bevy::{prelude::*, transform::TransformSystems};
use bevy_enhanced_input::prelude::*;

use crate::utils::SpawnGroundHeight;
use crate::utils::registry::commands::{CommandAppExt, command};
use crate::utils::registry::debug_info::{DebugInfoAppExt, debug_info};

use commands::{
    debug_movement_speed, debug_movement_state, debug_position, debug_water_depth,
    increase_character_speed, respawn_character, set_character_speed,
};
use ground::{
    GROUND_PROBE_MAX_DISTANCE, GROUND_PROBE_OFFSET_Y, GROUND_PROBE_RADIUS, character_ground_check,
};
use locomotion::character_physics_move;
use presentation::{character_rotate_body, sync_body_visibility};

pub use locomotion::{camera_relative_direction, camera_relative_direction_3d};
pub use types::{
    CAPSULE_HEIGHT, CAPSULE_RADIUS, CHARACTER_SPAWN_OFFSET_Y, Character, GroundState,
    MovementState, SubmersionState, WaterPhysics,
};
pub(crate) use types::{CameraFacing, CharacterBody};

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
            .add_systems(
                Update,
                character_rotate_body.in_set(CharacterSystems::Playing),
            )
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
pub(crate) struct Move;

#[derive(InputAction)]
#[action_output(bool)]
pub(crate) struct Sprint;

#[derive(InputAction)]
#[action_output(bool)]
pub(crate) struct AlignToCamera;

#[derive(InputAction)]
#[action_output(bool)]
pub(crate) struct Jump;

#[derive(InputAction)]
#[action_output(bool)]
pub(crate) struct Crouch;

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
        })
        .unwrap();

    let entity_id = char_entity.id();

    char_entity
        .insert((
            CameraFacing::default(),
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
