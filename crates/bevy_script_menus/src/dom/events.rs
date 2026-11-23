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
use crate::OnDomEvent;
use crate::lifecycle::MenuRoot;
use crate::modifiers::modifier_entries;
use crate::script_value::{script_map, script_str};

#[derive(SystemParam)]
pub(crate) struct DomEventCtx<'w, 's> {
    handlers: Query<'w, 's, &'static DomHandlers>,
    parents: Query<'w, 's, &'static ChildOf>,
    roots: Query<'w, 's, &'static MenuRoot>,
    handles: Query<'w, 's, &'static DomHandle>,
    callbacks: MessageWriter<'w, ScriptCallbackMessage>,
}

impl DomEventCtx<'_, '_> {
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
        modifier_entries(&keys).chain([
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

pub(crate) fn strip_menu_nav_text_edits(
    mut q_editable: Query<&mut EditableText, With<MenuTextInput>>,
) {
    for mut editable in &mut q_editable {
        editable
            .pending_edits
            .retain(|edit| !matches!(edit, TextEdit::Up(_) | TextEdit::Down(_)));
    }
}

#[cfg(test)]
mod tests {
    use super::super::node::{NodeSpec, spawn_node_entity};
    use super::*;
    use bevy::camera::NormalizedRenderTarget;
    use bevy::picking::backend::HitData;
    use bevy::picking::events::Pointer;
    use bevy::picking::pointer::{Location, PointerId};
    use bevy::window::WindowRef;
    use bevy_mod_scripting::asset::ScriptAsset;
    use std::time::Duration;

    fn click_app() -> App {
        let mut app = App::new();
        app.add_message::<ScriptCallbackMessage>();
        app.add_observer(on_dom_click);
        app
    }

    fn spawn_menu_root(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                MenuRoot {
                    name: "test".into(),
                    dom_id: 1,
                    script: Some(Handle::<ScriptAsset>::default()),
                    html: None,
                    ready_sent: true,
                    dom_built: true,
                },
                DomHandlers::default(),
                DomHandle(1),
            ))
            .id()
    }

    fn click(world: &mut World, target: Entity, button: PointerButton) {
        world.trigger(PointerClick {
            entity: target,
            pointer: Pointer::new(
                PointerId::Mouse,
                Location {
                    position: Vec2::ZERO,
                    target: NormalizedRenderTarget::Window(
                        WindowRef::Primary
                            .normalize(Some(Entity::PLACEHOLDER))
                            .unwrap(),
                    ),
                },
            ),
            button,
            hit: HitData {
                camera: Entity::PLACEHOLDER,
                depth: 0.0,
                position: None,
                normal: None,
                extra: None,
            },
            duration: Duration::ZERO,
            count: 1,
        });
    }

    fn callback_count(app: &App) -> usize {
        app.world()
            .resource::<Messages<ScriptCallbackMessage>>()
            .len()
    }

    #[test]
    fn a_click_dispatches_its_handler_exactly_once() {
        let mut app = click_app();
        let root = spawn_menu_root(&mut app);
        let node = spawn_node_entity(
            app.world_mut(),
            2,
            root,
            NodeSpec {
                tag: "button".into(),
                id: Some("btn".into()),
                ..default()
            },
        );
        app.world_mut()
            .get_mut::<DomHandlers>(node)
            .unwrap()
            .0
            .insert("click".into(), "on_btn".into());

        click(app.world_mut(), node, PointerButton::Primary);

        assert_eq!(
            callback_count(&app),
            1,
            "one click must produce exactly one script callback"
        );
    }

    #[test]
    fn right_click_on_the_root_reaches_its_handler() {
        let mut app = click_app();
        let root = spawn_menu_root(&mut app);
        app.world_mut()
            .get_mut::<DomHandlers>(root)
            .unwrap()
            .0
            .insert("right_click".into(), "on_right".into());

        click(app.world_mut(), root, PointerButton::Secondary);

        assert_eq!(
            callback_count(&app),
            1,
            "the root entity must receive right_click bindings"
        );
    }
}
