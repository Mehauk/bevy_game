use bevy::{
    ecs::{relationship::Relationship, system::IntoObserverSystem},
    prelude::*,
};
use bevy_enhanced_input::prelude::*;
use bevy_mod_scripting::bindings::{
    InteropError, WorldExtensions,
    function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
    script_value::ScriptValue,
};

use crate::menus::{MenuManager, MenuUpdateSystems};
use crate::utils::{script_map, script_str};

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
            .add_systems(PreStartup, setup_gameplay_input)
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
pub struct MenuInput;

#[derive(Component, Default, Clone, Copy, Debug)]
pub struct MovementBinding;

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

/// Generates `string => KeyCode` arms from bare variant names, so the parse string
/// can never drift from the variant it maps to.
macro_rules! parse_key_variants {
    ($s:expr, $($variant:ident),+ $(,)?) => {
        match $s {
            $(stringify!($variant) => Some(KeyCode::$variant),)+
            _ => None,
        }
    };
}

pub fn parse_key_code(s: &str) -> Option<KeyCode> {
    parse_key_variants!(s,
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM,
        KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
        Escape, Space, Enter, Tab, Backspace, CapsLock,
        ShiftLeft, ShiftRight, ControlLeft, ControlRight,
        AltLeft, AltRight, SuperLeft, SuperRight,
        Backquote, Minus, Equal, BracketLeft, BracketRight, Backslash,
        Semicolon, Quote, Comma, Period, Slash,
        ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    )
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

/// Canonical display name for a mouse button, shared by binding serialization and
/// the menu input dispatcher. `Other(4)`/`Other(5)` are treated as Back/Forward.
pub fn mouse_button_name(button: MouseButton) -> Option<&'static str> {
    match button {
        MouseButton::Left => Some("MouseLeft"),
        MouseButton::Right => Some("MouseRight"),
        MouseButton::Middle => Some("MouseMiddle"),
        MouseButton::Back | MouseButton::Other(4) => Some("MouseBack"),
        MouseButton::Forward | MouseButton::Other(5) => Some("MouseForward"),
        MouseButton::Other(_) => None,
    }
}

pub fn binding_key_and_mods(binding: &Binding) -> Option<(String, ModKeys)> {
    match binding {
        Binding::Keyboard { key, mod_keys } => Some((format!("{key:?}"), *mod_keys)),
        Binding::MouseButton { button, mod_keys } => {
            let s = mouse_button_name(*button)
                .map(str::to_string)
                .unwrap_or_else(|| match *button {
                    MouseButton::Other(n) => format!("MouseButton{n}"),
                    _ => "MouseButton".to_string(),
                });
            Some((s, *mod_keys))
        }
        Binding::MouseWheel { mod_keys } => Some(("MouseWheel".to_string(), *mod_keys)),
        Binding::MouseMotion { mod_keys } => Some(("MouseMotion".to_string(), *mod_keys)),
        _ => None,
    }
}

pub trait InputAppExt {
    fn register_hotkey_context<C: Component>(&mut self, name: &'static str) -> &mut Self;

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

#[derive(Clone)]
struct BindingSnapshot {
    entity: Entity,
    key: String,
    mods: ModKeys,
    context: String,
    is_movement: bool,
    label: Option<String>,
}

impl BindingSnapshot {
    fn display_label(&self) -> String {
        self.label.clone().unwrap_or_else(|| self.key.clone())
    }
}

/// Validate assigning `(key, mods)` to a binding whose action is (or isn't) a movement binding.
/// Movement keys must stay modifier-free and outside any hotkey chain; hotkey chains must not
/// land on a movement key. `chain_exclude` lists bindings allowed to share `key` with modifiers
/// (the pair being swapped).
fn validate_placement(
    snapshots: &[BindingSnapshot],
    key: &str,
    mods: ModKeys,
    is_movement: bool,
    chain_exclude: &[Entity],
) -> Result<(), String> {
    if is_movement {
        if !mods.is_empty() {
            return Err("Movement keys cannot have modifier keys".to_string());
        }
        if let Some(chain_desc) = snapshots.iter().find_map(|s| {
            (!chain_exclude.contains(&s.entity)
                && s.key.eq_ignore_ascii_case(key)
                && !s.mods.is_empty())
                .then(|| format_binding_chain(&s.mods, &s.key, s.label.as_deref()))
        }) {
            return Err(format!(
                "Cannot remap movement key to '{key}': key is part of hotkey chain '{chain_desc}'"
            ));
        }
    } else if !mods.is_empty()
        && snapshots
            .iter()
            .any(|s| s.is_movement && s.key.eq_ignore_ascii_case(key))
    {
        return Err(format!("Cannot bind hotkey chain to movement key '{key}'"));
    }
    Ok(())
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

        let binding_snapshots: Vec<BindingSnapshot> = q_bindings
            .iter()
            .filter_map(|(e, b, b_of, lbl, mov)| {
                let (key, mods) = binding_key_and_mods(b)?;
                let (_, ctx) = q_actions.get(b_of.get()).ok().unwrap_or((None, None));
                Some(BindingSnapshot {
                    entity: e,
                    key,
                    mods,
                    context: ctx.map_or("unknown", |c| c.name).to_string(),
                    is_movement: mov.is_some(),
                    label: lbl.map(|l| l.0.to_string()),
                })
            })
            .collect();

        let find = |key: &str, mods: ModKeys, exclude: Option<Entity>| {
            binding_snapshots.iter().find(|s| {
                Some(s.entity) != exclude
                    && s.key.eq_ignore_ascii_case(key)
                    && s.mods == mods
            })
        };

        let Some(source) = find(&msg.source_key, msg.source_mods, None).cloned() else {
            remap_status.set_error(format!("Source binding '{}' not found", msg.source_key));
            continue;
        };

        if source.context != "gameplay" {
            warn!("Cannot remap non-gameplay binding '{}'", source.key);
            remap_status.set_error("Only gameplay keybinds can be remapped".to_string());
            continue;
        }

        let Some(target_code) = parse_input_code(&msg.target_key) else {
            remap_status.set_error(format!("Unknown key or button '{}'", msg.target_key));
            continue;
        };

        let target = find(&msg.target_key, msg.target_mods, Some(source.entity)).cloned();
        let source_label = source.display_label();

        let result = match target {
            None => validate_placement(
                &binding_snapshots,
                &msg.target_key,
                msg.target_mods,
                source.is_movement,
                &[source.entity],
            )
            .map(|()| {
                commands
                    .entity(source.entity)
                    .insert(target_code.to_binding(msg.target_mods));
                format!("Remapped '{source_label}' to '{}'", msg.target_key)
            }),
            Some(target) => {
                if target.context != "gameplay" {
                    Err(format!(
                        "Cannot swap with non-gameplay keybind '{}'",
                        target.key
                    ))
                } else {
                    let source_code = parse_input_code(&source.key);
                    match source_code {
                        None => Err(format!("Unknown key or button '{}'", source.key)),
                        Some(source_code) => validate_placement(
                            &binding_snapshots,
                            &msg.target_key,
                            msg.target_mods,
                            source.is_movement,
                            &[source.entity, target.entity],
                        )
                        .and_then(|()| {
                            validate_placement(
                                &binding_snapshots,
                                &source.key,
                                source.mods,
                                target.is_movement,
                                &[source.entity, target.entity],
                            )
                        })
                        .map(|()| {
                            commands
                                .entity(source.entity)
                                .insert(target_code.to_binding(target.mods));
                            commands
                                .entity(target.entity)
                                .insert(source_code.to_binding(source.mods));
                            format!(
                                "Swapped '{source_label}' ({}) <-> '{}' ({})",
                                source.key,
                                target.display_label(),
                                target.key
                            )
                        }),
                    }
                }
            }
        };

        match result {
            Ok(message) => {
                info!("{message}");
                remap_status.set_success(message);
            }
            Err(message) => {
                warn!("{message}");
                remap_status.set_error(message);
            }
        }
    }
}

pub fn parse_mod_keys(val: &ScriptValue) -> ModKeys {
    let mut mods = ModKeys::empty();
    if let ScriptValue::Map(map) = val {
        for (name, flag) in [
            ("ctrl", ModKeys::CONTROL),
            ("shift", ModKeys::SHIFT),
            ("alt", ModKeys::ALT),
            ("super", ModKeys::SUPER),
        ] {
            if map.get(name).is_some_and(|v| matches!(v, ScriptValue::Bool(true))) {
                mods |= flag;
            }
        }
    }
    mods
}

pub fn current_modifier_flags(keys: &ButtonInput<KeyCode>) -> ModKeys {
    let mut mods = ModKeys::empty();
    if keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
        mods |= ModKeys::CONTROL;
    }
    if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        mods |= ModKeys::SHIFT;
    }
    if keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]) {
        mods |= ModKeys::ALT;
    }
    if keys.any_pressed([KeyCode::SuperLeft, KeyCode::SuperRight]) {
        mods |= ModKeys::SUPER;
    }
    mods
}

pub fn modifier_map(mods: ModKeys) -> ScriptValue {
    script_map([
        ("ctrl", ScriptValue::Bool(mods.contains(ModKeys::CONTROL))),
        ("shift", ScriptValue::Bool(mods.contains(ModKeys::SHIFT))),
        ("alt", ScriptValue::Bool(mods.contains(ModKeys::ALT))),
        ("super", ScriptValue::Bool(mods.contains(ModKeys::SUPER))),
    ])
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
                            script_map([
                                ("key", script_str(&entry.key)),
                                ("label", script_str(&entry.label)),
                                ("context", script_str(&entry.context)),
                                ("active", ScriptValue::Bool(entry.active)),
                                ("is_movement", ScriptValue::Bool(entry.is_movement)),
                                ("ctrl", ScriptValue::Bool(entry.ctrl)),
                                ("shift", ScriptValue::Bool(entry.shift)),
                                ("alt", ScriptValue::Bool(entry.alt)),
                                ("super", ScriptValue::Bool(entry.super_key)),
                            ])
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
                    modifier_map(current_modifier_flags(keys))
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
                    script_map([
                        ("success", ScriptValue::Bool(status.success)),
                        ("message", script_str(&status.message)),
                        ("version", ScriptValue::Integer(status.version as i64)),
                    ])
                })
            },
        );
}