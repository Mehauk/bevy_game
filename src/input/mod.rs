//! Input layer: hotkey *contexts* (gameplay vs menu), the derived hotkey registry,
//! keybind remapping, and the `input` Lua namespace.
//!
//! Contexts are driven by a single rule — gameplay input is active while no menu is open,
//! menu input is active while one is — evaluated in [`sync_context_activity`] and pushed
//! onto the entities that own each context.

pub mod hotkeys;
pub mod keys;
pub mod remap;
mod scripting;

pub use hotkeys::HotkeyRegistry;
pub use keys::{current_modifier_flags, modifier_entries, modifier_map, mouse_button_name};
pub use remap::{RemapBindingMessage, RemapStatus};

use bevy::{
    ecs::{relationship::Relationship, system::IntoObserverSystem},
    prelude::*,
};
use bevy_enhanced_input::prelude::*;

use crate::menus::{MenuAppExt, MenuManager, MenuSpec, MenuUpdateSystems};

/// Hotkey action that toggles the keyboard overlay (the rebinding UI).
#[derive(InputAction)]
#[action_output(bool)]
struct ToggleKeyboardOverlay;

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
                remap::handle_remap_bindings.in_set(InputUpdateSystems::ProcessRemap),
            )
            .add_systems(
                Update,
                hotkeys::sync_hotkey_registry.in_set(InputUpdateSystems::SyncHotkeyRegistry),
            );

        scripting::register_input_namespace(app);

        // The keyboard overlay is the front-end of the hotkey registry and remapping
        // defined here, so its declaration lives here. Queued, so `InputPlugin` needs
        // no ordering against `MenuPlugin`.
        app.register_menu::<ToggleKeyboardOverlay>(MenuSpec {
            id: "keyboard",
            hotkey_label: "Keyboard Overlay",
            command: Some(("menu>keyboard", "Toggle virtual keyboard overlay")),
            bindings: vec![
                Binding::from(KeyCode::F1),
                KeyCode::KeyK
                    .with_mod_keys(ModKeys::CONTROL | ModKeys::SHIFT)
                    .into(),
            ],
        });
    }
}

/// Marker: this entity owns the gameplay input context.
#[derive(Component, Default, Clone)]
pub struct GameplayInput;

/// Marker: this entity owns the menu input context.
#[derive(Component, Default, Clone)]
pub struct MenuInput;

/// Marks a binding as a movement key, which the remap rules treat as modifier-free.
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct MovementBinding;

/// The entity that owns the shared gameplay context (camera and menu hotkeys hang off it).
#[derive(Resource)]
pub struct GameplayInputEntity(pub Entity);

/// Human-readable name shown in the keyboard overlay for an action or binding.
#[derive(Component, Clone, Copy, Default)]
pub struct HotkeyLabel(pub &'static str);

/// Mirror of context activity as a component, so it can be queried and observed.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct ContextActive(pub bool);

/// Denormalised context info copied onto each action, so the registry can report
/// `{ context, active }` without walking back to the context entity.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct ActionContext {
    pub name: &'static str,
    pub active: bool,
}

fn setup_gameplay_input(mut commands: Commands) {
    let gameplay_entity = commands
        .spawn((GameplayInput, ContextActive(true), ContextActivity::<GameplayInput>::ACTIVE))
        .id();
    commands.insert_resource(GameplayInputEntity(gameplay_entity));
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

/// Push menu-open state onto the two input contexts. Runs in `PreUpdate` and only emits
/// commands when the state actually flips (or once close-time inputs are released).
fn sync_context_activity(
    menu_manager: Res<MenuManager>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut prev_open: Local<Option<bool>>,
    mut held_keys_on_close: Local<Vec<KeyCode>>,
    mut held_buttons_on_close: Local<Vec<MouseButton>>,
    q_gameplay: Query<Entity, With<GameplayInput>>,
    q_menu: Query<Entity, With<MenuInput>>,
    mut commands: Commands,
) {
    let open = menu_manager.any_open();
    let transitioned = *prev_open != Some(open);
    if transitioned {
        *prev_open = Some(open);
        for entity in &q_menu {
            set_context_active::<MenuInput>(&mut commands, entity, open);
        }
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

/// Spawn `A` on the shared gameplay context with `settings`, plus one binding entity per
/// binding. Shared by both registration entry points below.
fn spawn_gameplay_action<A: InputAction>(
    commands: &mut Commands,
    context: Entity,
    label: &'static str,
    settings: ActionSettings,
    bindings: &[Binding],
) {
    let action = commands
        .spawn((
            Action::<A>::new(),
            ActionOf::<GameplayInput>::new(context),
            settings,
            HotkeyLabel(label),
        ))
        .id();

    for binding in bindings {
        commands.spawn((binding.clone(), BindingOf(action)));
    }
}

pub trait InputAppExt {
    /// Mirror a context entity's [`ContextActive`] onto every action owned by that context.
    fn register_hotkey_context<C: Component>(&mut self, name: &'static str) -> &mut Self;

    /// Register a **value** action (mouse look, scroll zoom): `observer` sees `Fire<A>` and
    /// reads `event.value` for as long as the action is active.
    ///
    /// `settings` is explicit rather than defaulted: `consume_input` decides whether the
    /// binding is hidden from every other context once fired, and `require_reset` decides
    /// whether a held binding re-fires — both change behaviour invisibly if guessed at.
    fn register_gameplay_action<A, B, O, M>(
        &mut self,
        label: &'static str,
        settings: ActionSettings,
        bindings: B,
        observer: O,
    ) -> &mut Self
    where
        A: InputAction + Send + Sync + 'static,
        B: IntoIterator<Item = Binding>,
        O: IntoObserverSystem<Fire<A>, M>;

    /// Register a **one-shot** hotkey (toggle a menu, switch view): `observer` sees
    /// `Start<A>` once per press.
    fn register_gameplay_hotkey<A, B, O, M>(
        &mut self,
        label: &'static str,
        settings: ActionSettings,
        bindings: B,
        observer: O,
    ) -> &mut Self
    where
        A: InputAction + Send + Sync + 'static,
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
        settings: ActionSettings,
        bindings: B,
        observer: O,
    ) -> &mut Self
    where
        A: InputAction + Send + Sync + 'static,
        B: IntoIterator<Item = Binding>,
        O: IntoObserverSystem<Fire<A>, M>,
    {
        register_action::<A, _, Fire<A>, _, _>(self, label, settings, bindings, observer)
    }

    fn register_gameplay_hotkey<A, B, O, M>(
        &mut self,
        label: &'static str,
        settings: ActionSettings,
        bindings: B,
        observer: O,
    ) -> &mut Self
    where
        A: InputAction + Send + Sync + 'static,
        B: IntoIterator<Item = Binding>,
        O: IntoObserverSystem<Start<A>, M>,
    {
        register_action::<A, _, Start<A>, _, _>(self, label, settings, bindings, observer)
    }
}

/// Attach `observer` to `A` and spawn the action on the shared gameplay context at startup.
///
/// `E` is the enhanced-input event the observer listens for (`Fire<A>` for value actions,
/// `Start<A>` for one-shot hotkeys). The two trait entry points pin it explicitly so call
/// sites never have to name it, and so a `Fire` observer can never be registered as a
/// one-shot hotkey or vice versa.
fn register_action<'a, A, B, E, O, M>(
    app: &'a mut App,
    label: &'static str,
    settings: ActionSettings,
    bindings: B,
    observer: O,
) -> &'a mut App
where
    A: InputAction + Send + Sync + 'static,
    B: IntoIterator<Item = Binding>,
    E: Event,
    O: IntoObserverSystem<E, M>,
{
    let bindings: Vec<Binding> = bindings.into_iter().collect();
    app.add_observer(observer).add_systems(
        Startup,
        move |mut commands: Commands, gameplay: Res<GameplayInputEntity>| {
            spawn_gameplay_action::<A>(
                &mut commands,
                gameplay.0,
                label,
                settings,
                &bindings,
            );
        },
    );
    app
}