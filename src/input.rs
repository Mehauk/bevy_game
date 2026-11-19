use std::borrow::Cow;

use bevy::{
    ecs::{relationship::Relationship, system::IntoObserverSystem},
    platform::collections::HashMap,
    prelude::*,
};
use bevy_enhanced_input::prelude::*;
use bevy_mod_scripting::bindings::{
    InteropError, WorldExtensions,
    function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
    script_value::ScriptValue,
};

use crate::menus::{MenuManager, MenuUpdateSystems};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputUpdateSystems {
    SyncActionContexts,
    ApplyContextSync,
    ProcessRemap,
    SyncHotkeyRegistry,
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EnhancedInputPlugin)
            .init_resource::<HotkeyRegistry>()
            .init_resource::<RemapStatus>()
            .add_message::<RemapBindingMessage>()
            .register_hotkey_context::<GlobalInput>("global")
            .register_hotkey_context::<GameplayInput>("gameplay")
            .register_hotkey_context::<MenuInput>("menu")
            .configure_sets(
                Update,
                (
                    InputUpdateSystems::SyncActionContexts,
                    InputUpdateSystems::ApplyContextSync,
                    InputUpdateSystems::ProcessRemap,
                    InputUpdateSystems::SyncHotkeyRegistry,
                )
                    .chain()
                    .before(MenuUpdateSystems),
            )
            .add_systems(
                PreStartup,
                (setup_global_input, setup_gameplay_input),
            )
            .add_systems(
                PreUpdate,
                sync_context_activity.before(EnhancedInputSystems::Update),
            )
            .add_systems(
                Update,
                ApplyDeferred.in_set(InputUpdateSystems::ApplyContextSync),
            )
            .add_systems(
                Update,
                handle_remap_bindings.in_set(InputUpdateSystems::ProcessRemap),
            )
            .add_systems(
                Update,
                sync_hotkey_registry.in_set(InputUpdateSystems::SyncHotkeyRegistry),
            );

        register_input_namespace(app);
    }
}

fn setup_global_input(mut commands: Commands) {
    let global_entity = commands
        .spawn((
            GlobalInput,
            ContextActive(true),
            ContextActivity::<GlobalInput>::ACTIVE,
        ))
        .id();
    commands.insert_resource(GlobalInputEntity(global_entity));
}

fn setup_gameplay_input(mut commands: Commands) {
    let gameplay_entity = commands
        .spawn((
            GameplayInput,
            ContextActive(true),
            ContextActivity::<GameplayInput>::ACTIVE,
        ))
        .id();
    commands.insert_resource(GameplayInputEntity(gameplay_entity));
}

#[derive(Component, Default, Clone)]
pub struct GameplayInput;

#[derive(Component, Default, Clone)]
pub struct GlobalInput;

#[derive(Component, Default, Clone)]
pub struct MenuInput;

#[derive(Component, Default, Clone, Copy, Debug)]
pub struct MovementBinding;

#[derive(Resource)]
pub struct GlobalInputEntity(pub Entity);

#[derive(Resource)]
pub struct GameplayInputEntity(pub Entity);

#[derive(Component, Clone, Copy, Default)]
pub struct HotkeyLabel(pub &'static str);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct ContextActive(pub bool);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct ActionContext {
    pub name: &'static str,
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct HotkeyEntry {
    pub key: String,
    pub label: String,
    pub context: String,
    pub active: bool,
    pub is_movement: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[derive(Resource, Default)]
pub struct HotkeyRegistry {
    pub entries: Vec<HotkeyEntry>,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct RemapStatus {
    pub success: bool,
    pub message: String,
    pub version: u64,
}

impl RemapStatus {
    pub fn set_success(&mut self, message: String) {
        self.success = true;
        self.message = message;
        self.version = self.version.wrapping_add(1);
    }

    pub fn set_error(&mut self, message: String) {
        self.success = false;
        self.message = message;
        self.version = self.version.wrapping_add(1);
    }
}

#[derive(Message, Clone, Debug)]
pub struct RemapBindingMessage {
    pub source_key: String,
    pub source_mods: ModKeys,
    pub target_key: String,
    pub target_mods: ModKeys,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputCode {
    Key(KeyCode),
    Mouse(MouseButton),
    MouseWheel,
    MouseMotion,
}

impl InputCode {
    pub fn to_binding(&self, mod_keys: ModKeys) -> Binding {
        match *self {
            InputCode::Key(key) => Binding::Keyboard { key, mod_keys },
            InputCode::Mouse(button) => Binding::MouseButton { button, mod_keys },
            InputCode::MouseWheel => Binding::MouseWheel { mod_keys },
            InputCode::MouseMotion => Binding::MouseMotion { mod_keys },
        }
    }
}

pub fn parse_input_code(code: &str) -> Option<InputCode> {
    match code {
        "MouseLeft" | "Left" | "MouseButton1" => Some(InputCode::Mouse(MouseButton::Left)),
        "MouseRight" | "Right" | "MouseButton2" => Some(InputCode::Mouse(MouseButton::Right)),
        "MouseMiddle" | "Middle" | "MouseButton3" => Some(InputCode::Mouse(MouseButton::Middle)),
        "MouseBack" | "Back" | "MouseButton4" => Some(InputCode::Mouse(MouseButton::Back)),
        "MouseForward" | "Forward" | "MouseButton5" => Some(InputCode::Mouse(MouseButton::Forward)),
        "MouseWheel" => Some(InputCode::MouseWheel),
        "MouseMotion" => Some(InputCode::MouseMotion),
        key_str => parse_key_code(key_str).map(InputCode::Key),
    }
}

pub fn parse_key_code(s: &str) -> Option<KeyCode> {
    match s {
        "KeyA" => Some(KeyCode::KeyA),
        "KeyB" => Some(KeyCode::KeyB),
        "KeyC" => Some(KeyCode::KeyC),
        "KeyD" => Some(KeyCode::KeyD),
        "KeyE" => Some(KeyCode::KeyE),
        "KeyF" => Some(KeyCode::KeyF),
        "KeyG" => Some(KeyCode::KeyG),
        "KeyH" => Some(KeyCode::KeyH),
        "KeyI" => Some(KeyCode::KeyI),
        "KeyJ" => Some(KeyCode::KeyJ),
        "KeyK" => Some(KeyCode::KeyK),
        "KeyL" => Some(KeyCode::KeyL),
        "KeyM" => Some(KeyCode::KeyM),
        "KeyN" => Some(KeyCode::KeyN),
        "KeyO" => Some(KeyCode::KeyO),
        "KeyP" => Some(KeyCode::KeyP),
        "KeyQ" => Some(KeyCode::KeyQ),
        "KeyR" => Some(KeyCode::KeyR),
        "KeyS" => Some(KeyCode::KeyS),
        "KeyT" => Some(KeyCode::KeyT),
        "KeyU" => Some(KeyCode::KeyU),
        "KeyV" => Some(KeyCode::KeyV),
        "KeyW" => Some(KeyCode::KeyW),
        "KeyX" => Some(KeyCode::KeyX),
        "KeyY" => Some(KeyCode::KeyY),
        "KeyZ" => Some(KeyCode::KeyZ),
        "Digit0" => Some(KeyCode::Digit0),
        "Digit1" => Some(KeyCode::Digit1),
        "Digit2" => Some(KeyCode::Digit2),
        "Digit3" => Some(KeyCode::Digit3),
        "Digit4" => Some(KeyCode::Digit4),
        "Digit5" => Some(KeyCode::Digit5),
        "Digit6" => Some(KeyCode::Digit6),
        "Digit7" => Some(KeyCode::Digit7),
        "Digit8" => Some(KeyCode::Digit8),
        "Digit9" => Some(KeyCode::Digit9),
        "F1" => Some(KeyCode::F1),
        "F2" => Some(KeyCode::F2),
        "F3" => Some(KeyCode::F3),
        "F4" => Some(KeyCode::F4),
        "F5" => Some(KeyCode::F5),
        "F6" => Some(KeyCode::F6),
        "F7" => Some(KeyCode::F7),
        "F8" => Some(KeyCode::F8),
        "F9" => Some(KeyCode::F9),
        "F10" => Some(KeyCode::F10),
        "F11" => Some(KeyCode::F11),
        "F12" => Some(KeyCode::F12),
        "Escape" => Some(KeyCode::Escape),
        "Space" => Some(KeyCode::Space),
        "Enter" => Some(KeyCode::Enter),
        "Tab" => Some(KeyCode::Tab),
        "Backspace" => Some(KeyCode::Backspace),
        "CapsLock" => Some(KeyCode::CapsLock),
        "ShiftLeft" => Some(KeyCode::ShiftLeft),
        "ShiftRight" => Some(KeyCode::ShiftRight),
        "ControlLeft" => Some(KeyCode::ControlLeft),
        "ControlRight" => Some(KeyCode::ControlRight),
        "AltLeft" => Some(KeyCode::AltLeft),
        "AltRight" => Some(KeyCode::AltRight),
        "SuperLeft" => Some(KeyCode::SuperLeft),
        "SuperRight" => Some(KeyCode::SuperRight),
        "Backquote" => Some(KeyCode::Backquote),
        "Minus" => Some(KeyCode::Minus),
        "Equal" => Some(KeyCode::Equal),
        "BracketLeft" => Some(KeyCode::BracketLeft),
        "BracketRight" => Some(KeyCode::BracketRight),
        "Backslash" => Some(KeyCode::Backslash),
        "Semicolon" => Some(KeyCode::Semicolon),
        "Quote" => Some(KeyCode::Quote),
        "Comma" => Some(KeyCode::Comma),
        "Period" => Some(KeyCode::Period),
        "Slash" => Some(KeyCode::Slash),
        "ArrowUp" => Some(KeyCode::ArrowUp),
        "ArrowDown" => Some(KeyCode::ArrowDown),
        "ArrowLeft" => Some(KeyCode::ArrowLeft),
        "ArrowRight" => Some(KeyCode::ArrowRight),
        _ => None,
    }
}

pub fn format_binding_chain(mods: &ModKeys, key_str: &str, label: Option<&str>) -> String {
    let mut parts = Vec::new();
    if mods.contains(ModKeys::CONTROL) {
        parts.push("Ctrl");
    }
    if mods.contains(ModKeys::SHIFT) {
        parts.push("Shift");
    }
    if mods.contains(ModKeys::ALT) {
        parts.push("Alt");
    }
    if mods.contains(ModKeys::SUPER) {
        parts.push("Super");
    }
    parts.push(key_str);
    let chain = parts.join(" + ");
    if let Some(lbl) = label {
        format!("{chain} ({lbl})")
    } else {
        chain
    }
}

pub fn binding_key_and_mods(binding: &Binding) -> Option<(String, ModKeys)> {
    match binding {
        Binding::Keyboard { key, mod_keys } => Some((format!("{key:?}"), *mod_keys)),
        Binding::MouseButton { button, mod_keys } => {
            let s = match button {
                MouseButton::Left => "MouseLeft".to_string(),
                MouseButton::Right => "MouseRight".to_string(),
                MouseButton::Middle => "MouseMiddle".to_string(),
                MouseButton::Back | MouseButton::Other(4) => "MouseBack".to_string(),
                MouseButton::Forward | MouseButton::Other(5) => "MouseForward".to_string(),
                MouseButton::Other(n) => format!("MouseButton{n}"),
            };
            Some((s, *mod_keys))
        }
        Binding::MouseWheel { mod_keys } => Some(("MouseWheel".to_string(), *mod_keys)),
        Binding::MouseMotion { mod_keys } => Some(("MouseMotion".to_string(), *mod_keys)),
        _ => None,
    }
}

pub trait InputAppExt {
    fn register_hotkey_context<C: Component>(&mut self, name: &'static str) -> &mut Self;

    fn register_global_action<A, B, O, M>(
        &mut self,
        label: &'static str,
        bindings: B,
        observer: O,
    ) -> &mut Self
    where
        A: InputAction<Output = bool> + Send + Sync + 'static,
        B: IntoIterator<Item = Binding>,
        O: IntoObserverSystem<Start<A>, M>;

    fn register_gameplay_action<A, B, O, M>(
        &mut self,
        label: &'static str,
        bindings: B,
        observer: O,
    ) -> &mut Self
    where
        A: InputAction<Output = bool> + Send + Sync + 'static,
        B: IntoIterator<Item = Binding>,
        O: IntoObserverSystem<Start<A>, M>;
}

impl InputAppExt for App {
    fn register_hotkey_context<C: Component>(&mut self, name: &'static str) -> &mut Self {
        self.add_input_context::<C>().add_systems(
            Update,
            (move |q_changed_ctx: Query<(), (With<C>, Changed<ContextActive>)>,
                   q_ctx: Query<Option<&ContextActive>, With<C>>,
                   q_new_actions: Query<(), Added<ActionOf<C>>>,
                   q_actions: Query<(Entity, &ActionOf<C>)>,
                   mut commands: Commands| {
                if q_changed_ctx.is_empty() && q_new_actions.is_empty() {
                    return;
                }
                for (action_entity, action_of) in &q_actions {
                    let active = q_ctx
                        .get(action_of.get())
                        .ok()
                        .flatten()
                        .is_none_or(|c| c.0);
                    commands
                        .entity(action_entity)
                        .insert(ActionContext { name, active });
                }
            })
            .in_set(InputUpdateSystems::SyncActionContexts),
        )
    }

    fn register_global_action<A, B, O, M>(
        &mut self,
        label: &'static str,
        bindings: B,
        observer: O,
    ) -> &mut Self
    where
        A: InputAction<Output = bool> + Send + Sync + 'static,
        B: IntoIterator<Item = Binding>,
        O: IntoObserverSystem<Start<A>, M>,
    {
        let bindings: Vec<Binding> = bindings.into_iter().collect();
        self.add_observer(observer).add_systems(
            Startup,
            move |mut commands: Commands, global: Res<GlobalInputEntity>| {
                let action = commands
                    .spawn((
                        Action::<A>::new(),
                        ActionOf::<GlobalInput>::new(global.0),
                        ActionSettings {
                            consume_input: true,
                            require_reset: true,
                            ..default()
                        },
                        HotkeyLabel(label),
                    ))
                    .id();

                for binding in &bindings {
                    commands.spawn((binding.clone(), BindingOf(action)));
                }
            },
        )
    }

    fn register_gameplay_action<A, B, O, M>(
        &mut self,
        label: &'static str,
        bindings: B,
        observer: O,
    ) -> &mut Self
    where
        A: InputAction<Output = bool> + Send + Sync + 'static,
        B: IntoIterator<Item = Binding>,
        O: IntoObserverSystem<Start<A>, M>,
    {
        let bindings: Vec<Binding> = bindings.into_iter().collect();
        self.add_observer(observer).add_systems(
            Startup,
            move |mut commands: Commands, gameplay: Res<GameplayInputEntity>| {
                let action = commands
                    .spawn((
                        Action::<A>::new(),
                        ActionOf::<GameplayInput>::new(gameplay.0),
                        ActionSettings {
                            consume_input: true,
                            require_reset: true,
                            ..default()
                        },
                        HotkeyLabel(label),
                    ))
                    .id();

                for binding in &bindings {
                    commands.spawn((binding.clone(), BindingOf(action)));
                }
            },
        )
    }
}

pub fn set_context_active<C: Component>(commands: &mut Commands, entity: Entity, active: bool) {
    let activity = if active {
        ContextActivity::<C>::ACTIVE
    } else {
        ContextActivity::<C>::INACTIVE
    };
    commands
        .entity(entity)
        .insert((activity, ContextActive(active)));
}

fn sync_context_activity(
    menu_manager: Res<MenuManager>,
    mut prev_open: Local<Option<bool>>,
    q_gameplay: Query<Entity, With<GameplayInput>>,
    q_menu: Query<Entity, With<MenuInput>>,
    mut commands: Commands,
) {
    let open = menu_manager.any_open();
    if *prev_open == Some(open) {
        return;
    }
    *prev_open = Some(open);

    for entity in &q_gameplay {
        set_context_active::<GameplayInput>(&mut commands, entity, !open);
    }
    for entity in &q_menu {
        set_context_active::<MenuInput>(&mut commands, entity, open);
    }
}

fn sync_hotkey_registry(
    q_changed: Query<
        (),
        Or<(
            Added<Binding>,
            Changed<Binding>,
            Changed<BindingOf>,
            Added<HotkeyLabel>,
            Changed<HotkeyLabel>,
            Added<ActionContext>,
            Changed<ActionContext>,
            Added<MovementBinding>,
        )>,
    >,
    mut removed_bindings: RemovedComponents<Binding>,
    mut removed_labels: RemovedComponents<HotkeyLabel>,
    q_bindings: Query<(
        &Binding,
        &BindingOf,
        Option<&HotkeyLabel>,
        Option<&MovementBinding>,
    )>,
    q_actions: Query<(Option<&HotkeyLabel>, Option<&ActionContext>)>,
    mut registry: ResMut<HotkeyRegistry>,
) {
    if q_changed.is_empty()
        && removed_bindings.read().next().is_none()
        && removed_labels.read().next().is_none()
    {
        return;
    }

    let mut entries = Vec::new();
    for (binding, binding_of, binding_label, movement) in &q_bindings {
        let Some((key_str, mod_keys)) = binding_key_and_mods(binding) else {
            continue;
        };

        let (action_label, action_ctx) = q_actions
            .get(binding_of.get())
            .ok()
            .unwrap_or((None, None));
        let label = binding_label
            .or(action_label)
            .map(|l| l.0)
            .unwrap_or("Bound");
        let (context, active) = action_ctx.map_or(("unknown", true), |c| (c.name, c.active));

        entries.push(HotkeyEntry {
            key: key_str,
            label: label.to_string(),
            context: context.to_string(),
            active,
            is_movement: movement.is_some(),
            ctrl: mod_keys.contains(ModKeys::CONTROL),
            shift: mod_keys.contains(ModKeys::SHIFT),
            alt: mod_keys.contains(ModKeys::ALT),
            super_key: mod_keys.contains(ModKeys::SUPER),
        });
    }
    registry.entries = entries;
}

fn handle_remap_bindings(
    mut messages: MessageReader<RemapBindingMessage>,
    q_bindings: Query<(
        Entity,
        &Binding,
        &BindingOf,
        Option<&HotkeyLabel>,
        Option<&MovementBinding>,
    )>,
    q_actions: Query<(Option<&HotkeyLabel>, Option<&ActionContext>)>,
    mut commands: Commands,
    mut remap_status: ResMut<RemapStatus>,
) {
    for msg in messages.read() {
        if msg.source_key.eq_ignore_ascii_case("MouseMotion")
            || msg.target_key.eq_ignore_ascii_case("MouseMotion")
        {
            warn!("Aim/Look cannot be rebound");
            remap_status.set_error("Aim/Look cannot be rebound".to_string());
            continue;
        }
        let binding_snapshots: Vec<(Entity, String, ModKeys, String, bool, Option<String>)> =
            q_bindings
                .iter()
                .filter_map(|(e, b, b_of, lbl, mov)| {
                    let (k, m) = binding_key_and_mods(b)?;
                    let (_, ctx) = q_actions.get(b_of.get()).ok().unwrap_or((None, None));
                    let ctx_name = ctx.map_or("unknown", |c| c.name).to_string();
                    let label = lbl.map(|l| l.0.to_string());
                    Some((e, k, m, ctx_name, mov.is_some(), label))
                })
                .collect();

        let source_entry = binding_snapshots.iter().find(|(_, k, m, _, _, _)| {
            k.eq_ignore_ascii_case(&msg.source_key) && *m == msg.source_mods
        });

        let Some((source_entity, source_key, source_mods, source_ctx, source_is_movement, source_label)) =
            source_entry.cloned()
        else {
            remap_status.set_error(format!("Source binding '{}' not found", msg.source_key));
            continue;
        };

        if source_ctx != "gameplay" {
            warn!("Cannot remap non-gameplay binding '{source_key}' ({source_ctx})");
            remap_status.set_error("Only gameplay keybinds can be remapped".to_string());
            continue;
        }

        let Some(target_code) = parse_input_code(&msg.target_key) else {
            remap_status.set_error(format!("Unknown key or button '{}'", msg.target_key));
            continue;
        };

        let target_entry = binding_snapshots.iter().find(|(e, k, m, _, _, _)| {
            *e != source_entity
                && k.eq_ignore_ascii_case(&msg.target_key)
                && *m == msg.target_mods
        });

        let source_display_label = source_label.unwrap_or_else(|| source_key.clone());

        match target_entry.cloned() {
            None => {
                if source_is_movement {
                    if !msg.target_mods.is_empty() {
                        warn!("Movement keys cannot be part of any hotkey chain (cannot have modifiers)");
                        remap_status.set_error("Movement keys cannot have modifier keys".to_string());
                        continue;
                    }

                    let chain = binding_snapshots.iter().find_map(|(e, k, m, _, _, lbl)| {
                        if *e != source_entity
                            && k.eq_ignore_ascii_case(&msg.target_key)
                            && !m.is_empty()
                        {
                            Some(format_binding_chain(m, k, lbl.as_deref()))
                        } else {
                            None
                        }
                    });

                    if let Some(chain_desc) = chain {
                        warn!(
                            "Cannot remap movement key to '{}': key is already part of hotkey chain '{}'",
                            msg.target_key, chain_desc
                        );
                        remap_status.set_error(format!(
                            "Cannot remap movement key to '{}': key is part of hotkey chain '{}'",
                            msg.target_key, chain_desc
                        ));
                        continue;
                    }
                } else if !msg.target_mods.is_empty() {
                    let is_movement_key = binding_snapshots.iter().any(|(e, k, _, _, is_mov, _)| {
                        *e != source_entity && *is_mov && k.eq_ignore_ascii_case(&msg.target_key)
                    });

                    if is_movement_key {
                        warn!("Cannot bind hotkey chain to movement key '{}'", msg.target_key);
                        remap_status.set_error(format!(
                            "Cannot bind hotkey chain to movement key '{}'",
                            msg.target_key
                        ));
                        continue;
                    }
                }

                commands
                    .entity(source_entity)
                    .insert(target_code.to_binding(msg.target_mods));

                let success_msg = format!(
                    "Remapped '{}' to '{}'",
                    source_display_label, msg.target_key
                );
                info!("{success_msg}");
                remap_status.set_success(success_msg);
            }
            Some((target_entity, target_key, target_mods, target_ctx, target_is_movement, target_label)) => {
                if target_ctx != "gameplay" {
                    warn!(
                        "Cannot swap with non-gameplay binding '{}' ({target_ctx})",
                        target_key
                    );
                    remap_status.set_error(format!(
                        "Cannot swap with non-gameplay keybind '{}'",
                        target_key
                    ));
                    continue;
                }

                let Some(source_code) = parse_input_code(&source_key) else {
                    remap_status.set_error(format!("Unknown key or button '{}'", source_key));
                    continue;
                };

                let target_display_label = target_label.unwrap_or_else(|| target_key.clone());

                if source_is_movement {
                    if !msg.target_mods.is_empty() {
                        warn!("Movement keys cannot be part of any hotkey chain");
                        remap_status.set_error("Movement keys cannot have modifier keys".to_string());
                        continue;
                    }

                    let chain = binding_snapshots.iter().find_map(|(e, k, m, _, _, lbl)| {
                        if *e != source_entity
                            && *e != target_entity
                            && k.eq_ignore_ascii_case(&msg.target_key)
                            && !m.is_empty()
                        {
                            Some(format_binding_chain(m, k, lbl.as_deref()))
                        } else {
                            None
                        }
                    });

                    if let Some(chain_desc) = chain {
                        warn!(
                            "Cannot remap movement key to '{}': key is already part of hotkey chain '{}'",
                            msg.target_key, chain_desc
                        );
                        remap_status.set_error(format!(
                            "Cannot remap movement key to '{}': key is part of hotkey chain '{}'",
                            msg.target_key, chain_desc
                        ));
                        continue;
                    }
                }

                if target_is_movement {
                    if !source_mods.is_empty() {
                        warn!("Movement keys cannot be part of any hotkey chain");
                        remap_status.set_error("Movement keys cannot have modifier keys".to_string());
                        continue;
                    }

                    let chain = binding_snapshots.iter().find_map(|(e, k, m, _, _, lbl)| {
                        if *e != source_entity
                            && *e != target_entity
                            && k.eq_ignore_ascii_case(&source_key)
                            && !m.is_empty()
                        {
                            Some(format_binding_chain(m, k, lbl.as_deref()))
                        } else {
                            None
                        }
                    });

                    if let Some(chain_desc) = chain {
                        warn!(
                            "Cannot remap movement key to '{}': key is already part of hotkey chain '{}'",
                            source_key, chain_desc
                        );
                        remap_status.set_error(format!(
                            "Cannot remap movement key to '{}': key is part of hotkey chain '{}'",
                            source_key, chain_desc
                        ));
                        continue;
                    }
                }

                if !source_is_movement && !msg.target_mods.is_empty() && target_is_movement {
                    warn!("Cannot bind hotkey chain to movement key '{}'", msg.target_key);
                    remap_status.set_error(format!(
                        "Cannot bind hotkey chain to movement key '{}'",
                        msg.target_key
                    ));
                    continue;
                }

                if !target_is_movement && !source_mods.is_empty() && source_is_movement {
                    warn!("Cannot bind hotkey chain to movement key '{}'", source_key);
                    remap_status.set_error(format!(
                        "Cannot bind hotkey chain to movement key '{}'",
                        source_key
                    ));
                    continue;
                }

                commands
                    .entity(source_entity)
                    .insert(target_code.to_binding(target_mods));
                commands
                    .entity(target_entity)
                    .insert(source_code.to_binding(source_mods));

                let success_msg = format!(
                    "Swapped '{}' ({source_key}) <-> '{}' ({target_key})",
                    source_display_label, target_display_label
                );
                info!("{success_msg}");
                remap_status.set_success(success_msg);
            }
        }
    }
}

pub fn parse_mod_keys(val: &ScriptValue) -> ModKeys {
    let mut mods = ModKeys::empty();
    if let ScriptValue::Map(map) = val {
        if map
            .get("ctrl")
            .and_then(|v| match v {
                ScriptValue::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false)
        {
            mods |= ModKeys::CONTROL;
        }
        if map
            .get("shift")
            .and_then(|v| match v {
                ScriptValue::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false)
        {
            mods |= ModKeys::SHIFT;
        }
        if map
            .get("alt")
            .and_then(|v| match v {
                ScriptValue::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false)
        {
            mods |= ModKeys::ALT;
        }
        if map
            .get("super")
            .and_then(|v| match v {
                ScriptValue::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false)
        {
            mods |= ModKeys::SUPER;
        }
    }
    mods
}

pub fn current_modifier_flags(keys: &ButtonInput<KeyCode>) -> (bool, bool, bool, bool) {
    (
        keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
        keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
        keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]),
        keys.any_pressed([KeyCode::SuperLeft, KeyCode::SuperRight]),
    )
}

pub fn modifier_map(ctrl: bool, shift: bool, alt: bool, super_key: bool) -> ScriptValue {
    let mut map = HashMap::default();
    map.insert("ctrl".to_string(), ScriptValue::Bool(ctrl));
    map.insert("shift".to_string(), ScriptValue::Bool(shift));
    map.insert("alt".to_string(), ScriptValue::Bool(alt));
    map.insert("super".to_string(), ScriptValue::Bool(super_key));
    ScriptValue::Map(map)
}

fn register_input_namespace(app: &mut App) {
    NamespaceBuilder::in_namespace(app.world_mut(), "input")
        .register(
            "list_bindings",
            |ctx: FunctionCallContext| -> Result<ScriptValue, InteropError> {
                ctx.world()?.with_resource(|reg: &HotkeyRegistry| {
                    let list = reg
                        .entries
                        .iter()
                        .map(|entry| {
                            let mut map = HashMap::default();
                            map.insert(
                                "key".to_string(),
                                ScriptValue::String(Cow::Owned(entry.key.clone())),
                            );
                            map.insert(
                                "label".to_string(),
                                ScriptValue::String(Cow::Owned(entry.label.clone())),
                            );
                            map.insert(
                                "context".to_string(),
                                ScriptValue::String(Cow::Owned(entry.context.clone())),
                            );
                            map.insert("active".to_string(), ScriptValue::Bool(entry.active));
                            map.insert("is_movement".to_string(), ScriptValue::Bool(entry.is_movement));
                            map.insert("ctrl".to_string(), ScriptValue::Bool(entry.ctrl));
                            map.insert("shift".to_string(), ScriptValue::Bool(entry.shift));
                            map.insert("alt".to_string(), ScriptValue::Bool(entry.alt));
                            map.insert("super".to_string(), ScriptValue::Bool(entry.super_key));
                            ScriptValue::Map(map)
                        })
                        .collect();
                    ScriptValue::List(list)
                })
            },
        )
        .register(
            "active_modifiers",
            |ctx: FunctionCallContext| -> Result<ScriptValue, InteropError> {
                ctx.world()?.with_resource(|keys: &ButtonInput<KeyCode>| {
                    let (ctrl, shift, alt, super_key) = current_modifier_flags(keys);
                    modifier_map(ctrl, shift, alt, super_key)
                })
            },
        )
        .register(
            "remap",
            |ctx: FunctionCallContext,
             source_key: String,
             source_mods: ScriptValue,
             target_key: String,
             target_mods: ScriptValue|
             -> Result<(), InteropError> {
                let source_m = parse_mod_keys(&source_mods);
                let target_m = parse_mod_keys(&target_mods);
                let _ = ctx.world()?.with_resource_mut(|mut writer: Mut<Messages<RemapBindingMessage>>| {
                    writer.write(RemapBindingMessage {
                        source_key,
                        source_mods: source_m,
                        target_key,
                        target_mods: target_m,
                    });
                });
                Ok(())
            },
        )
        .register(
            "remap_status",
            |ctx: FunctionCallContext| -> Result<ScriptValue, InteropError> {
                ctx.world()?.with_resource(|status: &RemapStatus| {
                    let mut map = HashMap::default();
                    map.insert("success".to_string(), ScriptValue::Bool(status.success));
                    map.insert(
                        "message".to_string(),
                        ScriptValue::String(Cow::Owned(status.message.clone())),
                    );
                    map.insert(
                        "version".to_string(),
                        ScriptValue::Integer(status.version as i64),
                    );
                    ScriptValue::Map(map)
                })
            },
        );
}