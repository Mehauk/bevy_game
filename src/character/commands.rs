use avian3d::prelude::*;
use bevy::prelude::*;

use crate::character::spawn_translation;
use crate::character::types::{
    Character, GroundState, MovementState, SubmersionState, horizontal_speed,
};
use crate::utils::SpawnGroundHeight;
use crate::utils::registry::commands::{CommandArgs, CommandOutput};

const DEFAULT_SPEED_DELTA: f32 = 2.0;

pub(crate) fn respawn_character(
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

pub(crate) fn debug_movement_state(
    states: Query<&MovementState, With<Character>>,
) -> Option<String> {
    states.iter().next().map(|state| state.to_string())
}

pub(crate) fn debug_movement_speed(
    velocities: Query<&LinearVelocity, With<Character>>,
) -> Option<String> {
    velocities
        .iter()
        .next()
        .map(|velocity| format!("{:.2} m/s", horizontal_speed(velocity.0)))
}

pub(crate) fn debug_water_depth(
    submersion: Single<&SubmersionState, With<Character>>,
) -> Option<String> {
    Some(if submersion.is_submerged() {
        format!("{:.2} m", submersion.depth)
    } else {
        "dry".to_string()
    })
}

pub(crate) fn debug_position(transforms: Query<&Transform, With<Character>>) -> Option<String> {
    transforms.iter().next().map(|transform| {
        let p = transform.translation;
        format!("({:.1}, {:.1}, {:.1})", p.x, p.y, p.z)
    })
}

fn with_player_character(
    query: &mut Query<&mut Character>,
    edit: impl FnOnce(&mut Character),
) -> bool {
    let Ok(mut character) = query.single_mut() else {
        warn!("No character found for speed command");
        return false;
    };
    edit(&mut character);
    true
}

pub(crate) fn set_character_speed(
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

pub(crate) fn increase_character_speed(
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
