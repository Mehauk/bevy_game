mod menus;
mod utils;

use bevy::{
    asset::io::AssetSourceBuilder,
    dev_tools::fps_overlay::FpsOverlayPlugin,
    prelude::*,
};
use bevy_hui::prelude::*;
use bevy_mod_scripting::BMSPlugin;

use menus::{MenuPlugin, command_menu::CommandMenuPlugin};
use utils::{DevUtilsPlugin, commands::CommandRegistryPlugin};

use character::CharacterPlugin;
use terrain::TerrainPlugin;

fn main() {
    let _ = std::fs::create_dir_all("mods");

    App::new()
        .register_asset_source("mods", AssetSourceBuilder::platform_default("mods", None))
        .add_plugins((
            DefaultPlugins,
            HuiPlugin,
            BMSPlugin,
            FpsOverlayPlugin::default(),
            CommandRegistryPlugin,
            MenuPlugin,
            CommandMenuPlugin,
            DevUtilsPlugin,
            CharacterPlugin,
            TerrainPlugin,
        ))
        .run();
}

mod character {
    use bevy::{
        input::mouse::AccumulatedMouseMotion,
        prelude::*,
        window::{CursorGrabMode, CursorOptions, PrimaryWindow},
    };

    use crate::menus::MenuManager;

    pub struct CharacterPlugin;

    impl Plugin for CharacterPlugin {
        fn build(&self, app: &mut App) {
            app.add_systems(Startup, setup_character).add_systems(
                Update,
                (
                    grab_cursor,
                    toggle_view_mode,
                    character_look,
                    character_move,
                    sync_camera_and_body,
                )
                    .chain(),
            );
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum ViewMode {
        FirstPerson,
        #[default]
        ThirdPerson,
    }

    #[derive(Component)]
    pub struct Character {
        pub yaw: f32,
        pub pitch: f32,
        pub walk_speed: f32,
        pub sprint_speed: f32,
        pub turn_speed: f32,
        pub sensitivity: f32,
        pub third_person_distance: f32,
        pub eye_height_offset: f32,
        pub view_mode: ViewMode,
    }

    impl Default for Character {
        fn default() -> Self {
            Self {
                yaw: 0.0,
                pitch: -0.25,
                walk_speed: 6.0,
                sprint_speed: 12.0,
                turn_speed: 12.0,
                sensitivity: 0.0025,
                third_person_distance: 6.0,
                eye_height_offset: 0.65,
                view_mode: ViewMode::ThirdPerson,
            }
        }
    }

    #[derive(Component)]
    struct CharacterBody;

    #[derive(Component)]
    struct CharacterCamera;

    fn setup_character(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        commands
            .spawn((
                Character::default(),
                Transform::from_xyz(0.0, 0.9, 0.0),
                Visibility::default(),
            ))
            .with_children(|parent| {
                parent
                    .spawn((
                        CharacterBody,
                        Mesh3d(meshes.add(Capsule3d::new(0.4, 1.0))),
                        MeshMaterial3d(materials.add(Color::srgb(0.2, 0.45, 0.85))),
                        Transform::default(),
                        Visibility::default(),
                    ))
                    .with_children(|body| {
                        body.spawn((
                            Mesh3d(meshes.add(Cuboid::new(0.45, 0.18, 0.2))),
                            MeshMaterial3d(materials.add(Color::srgb(0.1, 0.12, 0.18))),
                            Transform::from_xyz(0.0, 0.45, -0.35),
                        ));
                    });

                parent.spawn((
                    CharacterCamera,
                    Camera3d::default(),
                    Transform::default(),
                ));
            });
    }

    fn grab_cursor(
        mut cursor_options: Single<&mut CursorOptions, With<PrimaryWindow>>,
        mouse: Res<ButtonInput<MouseButton>>,
        keyboard: Res<ButtonInput<KeyCode>>,
        menu_manager: Res<MenuManager>,
    ) {
        if menu_manager.any_open() {
            cursor_options.visible = true;
            cursor_options.grab_mode = CursorGrabMode::None;
            return;
        }
        if mouse.just_pressed(MouseButton::Left) {
            cursor_options.visible = false;
            cursor_options.grab_mode = CursorGrabMode::Locked;
        }
        if keyboard.just_pressed(KeyCode::Escape) {
            cursor_options.visible = true;
            cursor_options.grab_mode = CursorGrabMode::None;
        }
    }

    fn toggle_view_mode(
        keyboard: Res<ButtonInput<KeyCode>>,
        menu_manager: Res<MenuManager>,
        mut character: Single<&mut Character>,
    ) {
        if menu_manager.any_open() {
            return;
        }
        let shift_held =
            keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

        if shift_held && keyboard.just_pressed(KeyCode::KeyF) {
            character.view_mode = match character.view_mode {
                ViewMode::FirstPerson => ViewMode::ThirdPerson,
                ViewMode::ThirdPerson => ViewMode::FirstPerson,
            };
        }
    }

    fn character_look(
        cursor_options: Single<&CursorOptions, With<PrimaryWindow>>,
        mouse_motion: Res<AccumulatedMouseMotion>,
        menu_manager: Res<MenuManager>,
        mut character: Single<&mut Character>,
    ) {
        if menu_manager.any_open()
            || cursor_options.grab_mode != CursorGrabMode::Locked
            || mouse_motion.delta == Vec2::ZERO
        {
            return;
        }

        character.yaw -= mouse_motion.delta.x * character.sensitivity;
        character.pitch =
            (character.pitch - mouse_motion.delta.y * character.sensitivity).clamp(-1.35, 1.35);
    }

    fn character_move(
        time: Res<Time>,
        keyboard: Res<ButtonInput<KeyCode>>,
        menu_manager: Res<MenuManager>,
        character_query: Single<
            (&Character, &mut Transform),
            (Without<CharacterBody>, Without<CharacterCamera>),
        >,
        mut body_transform: Single<
            &mut Transform,
            (With<CharacterBody>, Without<Character>, Without<CharacterCamera>),
        >,
    ) {
        if menu_manager.any_open() {
            return;
        }
        let (character, mut transform) = character_query.into_inner();

        let yaw_rot = Quat::from_rotation_y(character.yaw);
        let forward = yaw_rot * Vec3::NEG_Z;
        let right = yaw_rot * Vec3::X;

        let mut direction = Vec3::ZERO;
        if keyboard.pressed(KeyCode::KeyW) {
            direction += forward;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            direction -= forward;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            direction += right;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            direction -= right;
        }

        let alt_held =
            keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);

        if direction != Vec3::ZERO {
            let move_dir = direction.normalize();
            let sprinting =
                keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
            let speed = if sprinting {
                character.sprint_speed
            } else {
                character.walk_speed
            };

            transform.translation += move_dir * speed * time.delta_secs();

            if character.view_mode == ViewMode::ThirdPerson && !alt_held {
                let target_yaw = (-move_dir.x).atan2(-move_dir.z);
                let target_rot = Quat::from_rotation_y(target_yaw);
                let turn_alpha = (character.turn_speed * time.delta_secs()).min(1.0);
                body_transform.rotation = body_transform.rotation.slerp(target_rot, turn_alpha);
            }
        }

        if character.view_mode == ViewMode::FirstPerson {
            body_transform.rotation = yaw_rot;
        } else if alt_held {
            let turn_alpha = (character.turn_speed * time.delta_secs()).min(1.0);
            body_transform.rotation = body_transform.rotation.slerp(yaw_rot, turn_alpha);
        }
    }

    fn sync_camera_and_body(
        character_query: Single<
            (&Character, &Transform),
            (Without<CharacterCamera>, Without<CharacterBody>),
        >,
        mut camera_transform: Single<
            &mut Transform,
            (With<CharacterCamera>, Without<Character>, Without<CharacterBody>),
        >,
        mut body_visibility: Single<&mut Visibility, With<CharacterBody>>,
    ) {
        let (character, char_transform) = *character_query;
        let eye_pivot = Vec3::new(0.0, character.eye_height_offset, 0.0);
        let cam_yaw_rot = Quat::from_rotation_y(character.yaw);
        let cam_pitch_rot = Quat::from_rotation_x(character.pitch);

        match character.view_mode {
            ViewMode::FirstPerson => {
                **body_visibility = Visibility::Hidden;
                camera_transform.translation = eye_pivot;
                camera_transform.rotation = cam_yaw_rot * cam_pitch_rot;
            }
            ViewMode::ThirdPerson => {
                **body_visibility = Visibility::Visible;
                let offset =
                    cam_yaw_rot * cam_pitch_rot * Vec3::new(0.0, 0.0, character.third_person_distance);

                let mut cam_pos = eye_pivot + offset;
                let min_local_y = -char_transform.translation.y + 0.25;
                cam_pos.y = cam_pos.y.max(min_local_y);

                camera_transform.translation = cam_pos;
                camera_transform.look_at(eye_pivot, Vec3::Y);
            }
        }
    }
}

mod terrain {
    use bevy::{
        mesh::VertexAttributeValues,
        prelude::*,
    };

    pub struct TerrainPlugin;

    impl Plugin for TerrainPlugin {
        fn build(&self, app: &mut App) {
            app.add_systems(Startup, setup_terrain);
        }
    }

    fn setup_terrain(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        let mut terrain = Mesh::from(Plane3d::default().mesh().size(200., 200.).subdivisions(100));

        if let Some(VertexAttributeValues::Float32x3(_positions)) =
            terrain.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        {}

        commands.spawn((
            Mesh3d(meshes.add(terrain)),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
        ));
    }
}