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
use node::{NodeSpec, PendingNode, spawn_pending_node};

#[derive(Component)]
pub(crate) struct DomNodeMeta {
    pub id: i64,
    pub menu_root: Entity,
    pub html_id: Option<String>,
    pub classes: Vec<String>,
}

impl DomNodeMeta {
    fn queued(id: i64, menu_root: Entity, spec: &NodeSpec) -> Self {
        Self {
            id,
            menu_root,
            html_id: spec.id.clone(),
            classes: spec.classes.clone(),
        }
    }
}

#[derive(Component, Default)]
pub(crate) struct DomHandlers(pub HashMap<String, String>);

impl DomHandlers {
    fn from_pairs(pairs: Vec<(String, String)>) -> Self {
        Self(
            pairs
                .into_iter()
                .map(|(event, handler)| (canonical_event(&event).to_string(), handler))
                .collect(),
        )
    }
}

fn canonical_event(event: &str) -> &str {
    if event == "press" { "click" } else { event }
}

pub(crate) fn root_node(id: i64, menu_root: Entity) -> impl Bundle {
    (
        DomNodeMeta {
            id,
            menu_root,
            html_id: None,
            classes: Vec::new(),
        },
        DomHandlers::default(),
    )
}

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

struct DomRecord {
    owner: Entity,
    entity: Option<Entity>,
}

#[derive(Resource, Default)]
pub(crate) struct DomIndex {
    records: HashMap<i64, DomRecord>,
    html_ids: HashMap<(Entity, String), i64>,
}

impl DomIndex {
    fn register(&mut self, id: i64, owner: Entity, html_id: Option<&str>) {
        if let Some(name) = html_id {
            self.html_ids.insert((owner, name.to_string()), id);
        }
        self.records.insert(
            id,
            DomRecord {
                owner,
                entity: None,
            },
        );
    }

    fn attach(&mut self, id: i64, entity: Entity) {
        if let Some(record) = self.records.get_mut(&id) {
            record.entity = Some(entity);
        }
    }

    pub(crate) fn entity(&self, id: i64) -> Option<Entity> {
        self.records.get(&id).and_then(|record| record.entity)
    }

    pub(crate) fn owner(&self, id: i64) -> Option<Entity> {
        self.records.get(&id).map(|record| record.owner)
    }

    fn lookup(&self, owner: Entity, html_id: &str) -> Option<i64> {
        self.html_ids
            .get(&(owner, html_id.to_string()))
            .copied()
            .filter(|id| self.entity(*id).is_some())
    }

    fn drop_record(&mut self, id: i64) {
        self.records.remove(&id);
    }

    fn detach(&mut self, meta: &DomNodeMeta) {
        self.drop_record(meta.id);
        if let Some(name) = &meta.html_id {
            self.html_ids.remove(&(meta.menu_root, name.clone()));
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct DomRuntime {
    next_id: i64,
    pub index: DomIndex,
    pub ops: Vec<DomOp>,
}

impl DomRuntime {
    pub(crate) fn alloc_id(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }

    pub(crate) fn queue_node(&mut self, menu_root: Entity, parent_id: i64, spec: NodeSpec) -> i64 {
        let id = self.alloc_id();
        self.index.register(id, menu_root, spec.id.as_deref());
        self.ops.push(DomOp::Create {
            id,
            parent_id,
            spec,
        });
        id
    }

    pub(crate) fn open_root(&mut self, root: Entity) -> i64 {
        let id = self.alloc_id();
        self.index.register(id, root, None);
        self.index.attach(id, root);
        id
    }

    pub(crate) fn find_by_html_id(&self, scope_id: i64, target_id: &str) -> Option<i64> {
        let owner = self.index.owner(scope_id)?;
        self.index.lookup(owner, target_id)
    }

    pub(crate) fn queue_spec_tree(
        &mut self,
        menu_root: Entity,
        parent_id: i64,
        tree: &SpecTree,
    ) -> i64 {
        let id = self.queue_node(menu_root, parent_id, tree.spec.clone());
        for child in &tree.children {
            self.queue_spec_tree(menu_root, id, child);
        }
        id
    }
}

pub(crate) struct SpecTree {
    spec: NodeSpec,
    children: Vec<SpecTree>,
}

pub(crate) fn spec_tree(xml: roxmltree::Node) -> SpecTree {
    SpecTree {
        spec: NodeSpec::from_xml(xml),
        children: xml
            .children()
            .filter(|c| c.is_element())
            .map(spec_tree)
            .collect(),
    }
}

pub(crate) fn queue_xml_subtree(
    rt: &mut DomRuntime,
    menu_root: Entity,
    parent_id: i64,
    xml: roxmltree::Node,
) -> i64 {
    rt.queue_spec_tree(menu_root, parent_id, &spec_tree(xml))
}

pub(crate) fn forget_despawned_node(
    trigger: On<Despawn<DomNodeMeta>>,
    metas: Query<&DomNodeMeta>,
    mut runtime: ResMut<DomRuntime>,
) {
    if let Ok(meta) = metas.get(trigger.event_target()) {
        runtime.index.detach(meta);
    }
}

pub(crate) fn process_dom_ops(
    mut commands: Commands,
    mut runtime: ResMut<DomRuntime>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
    mut spans: Query<&mut TextSpan>,
    mut editables: Query<&mut EditableText>,
    mut handlers: Query<&mut DomHandlers>,
    mut metas: Query<&mut DomNodeMeta>,
    mut focus_state: ResMut<MenuFocusState>,
    mut input_focus: ResMut<InputFocus>,
) {
    let ops = std::mem::take(&mut runtime.ops);
    if ops.is_empty() {
        return;
    }

    let mut pending: HashMap<Entity, PendingNode> = HashMap::default();

    for op in ops {
        match op {
            DomOp::Create {
                id,
                parent_id,
                spec,
            } => {
                let (Some(menu_root), Some(parent)) =
                    (runtime.index.owner(id), runtime.index.entity(parent_id))
                else {
                    continue;
                };
                let entity = commands
                    .spawn((ChildOf(parent), DomNodeMeta::queued(id, menu_root, &spec)))
                    .id();
                runtime.index.attach(id, entity);
                pending.insert(
                    entity,
                    PendingNode {
                        id,
                        menu_root,
                        parent,
                        spec,
                        value: None,
                        handlers: Vec::new(),
                    },
                );
            }
            DomOp::Clear(id) => {
                let Some(entity) = runtime.index.entity(id) else {
                    continue;
                };
                if pending.contains_key(&entity) {
                    continue;
                }
                if let Ok(kids) = children.get(entity) {
                    for kid in kids.iter() {
                        drop_pending_subtree(
                            &mut pending,
                            &mut runtime.index,
                            &mut commands,
                            kid,
                        );
                        commands.entity(kid).despawn();
                    }
                }
                drop_pending_subtree(&mut pending, &mut runtime.index, &mut commands, entity);
            }
            DomOp::Remove(id) => {
                let Some(entity) = runtime.index.entity(id) else {
                    continue;
                };
                pending.remove(&entity);
                drop_pending_subtree(&mut pending, &mut runtime.index, &mut commands, entity);
                runtime.index.drop_record(id);
                commands.entity(entity).despawn();
            }
            DomOp::SetText { id, text } => {
                let Some(entity) = runtime.index.entity(id) else {
                    continue;
                };
                if let Some(node) = pending.get_mut(&entity) {
                    node.spec.text = text;
                    continue;
                }
                if let Ok(mut text_node) = texts.get_mut(entity) {
                    if text_node.0 != text {
                        text_node.0 = text;
                    }
                } else if let Ok(mut span) = spans.get_mut(entity)
                    && span.0 != text
                {
                    span.0 = text;
                }
            }
            DomOp::SetValue { id, text } => {
                let Some(entity) = runtime.index.entity(id) else {
                    continue;
                };
                if let Some(node) = pending.get_mut(&entity) {
                    node.value = Some(text);
                    continue;
                }
                if let Ok(mut editable) = editables.get_mut(entity) {
                    editable.queue_edit(TextEdit::SelectAll);
                    editable.queue_edit(TextEdit::Insert(text.into()));
                }
            }
            DomOp::SetClass { id, cls, add } => {
                let Some(entity) = runtime.index.entity(id) else {
                    continue;
                };
                if let Some(node) = pending.get_mut(&entity) {
                    apply_class(&mut node.spec.classes, &cls, add);
                    continue;
                }
                let Ok(mut meta) = metas.get_mut(entity) else {
                    continue;
                };
                let before = meta.classes.len();
                apply_class(&mut meta.classes, &cls, add);
                if meta.classes.len() != before {
                    let classes = meta.classes.clone();
                    drop(meta);
                    commands.entity(entity).insert(ClassList::new_with_classes(classes));
                }
            }
            DomOp::BindEvent {
                id,
                event,
                handler,
            } => {
                let Some(entity) = runtime.index.entity(id) else {
                    warn!("dom.on: entity for dom id {id} has no DomHandlers, dropping '{event}' -> '{handler}'");
                    continue;
                };
                match handlers.get_mut(entity) {
                    Ok(mut handlers) => {
                        handlers
                            .0
                            .insert(canonical_event(&event).to_string(), handler);
                    }
                    Err(_) => match pending.get_mut(&entity) {
                        Some(node) => node.handlers.push((event, handler)),
                        None => {
                            warn!("dom.on: entity for dom id {id} has no DomHandlers, dropping '{event}' -> '{handler}'");
                        }
                    },
                }
            }
            DomOp::Focus(id) => {
                let Some(entity) = runtime.index.entity(id) else {
                    continue;
                };
                focus_state.last_focused = Some(entity);
                input_focus.set(entity, FocusCause::Navigated);
            }
            DomOp::Blur => {
                focus_state.last_focused = None;
                input_focus.clear();
            }
        }
    }

    for (entity, node) in pending {
        spawn_pending_node(&mut commands, entity, node);
    }
}

fn drop_pending_subtree(
    pending: &mut HashMap<Entity, PendingNode>,
    index: &mut DomIndex,
    commands: &mut Commands,
    parent: Entity,
) {
    let kids: Vec<Entity> = pending
        .iter()
        .filter(|(_, node)| node.parent == parent)
        .map(|(entity, _)| *entity)
        .collect();
    for entity in kids {
        if let Some(node) = pending.remove(&entity) {
            index.drop_record(node.id);
        }
        drop_pending_subtree(pending, index, commands, entity);
        commands.entity(entity).despawn();
    }
}

fn apply_class(classes: &mut Vec<String>, cls: &str, add: bool) {
    if add {
        if !classes.iter().any(|existing| existing == cls) {
            classes.push(cls.to_string());
        }
    } else {
        classes.retain(|existing| existing != cls);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholder(index: u32) -> Entity {
        Entity::from_raw_u32(index).unwrap()
    }

    #[test]
    fn html_ids_resolve_only_within_their_own_menu() {
        let mut runtime = DomRuntime::default();
        let first_menu = placeholder(1);
        let second_menu = placeholder(2);
        let first_root = runtime.open_root(first_menu);
        let second_root = runtime.open_root(second_menu);

        let search = runtime.queue_node(
            first_menu,
            first_root,
            NodeSpec {
                tag: "input".into(),
                id: Some("search".into()),
                ..default()
            },
        );
        let other = runtime.queue_node(
            second_menu,
            second_root,
            NodeSpec {
                tag: "input".into(),
                id: Some("search".into()),
                ..default()
            },
        );
        runtime.index.attach(search, placeholder(10));
        runtime.index.attach(other, placeholder(11));

        assert_eq!(runtime.find_by_html_id(first_root, "search"), Some(search));
        assert_eq!(runtime.find_by_html_id(second_root, "search"), Some(other));
        assert_eq!(runtime.find_by_html_id(first_root, "missing"), None);
    }

    #[test]
    fn a_queued_node_resolves_its_menu_before_it_exists() {
        let mut runtime = DomRuntime::default();
        let menu = placeholder(7);
        let root = runtime.open_root(menu);
        let parent = runtime.queue_node(menu, root, NodeSpec::default());

        assert_eq!(runtime.index.owner(parent), Some(menu));
        assert_eq!(
            runtime.index.entity(parent),
            None,
            "a queued node has no entity until the ops are processed"
        );
    }

    #[test]
    fn detaching_a_node_drops_every_index_entry() {
        let mut runtime = DomRuntime::default();
        let menu = placeholder(3);
        let root = runtime.open_root(menu);
        let named = runtime.queue_node(
            menu,
            root,
            NodeSpec {
                tag: "node".into(),
                id: Some("row".into()),
                ..default()
            },
        );
        let entity = placeholder(40);
        runtime.index.attach(named, entity);

        runtime.index.detach(&DomNodeMeta {
            id: named,
            menu_root: menu,
            html_id: Some("row".into()),
            classes: Vec::new(),
        });

        assert_eq!(runtime.index.entity(named), None);
        assert_eq!(runtime.index.owner(named), None);
        assert_eq!(runtime.find_by_html_id(root, "row"), None);
    }

    #[test]
    fn an_unattached_html_id_never_resolves() {
        let mut runtime = DomRuntime::default();
        let menu = placeholder(5);
        let root = runtime.open_root(menu);
        let named = runtime.queue_node(menu, root, NodeSpec {
            tag: "node".into(),
            id: Some("row".into()),
            ..default()
        });

        assert_eq!(
            runtime.find_by_html_id(root, "row"),
            None,
            "a queued node must not be handed out before it is spawned"
        );
        runtime.index.attach(named, placeholder(60));
        assert_eq!(runtime.find_by_html_id(root, "row"), Some(named));
    }

    #[test]
    fn press_is_the_same_handler_as_click() {
        assert_eq!(canonical_event("press"), "click");
        assert_eq!(canonical_event("right_click"), "right_click");
        assert_eq!(
            DomHandlers::from_pairs(vec![("press".into(), "on_row".into())]).0["click"],
            "on_row"
        );
    }

    #[test]
    fn class_lists_add_and_drop_without_duplicates() {
        let mut classes: Vec<String> = Vec::new();
        apply_class(&mut classes, "selected", true);
        apply_class(&mut classes, "selected", true);
        assert_eq!(classes, vec!["selected".to_string()]);
        apply_class(&mut classes, "row", true);
        apply_class(&mut classes, "selected", false);
        assert_eq!(classes, vec!["row".to_string()]);
    }

    fn dom_app() -> App {
        let mut app = App::new();
        app.init_resource::<DomRuntime>()
            .init_resource::<MenuFocusState>()
            .init_resource::<InputFocus>()
            .add_observer(forget_despawned_node)
            .add_systems(Update, process_dom_ops);
        app
    }

    fn open_menu(app: &mut App) -> (Entity, i64) {
        let menu = app.world_mut().spawn_empty().id();
        let root = {
            let mut runtime = app.world_mut().resource_mut::<DomRuntime>();
            runtime.open_root(menu)
        };
        app.world_mut().entity_mut(menu).insert(root_node(root, menu));
        (menu, root)
    }

    #[test]
    fn a_handler_bound_on_the_menu_root_reaches_the_root_entity() {
        let mut app = dom_app();
        let (menu, root) = open_menu(&mut app);
        app.world_mut()
            .resource_mut::<DomRuntime>()
            .ops
            .push(DomOp::BindEvent {
                id: root,
                event: "right_click".into(),
                handler: "on_right_click".into(),
            });

        app.update();

        assert_eq!(
            app.world()
                .get::<DomHandlers>(menu)
                .expect("the menu root is a dom node like any other")
                .0["right_click"],
            "on_right_click",
            "an event bound on the root dom id must bubble up from every node in the menu"
        );
    }

    #[test]
    fn a_nested_batch_spawns_every_node_under_its_own_parent() {
        let mut app = dom_app();
        let (menu, root) = open_menu(&mut app);
        let (parent, child) = {
            let mut runtime = app.world_mut().resource_mut::<DomRuntime>();
            let parent = runtime.queue_node(
                menu,
                root,
                NodeSpec {
                    tag: "node".into(),
                    id: Some("box".into()),
                    ..default()
                },
            );
            let child = runtime.queue_node(
                menu,
                parent,
                NodeSpec {
                    tag: "text".into(),
                    text: "hello".into(),
                    ..default()
                },
            );
            (parent, child)
        };

        app.update();

        let world = app.world();
        let parent_entity = world
            .resource::<DomRuntime>()
            .index
            .entity(parent)
            .expect("the parent was spawned");
        let child_entity = world
            .resource::<DomRuntime>()
            .index
            .entity(child)
            .expect("the child was spawned");

        assert_eq!(world.get::<ChildOf>(parent_entity).unwrap().parent(), menu);
        assert_eq!(
            world.get::<ChildOf>(child_entity).unwrap().parent(),
            parent_entity,
            "a child queued in the same batch as its parent must still find it"
        );
        assert_eq!(world.get::<Text>(child_entity).unwrap().0, "hello");
        assert_eq!(world.get::<DomNodeMeta>(child_entity).unwrap().menu_root, menu);
        assert_eq!(
            world.resource::<DomRuntime>().find_by_html_id(root, "box"),
            Some(parent)
        );
    }

    #[test]
    fn edits_queued_beside_a_create_land_on_the_fresh_node() {
        let mut app = dom_app();
        let (menu, root) = open_menu(&mut app);
        let id = {
            let mut runtime = app.world_mut().resource_mut::<DomRuntime>();
            let id = runtime.queue_node(
                menu,
                root,
                NodeSpec {
                    tag: "text".into(),
                    text: "before".into(),
                    ..default()
                },
            );
            runtime.ops.push(DomOp::SetText {
                id,
                text: "after".into(),
            });
            runtime.ops.push(DomOp::SetClass {
                id,
                cls: "selected".into(),
                add: true,
            });
            runtime.ops.push(DomOp::BindEvent {
                id,
                event: "click".into(),
                handler: "on_row".into(),
            });
            id
        };

        app.update();

        let world = app.world();
        let entity = world.resource::<DomRuntime>().index.entity(id).unwrap();
        assert_eq!(world.get::<Text>(entity).unwrap().0, "after");
        assert_eq!(
            world.get::<DomNodeMeta>(entity).unwrap().classes,
            vec!["selected".to_string()]
        );
        assert_eq!(
            world.get::<DomHandlers>(entity).unwrap().0["click"],
            "on_row",
            "a handler bound in the same batch as its node must survive the spawn"
        );
    }

    #[test]
    fn removing_a_node_drops_it_from_the_index() {
        let mut app = dom_app();
        let (menu, root) = open_menu(&mut app);
        let id = {
            let mut runtime = app.world_mut().resource_mut::<DomRuntime>();
            let id = runtime.queue_node(
                menu,
                root,
                NodeSpec {
                    tag: "node".into(),
                    id: Some("gone".into()),
                    ..default()
                },
            );
            runtime.ops.push(DomOp::Remove(id));
            id
        };

        app.update();

        let world = app.world();
        assert_eq!(world.resource::<DomRuntime>().index.entity(id), None);
        assert_eq!(world.resource::<DomRuntime>().find_by_html_id(root, "gone"), None);
    }

    #[test]
    fn despawning_a_menu_clears_the_index_as_it_goes() {
        let mut app = dom_app();
        let (menu, root) = open_menu(&mut app);
        let child = {
            let mut runtime = app.world_mut().resource_mut::<DomRuntime>();
            runtime.queue_node(
                menu,
                root,
                NodeSpec {
                    tag: "node".into(),
                    id: Some("row".into()),
                    ..default()
                },
            )
        };
        app.update();
        let child_entity = app.world().resource::<DomRuntime>().index.entity(child).unwrap();

        app.world_mut().entity_mut(menu).despawn();

        let world = app.world();
        assert!(
            world.get_entity(child_entity).is_err(),
            "the whole subtree goes with the menu root"
        );
        let runtime = world.resource::<DomRuntime>();
        assert_eq!(runtime.index.entity(root), None);
        assert_eq!(runtime.index.entity(child), None);
        assert_eq!(runtime.find_by_html_id(root, "row"), None);
    }

    #[test]
    fn clearing_a_node_drops_the_children_it_had_and_keeps_the_ones_it_gains() {
        let mut app = dom_app();
        let (menu, root) = open_menu(&mut app);
        let parent = {
            let mut runtime = app.world_mut().resource_mut::<DomRuntime>();
            runtime.queue_node(
                menu,
                root,
                NodeSpec {
                    tag: "node".into(),
                    ..default()
                },
            )
        };
        app.update();

        let (stale, fresh) = {
            let mut runtime = app.world_mut().resource_mut::<DomRuntime>();
            let stale = runtime.queue_node(
                menu,
                parent,
                NodeSpec {
                    tag: "node".into(),
                    ..default()
                },
            );
            runtime.ops.push(DomOp::Clear(parent));
            let fresh = runtime.queue_node(
                menu,
                parent,
                NodeSpec {
                    tag: "node".into(),
                    ..default()
                },
            );
            (stale, fresh)
        };
        app.update();

        let runtime = app.world().resource::<DomRuntime>();
        assert_eq!(
            runtime.index.entity(stale),
            None,
            "a child queued before the clear is despawned with the rest"
        );
        assert!(
            runtime.index.entity(fresh).is_some(),
            "a child queued after the clear survives it"
        );
        assert!(app.world().get_entity(runtime.index.entity(parent).unwrap()).is_ok());
    }
}
