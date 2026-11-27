use bevy::prelude::*;
use crest_water::interaction::SphereWaterInteraction;

use crate::character::{CAPSULE_RADIUS, Character};

const HULL_MARGIN: f32 = 0.15;
const HULL_WEIGHT: f32 = 3.0;

pub(super) fn register(app: &mut App) {
    app.add_observer(attach_sphere_interaction);
}

fn attach_sphere_interaction(trigger: On<Add<Character>>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert(SphereWaterInteraction::new(
            CAPSULE_RADIUS + HULL_MARGIN,
            HULL_WEIGHT,
        ));
}
