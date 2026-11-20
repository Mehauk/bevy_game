//! Cross-module integrations.
//!
//! Feature modules (`input`, `menus`, `camera`, `terrain`, `state`, `character`) are each
//! written to stand alone: none imports another feature module. Everything that genuinely
//! spans two of them lives here — the only place allowed to know both sides. This is what
//! makes each system pluggable: drop a feature in or take it out, and only this module (and
//! `main.rs`'s plugin list) changes.
//!
//! Three kinds of glue live here:
//! - **Menu wiring** — a menu that opens on a hotkey and/or console command ties `menus` to
//!   `input` and the command console. [`wire_menu`] performs that composite registration, and
//!   the concrete menus (keyboard overlay, command palette) are declared here rather than
//!   leaking a `menus` dependency into `input` and the command console.
//! - **Bridging systems** — [`sync_context_activity`] (menus → input),
//!   [`sync_cursor_with_menus`] (menus → camera) and [`sync_skybox_to_camera`] (camera →
//!   terrain) each read one module's state to drive another's. They are moved verbatim from
//!   the modules they used to live in; their behaviour is unchanged.
//! - **Cross-module ordering** — the constraint that input resolves before the menu engine.

use bevy::{
    prelude::*,
    transform::TransformSystems,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_enhanced_input::prelude::*;

use crate::camera::{CameraRig, CameraSystems, CameraTarget, MainCamera, ViewMode, WantedCursorLock};
use crate::character::{CameraFacing, Character, CharacterSystems};
use crate::input::{GameplayInput, InputAppExt, InputUpdateSystems, set_context_active};
use crate::menus::{MenuManager, MenuUpdateSystems, ToggleMenu};
use crate::state::PlayState;
use crate::terrain::SkyBox;
use crate::utils::registry::commands::CommandAppExt;

/// Hotkey action that toggles the keyboard overlay (the rebinding UI).
#[derive(InputAction)]
#[action_output(bool)]
struct ToggleKeyboardOverlay;

/// Hotkey action that toggles the command palette (the console's UI).
#[derive(InputAction)]
#[action_output(bool)]
struct ToggleCommandPalette;

/// How a menu is opened: an optional hotkey and an optional console command.
///
/// This is the *wiring* vocabulary — it names a hotkey action and a console command, both of
/// which belong to other modules — so it lives here, not in the menu engine.
pub struct MenuSpec {
    pub id: &'static str,
    pub hotkey_label: &'static str,
    /// `(command name, description)` to expose in the command palette, if any.
    pub command: Option<(&'static str, &'static str)>,
    pub bindings: Vec<Binding>,
}

pub struct IntegrationPlugin;

impl Plugin for IntegrationPlugin {
    fn build(&self, app: &mut App) {
        // Input resolves its actions before the menu engine reads them. This ordering spans
        // two modules, so it is asserted here rather than inside either of them.
        app.configure_sets(
            Update,
            (
                InputUpdateSystems::SyncActionContexts,
                InputUpdateSystems::ApplyContextSync,
                InputUpdateSystems::ProcessRemap,
                InputUpdateSystems::SyncHotkeyRegistry,
            )
                .before(MenuUpdateSystems),
        );

        // The keyboard overlay is the front-end of the hotkey registry and remapping in
        // `input`; the command palette is the front-end of the command console. Both tie a
        // menu to a hotkey and (optionally) a console command, which is exactly the wiring
        // this module owns.
        wire_menu::<ToggleKeyboardOverlay>(
            app,
            MenuSpec {
                id: "keyboard",
                hotkey_label: "Keyboard Overlay",
                command: Some(("menu>keyboard", "Toggle virtual keyboard overlay")),
                bindings: vec![
                    Binding::from(KeyCode::F1),
                    KeyCode::KeyK
                        .with_mod_keys(ModKeys::CONTROL | ModKeys::SHIFT)
                        .into(),
                ],
            },
        );
        wire_menu::<ToggleCommandPalette>(
            app,
            MenuSpec {
                id: "command_palette",
                hotkey_label: "Command Palette",
                command: None,
                bindings: vec![Binding::from(KeyCode::Backquote)],
            },
        );

        // The player acts only while playing. `character` defines the set; the gate that names
        // the state machine lives here so `character` never imports `state`.
        app.configure_sets(
            FixedUpdate,
            CharacterSystems::Playing.run_if(in_state(PlayState::Running)),
        )
        .configure_sets(
            Update,
            CharacterSystems::Playing.run_if(in_state(PlayState::Running)),
        );

        app.add_systems(
            PreUpdate,
            (
                sync_context_activity.before(EnhancedInputSystems::Update),
                sync_camera_facing,
            ),
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
}

/// Copy the camera state the character needs onto its [`CameraFacing`] component, so the
/// movement and visibility systems read a character-local value instead of the camera rig.
/// A `camera → character` bridge.
fn sync_camera_facing(rig: Single<&CameraRig>, mut facing: Single<&mut CameraFacing>) {
    facing.yaw = rig.yaw;
    facing.first_person = rig.view_mode == ViewMode::FirstPerson;
}

/// Tell the camera to follow the player by giving the character a [`CameraTarget`]. Done once,
/// as soon as the character exists. A `character → camera` bridge that keeps `CameraTarget`
/// (a camera type) out of the character module.
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

/// Wire one menu declaration: its console command (if any) and its consuming hotkey, both
/// of which write [`ToggleMenu`] so the menu engine opens/closes it.
///
/// This is the composite that used to live in `MenuPlugin`; it is here because it reaches into
/// three modules at once (menus, input, the command console) and neither of them should have
/// to know about the others to declare a menu.
fn wire_menu<A: InputAction<Output = bool> + Send + Sync + 'static>(
    app: &mut App,
    spec: MenuSpec,
) {
    let menu_id = spec.id;

    if let Some((cmd_name, desc)) = spec.command {
        app.register_command(cmd_name, desc, move |mut writer: MessageWriter<ToggleMenu>| {
            writer.write(ToggleMenu(menu_id.to_string()));
        });
    }

    if !spec.bindings.is_empty() {
        app.register_gameplay_hotkey::<A, _, _, _>(
            spec.hotkey_label,
            // A menu hotkey is a one-shot toggle: it consumes the press so the key does
            // not also reach whatever else is bound to it.
            ActionSettings {
                consume_input: true,
                require_reset: true,
                ..default()
            },
            spec.bindings,
            move |_ev: On<Start<A>>, mut writer: MessageWriter<ToggleMenu>| {
                writer.write(ToggleMenu(menu_id.to_string()));
            },
        );
    }
}

/// Push menu-open state onto the gameplay input context. Runs in `PreUpdate` and only emits
/// commands when the state actually flips (or once close-time inputs are released).
///
/// This is a `menus → input` bridge: it reads the menu stack to decide whether gameplay input
/// should be live, so it belongs here rather than in either module.
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
        // Delay re-activating GameplayInput until any keys or buttons still held when
        // the menu closed (e.g. Esc or ~) are released, so they cannot immediately fire
        // ReleaseCursor or re-open the menu.
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

/// Hide and grab the OS cursor while gameplay wants it locked, unless a menu is open (then the
/// cursor must be visible to click the menu). A `menus → camera` bridge.
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

/// Keep the skybox centred on the camera so it reads as infinitely far. A `camera → terrain`
/// bridge: the skybox is a terrain asset but its position tracks the camera.
fn sync_skybox_to_camera(
    camera: Query<&Transform, (With<MainCamera>, Without<SkyBox>)>,
    mut skybox: Single<&mut Transform, (With<SkyBox>, Without<MainCamera>)>,
) {
    if let Some(cam_transform) = camera.iter().next() {
        skybox.translation = cam_transform.translation;
    }
}
