use bevy::{
    prelude::*,
    transform::TransformSystems,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_enhanced_input::prelude::*;

use crate::camera::{
    CameraRig, CameraSystems, CameraTarget, MainCamera, ViewMode, WantedCursorLock,
};
use crate::character::{CameraFacing, Character, CharacterSystems};
use crate::input::{GameplayInput, set_context_active};
use crate::menus::MenuManager;
use crate::state::PlayState;
use crate::terrain::{SkyBox, TerrainMap};
use crate::utils::WaterImmersion;

pub(super) fn register(app: &mut App) {
    app.add_systems(
        PreUpdate,
        (
            sync_context_activity.before(EnhancedInputSystems::Update),
            sync_camera_facing,
        ),
    )
    .add_systems(
        FixedUpdate,
        sync_water_immersion
            .before(CharacterSystems::Playing)
            .run_if(in_state(PlayState::Running)),
    )
    .add_systems(Update, attach_camera_target)
    .add_systems(PostUpdate, sync_cursor_with_menus)
    .add_systems(
        PostUpdate,
        sync_skybox_to_camera
            .after(CameraSystems::UpdateTransform)
            .before(TransformSystems::Propagate),
    );
}

pub fn water_immersion_at(map: &TerrainMap, position: Vec3) -> WaterImmersion {
    let Some(sample) = map.water_surface_at(position.x, position.z) else {
        return WaterImmersion::default();
    };
    let depth = sample.surface_y - position.y;
    WaterImmersion {
        submerged: depth > 0.0,
        depth: depth.max(0.0),
        surface_y: sample.surface_y,
        flow: sample.flow,
        frozen: sample.frozen,
    }
}

fn sync_water_immersion(
    map: Res<TerrainMap>,
    mut characters: Query<(&Transform, &mut WaterImmersion), With<Character>>,
) {
    for (transform, mut immersion) in &mut characters {
        *immersion = water_immersion_at(&map, transform.translation);
    }
}

fn sync_camera_facing(rig: Single<&CameraRig>, mut facing: Single<&mut CameraFacing>) {
    facing.yaw = rig.yaw;
    facing.first_person = rig.view_mode == ViewMode::FirstPerson;
}

fn attach_camera_target(
    mut attached: Local<bool>,
    player: Single<Entity, With<Character>>,
    mut commands: Commands,
) {
    if *attached {
        return;
    }
    *attached = true;
    commands.entity(*player).insert(CameraTarget::default());
}

fn sync_context_activity(
    menu_manager: Res<MenuManager>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut prev_open: Local<Option<bool>>,
    mut held_keys_on_close: Local<Vec<KeyCode>>,
    mut held_buttons_on_close: Local<Vec<MouseButton>>,
    q_gameplay: Query<Entity, With<GameplayInput>>,
    mut commands: Commands,
) {
    let open = menu_manager.any_open();
    let transitioned = *prev_open != Some(open);
    if transitioned {
        *prev_open = Some(open);
        if open {
            held_keys_on_close.clear();
            held_buttons_on_close.clear();
            for entity in &q_gameplay {
                set_context_active::<GameplayInput>(&mut commands, entity, false);
            }
            return;
        }
        *held_keys_on_close = keys.get_pressed().copied().collect();
        *held_buttons_on_close = mouse_buttons.get_pressed().copied().collect();
    }

    if !open
        && (transitioned || !held_keys_on_close.is_empty() || !held_buttons_on_close.is_empty())
    {
        held_keys_on_close.retain(|k| keys.pressed(*k));
        held_buttons_on_close.retain(|b| mouse_buttons.pressed(*b));
        if held_keys_on_close.is_empty() && held_buttons_on_close.is_empty() {
            for entity in &q_gameplay {
                set_context_active::<GameplayInput>(&mut commands, entity, true);
            }
        }
    }
}

fn sync_cursor_with_menus(
    wanted_lock: Res<WantedCursorLock>,
    menu_manager: Res<MenuManager>,
    primary_window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let (window, mut cursor_options) = primary_window.into_inner();
    let should_lock = wanted_lock.0 && !menu_manager.any_open() && window.focused;

    let (desired_visible, desired_grab) = if should_lock {
        (false, CursorGrabMode::Locked)
    } else {
        (true, CursorGrabMode::None)
    };

    if cursor_options.visible != desired_visible {
        cursor_options.visible = desired_visible;
    }
    if cursor_options.grab_mode != desired_grab {
        cursor_options.grab_mode = desired_grab;
    }
}

fn sync_skybox_to_camera(
    camera: Query<&Transform, (With<MainCamera>, Without<SkyBox>)>,
    mut skybox: Single<&mut Transform, (With<SkyBox>, Without<MainCamera>)>,
) {
    if let Some(cam_transform) = camera.iter().next() {
        skybox.translation = cam_transform.translation;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;

    #[test]
    fn the_bridge_marks_immersion_only_in_water() {
        let map = TerrainMap::compact();
        let field = map.heights();
        let lattice = (0..field.len())
            .map(|i| field.world_of(field.coords(i).0, field.coords(i).1))
            .collect::<Vec<_>>();
        let water_point = lattice
            .iter()
            .find(|p| map.water_surface_at(p.x, p.y).is_some())
            .expect("the compact world has water");
        let dry_point = lattice
            .iter()
            .find(|p| map.water_surface_at(p.x, p.y).is_none())
            .expect("the compact world has dry land");
        let surface_y = map
            .water_surface_at(water_point.x, water_point.y)
            .expect("water sample")
            .surface_y;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(
            1.0 / 60.0,
        )));
        app.insert_resource(map);
        app.add_systems(FixedUpdate, sync_water_immersion);

        let swimmer = app
            .world_mut()
            .spawn((
                Character::default(),
                Transform::from_xyz(water_point.x, surface_y - 1.0, water_point.y),
                WaterImmersion::default(),
            ))
            .id();
        let walker = app
            .world_mut()
            .spawn((
                Character::default(),
                Transform::from_xyz(dry_point.x, 0.0, dry_point.y),
                WaterImmersion::default(),
            ))
            .id();

        app.update();
        app.update();

        let swim = app.world().get::<WaterImmersion>(swimmer).expect("swimmer");
        assert!(swim.submerged, "a character under the surface must be submerged");
        assert!(swim.depth > 0.9 && swim.surface_y.is_finite());
        let walk = app.world().get::<WaterImmersion>(walker).expect("walker");
        assert!(!walk.submerged, "dry land must report no immersion");
        assert_eq!(walk.depth, 0.0);
    }
}
