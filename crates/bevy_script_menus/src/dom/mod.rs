pub mod events;
pub mod node;

use bevy::{
    input_focus::{FocusCause, InputFocus},
    platform::collections::HashMap,
    prelude::*,
    text::{EditableText, TextEdit},
};
use bevy_flair::prelude::*;

use crate::focus::MenuFocusState;
use node::{NodeSpec, spawn_node_entity};

#[derive(Component)]
pub(crate) struct DomHandle(pub i64);

#[derive(Component, Default)]
pub(crate) struct DomHandlers(pub HashMap<String, String>);

#[derive(Debug)]
pub(crate) enum DomOp {
    Create {
        id: i64,
        parent_id: i64,
        spec: NodeSpec,
    },
    Clear(i64),
    Remove(i64),
    SetText {
        id: i64,
        text: String,
    },
    SetValue {
        id: i64,
        text: String,
    },
    SetClass {
        id: i64,
        cls: String,
        add: bool,
    },
    BindEvent {
        id: i64,
        event: String,
        handler: String,
    },
    Focus(i64),
    Blur,
}

pub(crate) struct DomNode {
    pub entity: Entity,
    pub menu_root: Entity,
    pub html_id: Option<String>,
    pub classes: Vec<String>,
}

#[derive(Resource, Default)]
pub(crate) struct DomRuntime {
    next_id: i64,
    pub nodes: HashMap<i64, DomNode>,
    pub ops: Vec<DomOp>,
}

impl DomRuntime {
    pub(crate) fn alloc_node(
        &mut self,
        entity: Entity,
        menu_root: Entity,
        html_id: Option<String>,
        classes: Vec<String>,
    ) -> i64 {
        self.next_id += 1;
        let id = self.next_id;
        self.nodes.insert(
            id,
            DomNode {
                entity,
                menu_root,
                html_id,
                classes,
            },
        );
        id
    }

    pub(crate) fn queue_node(&mut self, menu_root: Entity, parent_id: i64, spec: NodeSpec) -> i64 {
        let id = self.alloc_node(
            Entity::PLACEHOLDER,
            menu_root,
            spec.id.clone(),
            spec.classes.clone(),
        );
        self.ops.push(DomOp::Create {
            id,
            parent_id,
            spec,
        });
        id
    }

    pub(crate) fn find_by_html_id(&self, scope_id: i64, target_id: &str) -> Option<i64> {
        let menu_root = self.nodes.get(&scope_id)?.menu_root;
        self.nodes.iter().find_map(|(&id, n)| {
            (n.menu_root == menu_root && n.html_id.as_deref() == Some(target_id)).then_some(id)
        })
    }
}

pub(crate) fn queue_xml_subtree(
    rt: &mut DomRuntime,
    menu_root: Entity,
    parent_id: i64,
    xml: roxmltree::Node,
) -> i64 {
    let id = rt.queue_node(menu_root, parent_id, NodeSpec::from_xml(xml));
    for child in xml.children().filter(|c| c.is_element()) {
        queue_xml_subtree(rt, menu_root, id, child);
    }
    id
}

pub(crate) fn apply_dom_ops(world: &mut World) {
    let ops = std::mem::take(&mut world.resource_mut::<DomRuntime>().ops);
    if ops.is_empty() {
        return;
    }

    let mut needs_prune = false;
    let entity_of =
        |world: &World, id: i64| world.resource::<DomRuntime>().nodes.get(&id).map(|n| n.entity);

    for op in ops {
        match op {
            DomOp::Create {
                id,
                parent_id,
                spec,
            } => {
                if let Some(parent) = entity_of(world, parent_id) {
                    let entity = spawn_node_entity(world, id, parent, spec);
                    if let Some(node) = world.resource_mut::<DomRuntime>().nodes.get_mut(&id) {
                        node.entity = entity;
                    }
                }
            }
            DomOp::Clear(id) => {
                if let Some(entity) = entity_of(world, id)
                    && let Ok(mut e) = world.get_entity_mut(entity)
                {
                    e.despawn_children();
                    needs_prune = true;
                }
            }
            DomOp::Remove(id) => {
                if let Some(entity) = entity_of(world, id)
                    && let Ok(e) = world.get_entity_mut(entity)
                {
                    e.despawn();
                    needs_prune = true;
                }
            }
            DomOp::SetText { id, text } => {
                let Some(entity) = entity_of(world, id) else {
                    continue;
                };
                if let Some(mut t) = world.get_mut::<Text>(entity) {
                    if t.0 != text {
                        t.0 = text;
                    }
                } else if let Some(mut span) = world.get_mut::<TextSpan>(entity)
                    && span.0 != text
                {
                    span.0 = text;
                }
            }
            DomOp::SetValue { id, text } => {
                let Some(entity) = entity_of(world, id) else {
                    continue;
                };
                if let Some(mut editable) = world.get_mut::<EditableText>(entity) {
                    editable.queue_edit(TextEdit::SelectAll);
                    editable.queue_edit(TextEdit::Insert(text.into()));
                }
            }
            DomOp::SetClass { id, cls, add } => {
                let Some(entity) = entity_of(world, id) else {
                    continue;
                };
                let updated = {
                    let mut rt = world.resource_mut::<DomRuntime>();
                    let Some(node) = rt.nodes.get_mut(&id) else {
                        continue;
                    };
                    let len = node.classes.len();
                    if add {
                        if !node.classes.contains(&cls) {
                            node.classes.push(cls.clone());
                        }
                    } else {
                        node.classes.retain(|c| c != &cls);
                    }
                    (node.classes.len() != len).then(|| node.classes.clone())
                };
                if let Some(classes) = updated
                    && let Ok(mut e) = world.get_entity_mut(entity)
                {
                    e.insert(ClassList::new_with_classes(classes));
                }
            }
            DomOp::BindEvent { id, event, handler } => {
                let Some(entity) = entity_of(world, id) else {
                    continue;
                };
                match world.get_mut::<DomHandlers>(entity) {
                    Some(mut handlers) => {
                        let key = if event == "press" { "click" } else { &event };
                        handlers.0.insert(key.to_string(), handler);
                    }
                    None => {
                        warn!("dom.on: entity for dom id {id} has no DomHandlers, dropping '{event}' -> '{handler}'");
                    }
                }
            }
            DomOp::Focus(id) => {
                if let Some(entity) = entity_of(world, id) {
                    world.resource_mut::<MenuFocusState>().last_focused = Some(entity);
                    world
                        .resource_mut::<InputFocus>()
                        .set(entity, FocusCause::Navigated);
                }
            }
            DomOp::Blur => {
                world.resource_mut::<MenuFocusState>().last_focused = None;
                world.resource_mut::<InputFocus>().clear();
            }
        }
    }

    if needs_prune {
        prune_dead_nodes(world);
    }
}

pub(crate) fn prune_dead_nodes(world: &mut World) {
    let dead: Vec<i64> = world
        .resource::<DomRuntime>()
        .nodes
        .iter()
        .filter_map(|(&id, n)| {
            (n.entity != Entity::PLACEHOLDER && world.get_entity(n.entity).is_err()).then_some(id)
        })
        .collect();
    let mut rt = world.resource_mut::<DomRuntime>();
    for id in dead {
        rt.nodes.remove(&id);
    }
}
