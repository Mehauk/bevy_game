pub mod hotkeys;
pub mod remap;
mod scripting;

use crate::utils::RemapStatus;

pub use hotkeys::HotkeyRegistry;
pub use remap::RemapBindingMessage;

use bevy::{
    ecs::{relationship::Relationship, system::IntoObserverSystem},
    prelude::*,
};
use bevy_enhanced_input::prelude::*;

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
            .configure_sets(
                Update,
                (
                    InputUpdateSystems::SyncActionContexts,
                    InputUpdateSystems::ApplyContextSync,
                    InputUpdateSystems::ProcessRemap,
                    InputUpdateSystems::SyncHotkeyRegistry,
                )
                    .chain(),
            )
            .add_systems(PreStartup, setup_gameplay_input)
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
    }
}

#[derive(Component, Default, Clone)]
pub struct GameplayInput;

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

fn setup_gameplay_input(mut commands: Commands) {
    let gameplay_entity = commands
        .spawn((GameplayInput, ContextActive(true), ContextActivity::<GameplayInput>::ACTIVE))
        .id();
    commands.insert_resource(GameplayInputEntity(gameplay_entity));
}

pub(crate) fn set_context_active<C: Component>(
    commands: &mut Commands,
    entity: Entity,
    active: bool,
) {
    let activity = if active {
        ContextActivity::<C>::ACTIVE
    } else {
        ContextActivity::<C>::INACTIVE
    };
    commands
        .entity(entity)
        .insert((activity, ContextActive(active)));
}

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
    fn register_hotkey_context<C: Component>(&mut self, name: &'static str) -> &mut Self;

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
