use bevy::{core_pipeline::prepass::DepthPrepass, prelude::*};
use crest_water::underwater::UnderwaterFog;

use crate::camera::MainCamera;
use crate::screenshot::ScreenshotCaptureCamera;

pub(super) fn register(app: &mut App) {
    app.add_observer(attach_underwater_fog)
        .add_observer(mirror_underwater_capture);
}

fn attach_underwater_fog(trigger: On<Add<MainCamera>>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert(UnderwaterFog::default());
}

fn mirror_underwater_capture(
    trigger: On<Add<ScreenshotCaptureCamera>>,
    mut commands: Commands,
    cameras: Query<(&DepthPrepass, &UnderwaterFog), With<MainCamera>>,
) {
    let Some((depth_prepass, fog)) = cameras.iter().next() else {
        return;
    };
    commands
        .entity(trigger.entity)
        .insert((depth_prepass.clone(), *fog));
}
