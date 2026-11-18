mod input;
mod menus;
mod utils;

use bevy::{
    asset::io::AssetSourceBuilder,
    dev_tools::fps_overlay::FpsOverlayPlugin,
    prelude::*,
};
use bevy_flair::prelude::*;
use bevy_mod_scripting::BMSPlugin;

use character::CharacterPlugin;
use input::InputPlugin;
use menus::{
    MenuPlugin, command_menu::CommandMenuPlugin, keyboard_menu::KeyboardMenuPlugin,
};
use terrain::TerrainPlugin;
use utils::{DevUtilsPlugin, commands::CommandRegistryPlugin};

fn main() {
    let _ = std::fs::create_dir_all("mods");

    App::new()
        .register_asset_source("mods", AssetSourceBuilder::platform_default("mods", None))
        .add_plugins((
            DefaultPlugins,
            FlairPlugin,
            BMSPlugin,
            FpsOverlayPlugin::default(),
            CommandRegistryPlugin,
            InputPlugin,
            MenuPlugin,
            CommandMenuPlugin,
            KeyboardMenuPlugin,
            DevUtilsPlugin,
            CharacterPlugin,
            TerrainPlugin,
        ))
        .run();
}

mod character {
    use bevy::{
        prelude::*,
        window::{CursorGrabMode, CursorOptions, PrimaryWindow},
    };
    use bevy_enhanced_input::prelude::*;

    use crate::{
        input::{ContextActive, GameplayInput, HotkeyLabel},
        menus::MenuManager,
        utils::commands::{CommandAppExt, command},
    };

    pub struct CharacterPlugin;

    impl Plugin for CharacterPlugin {
        fn build(&self, app: &mut App) {
            app.init_resource::<WantedCursorLock>()
                .add_systems(Startup, setup_character)
                .add_observer(on_grab_cursor)
                .add_observer(on_release_cursor)
                .add_observer(on_toggle_view_mode)
                .add_observer(on_character_look)
                .add_systems(Update, (character_move, sync_camera_and_body).chain())
                .add_systems(PostUpdate, sync_cursor_with_menus);

            command!(
                app,
                "toggle between first and third person view",
                toggle_view_mode
            );
        }
    }

    #[derive(Resource, Default)]
    struct WantedCursorLock(bool);

    #[derive(InputAction)]
    #[action_output(Vec2)]
    struct Move;

    #[derive(InputAction)]
    #[action_output(Vec2)]
    struct Look;

    #[derive(InputAction)]
    #[action_output(bool)]
    struct Sprint;

    #[derive(InputAction)]
    #[action_output(bool)]
    struct AlignToCamera;

    #[derive(InputAction)]
    #[action_output(bool)]
    struct ToggleViewMode;

    #[derive(InputAction)]
    #[action_output(bool)]
    struct GrabCursor;

    #[derive(InputAction)]
    #[action_output(bool)]
    struct ReleaseCursor;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum ViewMode {
        FirstPerson,
        #[default]
        ThirdPerson,
    }

    #[derive(Component, Clone)]
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

    fn setup_character(world: &mut World) {
        let capsule_mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Capsule3d::new(0.4, 1.0));
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
        let toggle_view: Binding = KeyCode::KeyF.with_mod_keys(ModKeys::SHIFT).into();

        let mut char_entity = world
            .spawn_scene(bsn! {
                Character
                Transform::from_xyz(0.0, 0.9, 0.0)
                Visibility::default()
                GameplayInput
                Actions<GameplayInput> [
                    Action::<Move>::new()
                    reset_settings
                    Bindings [
                        Binding(KeyCode::KeyW)
                        HotkeyLabel("Move Forward")
                        ~SwizzleAxis::YXZ
                        --
                        Binding(KeyCode::KeyS)
                        HotkeyLabel("Move Back")
                        ~Negate::all()
                        ~SwizzleAxis::YXZ
                        --
                        Binding(KeyCode::KeyD)
                        HotkeyLabel("Move Right")
                        --
                        Binding(KeyCode::KeyA)
                        HotkeyLabel("Move Left")
                        ~Negate::all()
                    ]
                    --
                    Action::<Look>::new()
                    Bindings [
                        Binding::mouse_motion()
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
                    Action::<ToggleViewMode>::new()
                    reset_settings
                    HotkeyLabel("Toggle 1st/3rd View")
                    Bindings [
                        toggle_view
                    ]
                    --
                    Action::<GrabCursor>::new()
                    reset_settings
                    Bindings [
                        Binding(MouseButton::Left)
                    ]
                    --
                    Action::<ReleaseCursor>::new()
                    reset_settings
                    HotkeyLabel("Unlock Cursor")
                    Bindings [
                        Binding(KeyCode::Escape)
                    ]
                ]
            })
            .unwrap();

        char_entity
            .insert((
                ContextActive(true),
                ContextActivity::<GameplayInput>::ACTIVE,
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

                parent.spawn((CharacterCamera, Camera3d::default(), Transform::default()));
            });
    }

    fn on_grab_cursor(_event: On<Start<GrabCursor>>, mut wanted_lock: ResMut<WantedCursorLock>) {
        wanted_lock.0 = true;
    }

    fn on_release_cursor(
        _event: On<Start<ReleaseCursor>>,
        mut wanted_lock: ResMut<WantedCursorLock>,
    ) {
        wanted_lock.0 = false;
    }

    fn sync_cursor_with_menus(
        wanted_lock: Res<WantedCursorLock>,
        menu_manager: Res<MenuManager>,
        mut prev_locked: Local<Option<bool>>,
        mut cursor_options: Single<&mut CursorOptions, With<PrimaryWindow>>,
    ) {
        let should_lock = wanted_lock.0 && !menu_manager.any_open();
        if *prev_locked == Some(should_lock) {
            return;
        }
        *prev_locked = Some(should_lock);

        if should_lock {
            cursor_options.visible = false;
            cursor_options.grab_mode = CursorGrabMode::Locked;
        } else {
            cursor_options.visible = true;
            cursor_options.grab_mode = CursorGrabMode::None;
        }
    }

    fn toggle_view_mode(mut character: Single<&mut Character>) {
        character.view_mode = match character.view_mode {
            ViewMode::FirstPerson => ViewMode::ThirdPerson,
            ViewMode::ThirdPerson => ViewMode::FirstPerson,
        };
    }

    fn on_toggle_view_mode(_event: On<Start<ToggleViewMode>>, character: Single<&mut Character>) {
        toggle_view_mode(character);
    }

    fn on_character_look(
        event: On<Fire<Look>>,
        cursor_options: Single<&CursorOptions, With<PrimaryWindow>>,
        mut character: Single<&mut Character>,
    ) {
        if cursor_options.grab_mode != CursorGrabMode::Locked || event.value == Vec2::ZERO {
            return;
        }
        character.yaw -= event.value.x * character.sensitivity;
        character.pitch =
            (character.pitch - event.value.y * character.sensitivity).clamp(-1.35, 1.35);
    }

    fn character_move(
        time: Res<Time>,
        move_action: Single<&Action<Move>>,
        sprint_action: Single<&Action<Sprint>>,
        align_action: Single<&Action<AlignToCamera>>,
        character_query: Single<
            (&Character, &mut Transform),
            (Without<CharacterBody>, Without<CharacterCamera>),
        >,
        mut body_transform: Single<
            &mut Transform,
            (
                With<CharacterBody>,
                Without<Character>,
                Without<CharacterCamera>,
            ),
        >,
    ) {
        let (character, mut transform) = character_query.into_inner();
        let move_vec = ***move_action;
        let sprinting = ***sprint_action;
        let alt_held = ***align_action;

        let yaw_rot = Quat::from_rotation_y(character.yaw);
        let forward = yaw_rot * Vec3::NEG_Z;
        let right = yaw_rot * Vec3::X;
        let direction = right * move_vec.x + forward * move_vec.y;

        if direction != Vec3::ZERO {
            let move_dir = direction.normalize();
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
            (
                With<CharacterCamera>,
                Without<Character>,
                Without<CharacterBody>,
            ),
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
                let offset = cam_yaw_rot
                    * cam_pitch_rot
                    * Vec3::new(0.0, 0.0, character.third_person_distance);

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
    use bevy::{mesh::VertexAttributeValues, prelude::*};

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