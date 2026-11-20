mod assets;
mod dispatch;
mod dom;
mod focus;
mod lifecycle;
mod scripting;
mod state;

pub use state::{MenuManager, ToggleMenu};

use bevy::prelude::*;
use bevy::text::EditableTextSystems;
use bevy_mod_scripting::core::{callback_labels, handler::event_handler};
use bevy_mod_scripting::lua::LuaScriptingPlugin;

use dom::apply_dom_ops;
use lifecycle::{dispatch_menu_ready, handle_menu_commands};
use state::MenuCmd;

callback_labels!(
    OnMenuReady => "on_menu_ready",
    OnDomEvent => "on_dom_event",
    OnMenuModifiersChanged => "on_modifiers_changed",
    OnMenuRemapResult => "on_remap_result",
    OnHistoryChanged => "on_history_changed",
);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuUpdateSystems;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuManager>()
            .init_resource::<dom::DomRuntime>()
            .init_resource::<assets::HtmlHotReloadState>()
            .init_resource::<focus::MenuFocusState>()
            .init_resource::<dispatch::ModifierTracking>()
            .init_resource::<dispatch::HistoryTracking>()
            .add_message::<ToggleMenu>()
            .add_message::<MenuCmd>()
            .add_observer(dom::events::on_dom_click)
            .add_observer(dom::events::on_dom_hover_enter)
            .add_observer(dom::events::on_dom_hover_exit)
            .add_systems(
                Update,
                (
                    assets::reload_menus_on_asset_change,
                    handle_menu_commands,
                    focus::track_and_maintain_menu_focus,
                    dispatch_menu_ready,
                    dispatch::dispatch_modifier_changes,
                    dispatch::dispatch_remap_results,
                    dispatch::dispatch_history_changes,
                    dom::events::dispatch_dom_wheel,
                    event_handler::<OnMenuReady, LuaScriptingPlugin>,
                    event_handler::<OnDomEvent, LuaScriptingPlugin>,
                    event_handler::<OnMenuModifiersChanged, LuaScriptingPlugin>,
                    event_handler::<OnMenuRemapResult, LuaScriptingPlugin>,
                    event_handler::<OnHistoryChanged, LuaScriptingPlugin>,
                    apply_dom_ops,
                )
                    .chain()
                    .in_set(MenuUpdateSystems),
            )
            .add_systems(
                PostUpdate,
                dom::events::strip_menu_nav_text_edits.before(EditableTextSystems),
            );

        scripting::register_menu_namespaces(app);
    }
}
