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
    SyncHotkeyRegistry,
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EnhancedInputPlugin)
            .init_resource::<HotkeyRegistry>()
            .register_hotkey_context::<GlobalInput>("global")
            .register_hotkey_context::<GameplayInput>("gameplay")
            .register_hotkey_context::<MenuInput>("menu")
            .configure_sets(
                Update,
                (
                    InputUpdateSystems::SyncActionContexts,
                    InputUpdateSystems::ApplyContextSync,
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
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[derive(Resource, Default)]
pub struct HotkeyRegistry {
    pub entries: Vec<HotkeyEntry>,
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
        )>,
    >,
    mut removed_bindings: RemovedComponents<Binding>,
    mut removed_labels: RemovedComponents<HotkeyLabel>,
    q_bindings: Query<(&Binding, &BindingOf, Option<&HotkeyLabel>)>,
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
    for (binding, binding_of, binding_label) in &q_bindings {
        let (key_str, mod_keys) = match *binding {
            Binding::Keyboard { key, mod_keys } => (format!("{key:?}"), mod_keys),
            Binding::MouseButton { button, mod_keys } => {
                let s = match button {
                    MouseButton::Left => "MouseLeft".to_string(),
                    MouseButton::Right => "MouseRight".to_string(),
                    MouseButton::Middle => "MouseMiddle".to_string(),
                    MouseButton::Back => "MouseBack".to_string(),
                    MouseButton::Forward => "MouseForward".to_string(),
                    MouseButton::Other(4) => "MouseBack".to_string(),
                    MouseButton::Other(5) => "MouseForward".to_string(),
                    MouseButton::Other(n) => format!("MouseButton{n}"),
                    _ => format!("{button:?}"),
                };
                (s, mod_keys)
            }
            Binding::MouseWheel { mod_keys } => ("MouseWheel".to_string(), mod_keys),
            Binding::MouseMotion { mod_keys } => ("MouseMotion".to_string(), mod_keys),
            _ => continue,
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
            ctrl: mod_keys.contains(ModKeys::CONTROL),
            shift: mod_keys.contains(ModKeys::SHIFT),
            alt: mod_keys.contains(ModKeys::ALT),
            super_key: mod_keys.contains(ModKeys::SUPER),
        });
    }
    registry.entries = entries;
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
        );
}