use bevy::prelude::*;

use crate::distance_field::{DistanceFieldDimmedColor, DistanceFieldViewHidden};
use crate::terrain::{Sea, SkyBox};

const SKYBOX_VIEW_COLOR: Color = Color::srgb(0.015, 0.015, 0.018);

pub(super) fn register(app: &mut App) {
    app.add_observer(dim_the_sky_box);
    app.add_observer(hide_the_sea);
}

fn dim_the_sky_box(trigger: On<Add<SkyBox>>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert(DistanceFieldDimmedColor(SKYBOX_VIEW_COLOR));
}

fn hide_the_sea(trigger: On<Add<Sea>>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert(DistanceFieldViewHidden);
}
