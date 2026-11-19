//! HTML/CSS menus driven by Lua scripts.
//!
//! A menu is `assets/menus/<id>/{index.html,style.css,script.lua}` (overridable per-file
//! from `mods/`). The HTML is parsed into a DOM of UI entities, the CSS is applied by
//! bevy_flair, and the script drives it through the `dom`, `menus`, `input` and
//! `commands` namespaces while the engine pushes state changes back as callbacks.
//! See `docs/lua-hooks.md` for the script-facing contract.
//!
//! Module map:
//! - [`state`] — the open-menu stack, read by input contexts and cursor locking
//! - [`lifecycle`] — opening/closing menus and the script-ready handshake
//! - [`assets`] — locating menu files (mods override assets) and hot reload
//! - [`dom`] — the Lua-addressable node tree, its op queue, and its event bridge
//! - [`focus`] — keeping keyboard focus on the right text field
//! - [`dispatch`] — pushing modifier/remap changes out to ready scripts
//! - [`scripting`] — the `menus` and `dom` namespaces
//!
//! This module is the menu *engine* only: it knows no concrete menu. Features declare
//! their own menus through [`MenuAppExt::register_menu`] (the command palette is
//! declared by the command console in `utils::registry::commands`, the keyboard overlay
//! by `input`), queued via `utils::registry::menus` so plugin order stays irrelevant.

mod assets;
mod dispatch;
mod dom;
mod focus;
mod lifecycle;
mod scripting;
mod state;

pub use state::MenuManager;

use bevy::prelude::*;
use bevy::text::EditableTextSystems;
use bevy_enhanced_input::prelude::*;
use bevy_mod_scripting::core::{callback_labels, handler::event_handler};
use bevy_mod_scripting::lua::LuaScriptingPlugin;

use crate::input::InputAppExt;
use crate::utils::registry::commands::CommandAppExt;
use crate::utils::registry::menus::{MenuRegistry, MenuRegistryExt};

use dom::apply_dom_ops;
use lifecycle::{dispatch_menu_ready, handle_menu_commands};
use state::{MenuCmd, ToggleMenu};

// Engine → Lua callbacks. The string on the right is the global function name Lua sees.
callback_labels!(
    OnMenuReady => "on_menu_ready",
    OnDomEvent => "on_dom_event",
    OnMenuModifiersChanged => "on_modifiers_changed",
    OnMenuRemapResult => "on_remap_result",
);

/// All menu work happens in one ordered block, after input has been resolved.
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
                    event_handler::<OnMenuReady, LuaScriptingPlugin>,
                    event_handler::<OnDomEvent, LuaScriptingPlugin>,
                    event_handler::<OnMenuModifiersChanged, LuaScriptingPlugin>,
                    event_handler::<OnMenuRemapResult, LuaScriptingPlugin>,
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

        // Register every menu queued by feature plugins until now (the drain is the
        // only place that needs `ToggleMenu` etc. to exist — see `utils::registry::menus`).
        let queued = app
            .world_mut()
            .resource_mut::<MenuRegistry>()
            .take_pending();
        for register in queued {
            register(app);
        }
    }
}

/// How a menu is opened: an optional hotkey and an optional console command.
pub struct MenuSpec {
    pub id: &'static str,
    pub hotkey_label: &'static str,
    /// `(command name, description)` to expose in the command palette, if any.
    pub command: Option<(&'static str, &'static str)>,
    pub bindings: Vec<Binding>,
}

pub trait MenuAppExt {
    /// Declare a menu so a hotkey and/or console command can toggle `menus/<id>/`.
    ///
    /// Safe to call from any plugin's `build()`: the declaration is queued and performed
    /// when `MenuPlugin` builds, so declarers need no ordering against `MenuPlugin`
    /// (see [`crate::utils::registry::menus`]).
    fn register_menu<A: InputAction<Output = bool> + Send + Sync + 'static>(
        &mut self,
        spec: MenuSpec,
    ) -> &mut Self;
}

impl MenuAppExt for App {
    fn register_menu<A: InputAction<Output = bool> + Send + Sync + 'static>(
        &mut self,
        spec: MenuSpec,
    ) -> &mut Self {
        self.queue_menu_registration(move |app| register_menu_with_hotkey::<A>(app, spec))
    }
}

/// Perform a queued declaration: the console command and the consuming hotkey toggle.
fn register_menu_with_hotkey<A: InputAction<Output = bool> + Send + Sync + 'static>(
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