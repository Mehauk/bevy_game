mod assets;
mod config;
mod dom;
mod focus;
mod lifecycle;
mod modifiers;
mod script_value;
mod scripting;
mod state;

pub use config::{MenuAssetRoot, MenuConfig};
pub use lifecycle::{MenuRoot, broadcast_menu_callback};
pub use state::{MenuCmd, MenuManager, ToggleMenu};

use bevy::prelude::*;
use bevy::text::EditableTextSystems;
use bevy_flair::core::PropertyRegistryPlugin;
use bevy_flair::prelude::FlairPlugin;
use bevy_mod_scripting::BMSPlugin;
use bevy_mod_scripting::core::{BMSScriptingInfrastructurePlugin, callback_labels, handler::event_handler};
use bevy_mod_scripting::lua::LuaScriptingPlugin;

use assets::{HtmlHotReloadState, reload_menus_on_asset_change};
use dom::apply_dom_ops;
use lifecycle::{dispatch_menu_ready, handle_menu_commands};

pub type MenuScriptingPlugin = LuaScriptingPlugin;

callback_labels!(
    OnMenuReady => "on_menu_ready",
    OnDomEvent => "on_dom_event",
);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuUpdateSystems;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MenuSystems {
    Lifecycle,
    DispatchEvents,
    UserCallbacks,
    ApplyDom,
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        install_scripting_backends(app);

        app.init_resource::<MenuConfig>()
            .init_resource::<MenuManager>()
            .init_resource::<dom::DomRuntime>()
            .init_resource::<HtmlHotReloadState>()
            .init_resource::<focus::MenuFocusState>()
            .add_message::<ToggleMenu>()
            .add_message::<MenuCmd>()
            .add_observer(dom::events::on_dom_click)
            .add_observer(dom::events::on_dom_hover_enter)
            .add_observer(dom::events::on_dom_hover_exit)
            .configure_sets(
                Update,
                (
                    MenuSystems::Lifecycle,
                    MenuSystems::DispatchEvents,
                    MenuSystems::UserCallbacks,
                    MenuSystems::ApplyDom,
                )
                    .chain()
                    .in_set(MenuUpdateSystems),
            )
            .add_systems(
                Update,
                (
                    reload_menus_on_asset_change,
                    handle_menu_commands,
                    focus::track_and_maintain_menu_focus,
                    dispatch_menu_ready,
                )
                    .chain()
                    .in_set(MenuSystems::Lifecycle),
            )
            .add_systems(
                Update,
                (
                    dom::events::dispatch_dom_wheel,
                    event_handler::<OnMenuReady, MenuScriptingPlugin>,
                    event_handler::<OnDomEvent, MenuScriptingPlugin>,
                )
                    .chain()
                    .in_set(MenuSystems::DispatchEvents),
            )
            .add_systems(Update, apply_dom_ops.in_set(MenuSystems::ApplyDom))
            .add_systems(
                PostUpdate,
                dom::events::strip_menu_nav_text_edits.before(EditableTextSystems),
            );

        scripting::register_menu_namespaces(app);
    }
}

fn install_scripting_backends(app: &mut App) {
    if !app.is_plugin_added::<PropertyRegistryPlugin>() {
        app.add_plugins(FlairPlugin);
    }
    if !app.is_plugin_added::<BMSScriptingInfrastructurePlugin>() {
        app.add_plugins(BMSPlugin);
    }
}
