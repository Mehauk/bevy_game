use bevy::{core_pipeline::prepass::DepthPrepass, prelude::*};
use bevy_shader_kit::UnderwaterPostProcess;

use crate::camera::MainCamera;
use crate::distance_field::DistanceFieldView;
use crate::screenshot::ScreenshotCaptureCamera;
use crate::terrain::SeaState;
use crate::utils::registry::commands::{CommandAppExt, CommandOutput, command};
use crate::utils::registry::debug_info::{DebugInfoAppExt, debug_info};

const SUBMERGENCE_MARGIN: f32 = 0.5;
const SURFACE_EXIT_MARGIN: f32 = 1.0;

#[derive(Resource)]
struct UnderwaterToggle {
    enabled: bool,
}

impl Default for UnderwaterToggle {
    fn default() -> Self {
        Self { enabled: true }
    }
}

fn submersion_state(camera_y: f32, water_height: f32) -> &'static str {
    let delta = camera_y - water_height;
    if delta > SUBMERGENCE_MARGIN {
        "above"
    } else if delta < -SUBMERGENCE_MARGIN {
        "submerged"
    } else {
        "partial"
    }
}

fn lens_submerged(camera_y: f32, water_height: f32, was_submerged: bool) -> bool {
    let delta = camera_y - water_height;
    if was_submerged {
        delta < SURFACE_EXIT_MARGIN
    } else {
        delta < SUBMERGENCE_MARGIN
    }
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<UnderwaterToggle>()
        .add_observer(attach_underwater_post_process)
        .add_observer(mirror_underwater_capture)
        .add_systems(Update, sync_underwater_post_process);

    command!(
        app,
        "Toggle the underwater post-process effect",
        toggle_underwater
    );
    debug_info!(app, "Submersion", submersion_debug_info);
}

fn attach_underwater_post_process(trigger: On<Add<MainCamera>>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert(UnderwaterPostProcess::default());
}

fn mirror_underwater_capture(
    trigger: On<Add<ScreenshotCaptureCamera>>,
    mut commands: Commands,
    cameras: Query<(&DepthPrepass, &UnderwaterPostProcess), With<MainCamera>>,
) {
    let Some((depth_prepass, underwater)) = cameras.iter().next() else {
        return;
    };
    commands
        .entity(trigger.entity)
        .insert((depth_prepass.clone(), *underwater));
}

fn lens_surface_height(sea_state: &SeaState, camera: &GlobalTransform, time: f32) -> f32 {
    let lens = camera.translation();
    sea_state.surface_height_at(lens.x, lens.z, time)
}

fn sync_underwater_post_process(
    time: Res<Time>,
    toggle: Res<UnderwaterToggle>,
    field_view: Res<DistanceFieldView>,
    sea_state: Res<SeaState>,
    cameras: Query<&GlobalTransform, With<MainCamera>>,
    mut post_processes: Query<&mut UnderwaterPostProcess, With<MainCamera>>,
    mut was_submerged: Local<bool>,
) {
    let Ok(mut post_process) = post_processes.single_mut() else {
        return;
    };
    let Some(camera) = cameras.iter().next() else {
        return;
    };

    let now = time.elapsed_secs();
    let mut settings = *post_process;
    settings.time = now;
    settings.water_height = sea_state.sea_level;
    settings.swells = sea_state.effective_swells();
    settings.optics = sea_state.optics;

    let lens_in_water = lens_submerged(
        camera.translation().y,
        lens_surface_height(&sea_state, camera, now),
        *was_submerged,
    );
    *was_submerged = lens_in_water;
    settings.enabled = u32::from(toggle.enabled && lens_in_water && !field_view.active);
    *post_process = settings;
}

fn toggle_underwater(mut toggle: ResMut<UnderwaterToggle>, mut output: ResMut<CommandOutput>) {
    toggle.enabled = !toggle.enabled;
    output.set(if toggle.enabled {
        "Underwater post-process: on"
    } else {
        "Underwater post-process: off"
    });
}

fn submersion_debug_info(
    time: Res<Time>,
    sea_state: Res<SeaState>,
    cameras: Query<&GlobalTransform, With<MainCamera>>,
) -> Option<String> {
    let camera = cameras.iter().next()?;
    let water_height = lens_surface_height(&sea_state, camera, time.elapsed_secs());
    let delta = camera.translation().y - water_height;
    Some(format!(
        "{} ({delta:+.2}m)",
        submersion_state(camera.translation().y, water_height)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_height_maps_to_a_submersion_state() {
        assert_eq!(submersion_state(2.0, 0.0), "above");
        assert_eq!(submersion_state(0.2, 0.0), "partial");
        assert_eq!(submersion_state(-2.0, 0.0), "submerged");
    }

    #[test]
    fn the_lens_hysteresis_holds_the_effect_across_a_bob() {
        assert!(!lens_submerged(2.0, 0.0, false));
        assert!(lens_submerged(0.2, 0.0, false));
        assert!(!lens_submerged(0.7, 0.0, false));

        assert!(lens_submerged(0.7, 0.0, true));
        assert!(lens_submerged(-3.0, 0.0, true));
        assert!(!lens_submerged(1.2, 0.0, true));
    }
}
