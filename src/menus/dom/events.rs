//! Bridging Bevy UI pointer/keyboard/text events to Lua handlers bound with `dom.on`.
//!
//! Handlers are found by walking *up* the node tree, so binding `click` on a container
//! catches clicks on any descendant — and the nearest binding wins, which is what makes
//! "bind the row, not the row's twelve spans" work.

use bevy::{
    ecs::system::SystemParam,
    input::keyboard::KeyboardInput,
    input::mouse::MouseWheel,
    input_focus::FocusedInput,
    picking::events::{PointerClick, PointerOut, PointerOver},
    picking::hover::HoverMap,
    picking::pointer::PointerId,
    picking::pointer::PointerButton,
    prelude::*,
    text::{EditableText, TextEdit, TextEditChange},
};
use bevy_mod_scripting::{
    bindings::script_value::ScriptValue,
    core::event::{IntoCallbackLabel, ScriptCallbackMessage},
};

use super::node::MenuTextInput;
use super::{DomHandle, DomHandlers};
use crate::input::{current_modifier_flags, modifier_entries};
use crate::menus::OnDomEvent;
use crate::menus::lifecycle::MenuRoot;
use crate::utils::{script_map, script_str};

/// Shared read-only access for the DOM event observers.
#[derive(SystemParam)]
pub(crate) struct DomEventCtx<'w, 's> {
    handlers: Query<'w, 's, &'static DomHandlers>,
    parents: Query<'w, 's, &'static ChildOf>,
    roots: Query<'w, 's, &'static MenuRoot>,
    handles: Query<'w, 's, &'static DomHandle>,
    callbacks: MessageWriter<'w, ScriptCallbackMessage>,
}

impl DomEventCtx<'_, '_> {
    /// Nearest ancestor of `target` (inclusive) that has a handler for `event`, with the handler name.
    fn find_handler(&self, target: Entity, event: &str) -> Option<(Entity, String)> {
        std::iter::once(target)
            .chain(self.parents.iter_ancestors(target))
            .find_map(|e| {
                self.handlers
                    .get(e)
                    .ok()
                    .and_then(|h| h.0.get(event).map(|s| (e, s.clone())))
            })
    }

    fn has_handler(&self, target: Entity, event: &str) -> bool {
        self.find_handler(target, event).is_some()
    }

    /// Dispatch `event` to the owning menu's script; returns whether anything was sent.
    fn emit(&mut self, target: Entity, event: &str, payload: ScriptValue) -> bool {
        let Some((target_entity, handler)) = self.find_handler(target, event) else {
            return false;
        };

        let Some((menu_entity, root)) = std::iter::once(target_entity)
            .chain(self.parents.iter_ancestors(target_entity))
            .find_map(|e| self.roots.get(e).ok().map(|r| (e, r)))
        else {
            return false;
        };
        let Some(script) = root.ready_script() else {
            return false;
        };
        let node_id = self.handles.get(target_entity).map_or(0, |h| h.0);

        self.callbacks
            .write(ScriptCallbackMessage::new_for_script_entity(
                OnDomEvent::into_callback_label(),
                vec![
                    ScriptValue::String(handler.into()),
                    ScriptValue::Integer(node_id),
                    payload,
                ],
                script,
                menu_entity,
            ));
        true
    }
}

pub(crate) fn on_dom_click(mut event: On<PointerClick>, mut ctx: DomEventCtx) {
    // The secondary button is its own event so menus can use right-click as a
    // cancel/dismiss gesture without also triggering whatever `click` is bound to.
    let event_name = match event.button {
        PointerButton::Secondary => "right_click",
        _ => "click",
    };
    if ctx.emit(event.event_target(), event_name, ScriptValue::Unit) {
        event.propagate(false);
    }
}

pub(crate) fn on_dom_hover_enter(event: On<PointerOver>, mut ctx: DomEventCtx) {
    ctx.emit(event.event_target(), "hover_enter", ScriptValue::Unit);
}

pub(crate) fn on_dom_hover_exit(event: On<PointerOut>, mut ctx: DomEventCtx) {
    ctx.emit(event.event_target(), "hover_exit", ScriptValue::Unit);
}

/// Bridge raw mouse-wheel messages to the DOM node the pointer is over as `wheel` events.
///
/// Picking has no wheel event, so we take the hover stack from the last picking update and
/// hand the wheel to the deepest node that bound a `wheel` handler. The UI hit stack is a
/// chain of ancestors, so the deepest bound handler is the one the cursor is really over.
pub(crate) fn dispatch_dom_wheel(
    mut mouse_wheel: MessageReader<MouseWheel>,
    hover_map: Option<Res<HoverMap>>,
    mut ctx: DomEventCtx,
) {
    let mut delta_y = 0.0;
    for ev in mouse_wheel.read() {
        delta_y += ev.y;
    }
    if delta_y == 0.0 {
        return;
    }

    let Some(map) = hover_map else { return };
    let Some(hovered) = map.get(&PointerId::Mouse) else {
        return;
    };

    let mut best: Option<(usize, Entity)> = None;
    for &entity in hovered.keys() {
        let Some((handler_entity, _)) = ctx.find_handler(entity, "wheel") else {
            continue;
        };
        let depth = ctx.parents.iter_ancestors(handler_entity).count();
        if best.is_none_or(|(best_depth, _)| depth > best_depth) {
            best = Some((depth, handler_entity));
        }
    }
    if let Some((_, target)) = best {
        let payload = script_map([("delta_y", ScriptValue::Float(delta_y as f64))]);
        ctx.emit(target, "wheel", payload);
    }
}

pub(crate) fn on_dom_key_input(
    mut event: On<FocusedInput<KeyboardInput>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut ctx: DomEventCtx,
) {
    let target = event.event_target();
    if !event.input.state.is_pressed() || !ctx.has_handler(target, "key_down") {
        return;
    }
    let key = event.input.key_code;
    // Navigation and close keys belong to the menu script, not to text-edit caret movement.
    if matches!(
        key,
        KeyCode::ArrowDown | KeyCode::ArrowUp | KeyCode::Tab | KeyCode::Enter | KeyCode::Escape
    ) {
        event.propagate(false);
        if event.input.repeat && matches!(key, KeyCode::Tab | KeyCode::Enter | KeyCode::Escape) {
            return;
        }
    }

    let payload = script_map(
        modifier_entries(current_modifier_flags(&keys)).chain([
            ("key", script_str(&format!("{key:?}"))),
            ("repeat", ScriptValue::Bool(event.input.repeat)),
        ]),
    );

    ctx.emit(target, "key_down", payload);
}

pub(crate) fn on_dom_text_change(
    event: On<TextEditChange>,
    mut q_inputs: Query<(&EditableText, &mut MenuTextInput)>,
    mut ctx: DomEventCtx,
) {
    let target = event.event_target();
    let Ok((editable, mut last_value)) = q_inputs.get_mut(target) else {
        return;
    };
    let value = editable.value().to_string();
    if last_value.0 != value {
        last_value.0 = value.clone();
        ctx.emit(target, "text_change", ScriptValue::String(value.into()));
    }
}

/// Strip caret up/down edits from menu text fields so arrow keys drive menu selection
/// instead of moving an invisible cursor.
pub(crate) fn strip_menu_nav_text_edits(
    mut q_editable: Query<&mut EditableText, With<MenuTextInput>>,
) {
    for mut editable in &mut q_editable {
        editable
            .pending_edits
            .retain(|edit| !matches!(edit, TextEdit::Up(_) | TextEdit::Down(_)));
    }
}