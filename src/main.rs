use bevy::{
    dev_tools::fps_overlay::FpsOverlayPlugin,
    pbr::wireframe::WireframePlugin,
    prelude::*,
};

use character::CharacterPlugin;
use terrain::setup_terrain;
use utils::dev_utils::toggle_wireframe;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            FpsOverlayPlugin::default(),
            WireframePlugin::default(),
            CharacterPlugin,
        ))
        .add_systems(Startup, (setup_terrain, setup,))
        .add_systems(Update, (toggle_wireframe,))
        .run();
}

fn setup(
    mut commands: Commands,
) {
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_translation(Vec3::ONE).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

mod character {
    use bevy::{
        input::mouse::AccumulatedMouseMotion,
        prelude::*,
        window::{CursorGrabMode, CursorOptions, PrimaryWindow},
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum ViewMode {
        FirstPerson,
        #[default]
        ThirdPerson,
    }

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

    #[derive(Component)]
    pub struct Character {
        pub yaw: f32,
        pub pitch: f32,
        pub walk_speed: f32,
        pub sprint_speed: f32,
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
        // Capsule3d(radius = 0.4, length = 1.0) has total height 1.8, so y = 0.9 rests on y = 0.0.
        commands
            .spawn((
                Character::default(),
                Transform::from_xyz(0.0, 0.9, 0.0),
                Visibility::default(),
            ))
            .with_children(|parent| {
                parent.spawn((
                    CharacterBody,
                    Mesh3d(meshes.add(Capsule3d::new(0.4, 1.0))),
                    MeshMaterial3d(materials.add(Color::srgb(0.2, 0.45, 0.85))),
                ));

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
    ) {
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
        mut character: Single<&mut Character>,
    ) {
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
        character_query: Single<(&mut Character, &mut Transform)>,
    ) {
        if cursor_options.grab_mode != CursorGrabMode::Locked || mouse_motion.delta == Vec2::ZERO {
            return;
        }

        let (mut character, mut transform) = character_query.into_inner();
        character.yaw -= mouse_motion.delta.x * character.sensitivity;
        character.pitch =
            (character.pitch - mouse_motion.delta.y * character.sensitivity).clamp(-1.35, 1.35);

        transform.rotation = Quat::from_rotation_y(character.yaw);
    }

    fn character_move(
        time: Res<Time>,
        keyboard: Res<ButtonInput<KeyCode>>,
        character_query: Single<(&Character, &mut Transform)>,
    ) {
        let (character, mut transform) = character_query.into_inner();

        let mut direction = Vec3::ZERO;
        if keyboard.pressed(KeyCode::KeyW) {
            direction += *transform.forward();
        }
        if keyboard.pressed(KeyCode::KeyS) {
            direction += *transform.back();
        }
        if keyboard.pressed(KeyCode::KeyA) {
            direction += *transform.left();
        }
        if keyboard.pressed(KeyCode::KeyD) {
            direction += *transform.right();
        }

        direction.y = 0.0;
        if direction == Vec3::ZERO {
            return;
        }

        let sprinting =
            keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
        let speed = if sprinting {
            character.sprint_speed
        } else {
            character.walk_speed
        };

        transform.translation += direction.normalize() * speed * time.delta_secs();
    }

    fn sync_camera_and_body(
        character_query: Single<(&Character, &Transform), Without<CharacterCamera>>,
        mut camera_transform: Single<&mut Transform, With<CharacterCamera>>,
        mut body_visibility: Single<&mut Visibility, With<CharacterBody>>,
    ) {
        let (character, char_transform) = *character_query;
        let eye_pivot = Vec3::new(0.0, character.eye_height_offset, 0.0);

        match character.view_mode {
            ViewMode::FirstPerson => {
                **body_visibility = Visibility::Hidden;
                camera_transform.translation = eye_pivot;
                camera_transform.rotation = Quat::from_rotation_x(character.pitch);
            }
            ViewMode::ThirdPerson => {
                **body_visibility = Visibility::Visible;
                let offset = Quat::from_rotation_x(character.pitch)
                    * Vec3::new(0.0, 0.0, character.third_person_distance);

                let mut cam_pos = eye_pivot + offset;
                // Keep third-person camera above the ground plane (y = 0.0)
                let min_local_y = -char_transform.translation.y + 0.25;
                cam_pos.y = cam_pos.y.max(min_local_y);

                camera_transform.translation = cam_pos;
                camera_transform.look_at(eye_pivot, Vec3::Y);
            }
        }
    }

}

mod utils {
    pub mod dev_utils {
        use bevy::{
            pbr::wireframe::WireframeConfig,
            prelude::*,
        };

        pub fn toggle_wireframe(
            keyboard_input: Res<ButtonInput<KeyCode>>,
            mut wireframe_config: ResMut<WireframeConfig>,
        ) {
            if keyboard_input.just_pressed(KeyCode::KeyZ) {
                wireframe_config.global = !wireframe_config.global;
            }
        }
    }
}

mod terrain {
    use bevy::{
        mesh::VertexAttributeValues,
        prelude::*,
    };

    pub fn setup_terrain(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        let mut terrain = Mesh::from(Plane3d::default().mesh().size(200., 200.).subdivisions(100));

        if let Some(VertexAttributeValues::Float32x3(positions)) =
            terrain.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        {}

        commands.spawn((
            Mesh3d(meshes.add(terrain)),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
        ));
    }
}