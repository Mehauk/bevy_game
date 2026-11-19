pub mod command_menu;
pub mod keyboard_menu;

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    time::SystemTime,
};

use bevy::{
    ecs::system::SystemParam,
    input::keyboard::KeyboardInput,
    input_focus::{AutoFocus, FocusCause, FocusedInput, InputFocus},
    picking::{
        events::{PointerClick, PointerOut, PointerOver},
        hover::Hovered,
        Pickable,
    },
    platform::collections::HashMap,
    prelude::*,
    text::{
        EditableText, EditableTextSystems, LineBreak, TextCursorStyle, TextEdit, TextEditChange,
    },
    ui::InteractionDisabled,
    ui_widgets::{Button as UiButton, TextInput},
};
use bevy_enhanced_input::prelude::*;
use bevy_flair::prelude::*;
use bevy_mod_scripting::{
    asset::ScriptAsset,
    bindings::{
        InteropError, WorldExtensions,
        function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
        script_value::ScriptValue,
    },
    core::{
        callback_labels,
        event::{IntoCallbackLabel, ScriptCallbackMessage},
        handler::event_handler,
        script::{ScriptComponent, ScriptContexts},
    },
    lua::LuaScriptingPlugin,
    script::ScriptAttachment,
};

use crate::{
    input::{ContextActive, HotkeyLabel, InputAppExt, MenuInput, RemapStatus},
    utils::commands::CommandAppExt,
};

callback_labels!(
    OnMenuReady => "on_menu_ready",
    OnDomEvent => "on_dom_event",
    OnMenuModifiersChanged => "on_modifiers_changed",
    OnMenuRemapResult => "on_remap_result",
);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuUpdateSystems;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuManager>()
            .init_resource::<MenuRegistry>()
            .init_resource::<DomRuntime>()
            .init_resource::<HtmlHotReloadState>()
            .init_resource::<MenuFocusState>()
            .add_message::<ToggleMenu>()
            .add_message::<MenuCmd>()
            .add_systems(Startup, setup_menu_input)
            .add_observer(on_close_top_menu)
            .add_observer(on_dom_click)
            .add_observer(on_dom_hover_enter)
            .add_observer(on_dom_hover_exit)
            .add_systems(
                Update,
                (
                    reload_menus_on_asset_change,
                    handle_menu_commands,
                    track_and_maintain_menu_focus,
                    dispatch_menu_ready,
                    dispatch_modifier_changes,
                    dispatch_remap_results,
                    event_handler::<OnMenuReady, LuaScriptingPlugin>,
                    event_handler::<OnDomEvent, LuaScriptingPlugin>,
                    event_handler::<OnMenuModifiersChanged, LuaScriptingPlugin>,
                    event_handler::<OnMenuRemapResult, LuaScriptingPlugin>,
                    apply_dom_ops,
                )
                    .chain()
                    .in_set(MenuUpdateSystems),
            )
            .add_systems(PostUpdate, strip_menu_nav_text_edits.before(EditableTextSystems));

        register_menu_namespaces(app);
    }
}

pub struct MenuSpec {
    pub id: &'static str,
    pub hotkey_label: &'static str,
    pub command: Option<(&'static str, &'static str)>,
    pub bindings: Vec<Binding>,
}

#[derive(Clone)]
pub struct MenuEntry {
    pub id: &'static str,
}

#[derive(Resource, Default)]
pub struct MenuRegistry {
    pub menus: Vec<MenuEntry>,
}

impl MenuRegistry {
    fn record(&mut self, id: &'static str) {
        if !self.menus.iter().any(|m| m.id == id) {
            self.menus.push(MenuEntry { id });
        }
    }
}

pub trait MenuAppExt {
    fn register_menu_with_hotkey<A: InputAction<Output = bool> + Send + Sync + 'static>(
        &mut self,
        spec: MenuSpec,
    ) -> &mut Self;
}

impl MenuAppExt for App {
    fn register_menu_with_hotkey<A: InputAction<Output = bool> + Send + Sync + 'static>(
        &mut self,
        spec: MenuSpec,
    ) -> &mut Self {
        let menu_id = spec.id;
        self.init_resource::<MenuRegistry>();
        self.world_mut()
            .resource_mut::<MenuRegistry>()
            .record(menu_id);

        if let Some((cmd_name, desc)) = spec.command {
            self.register_command(cmd_name, desc, move |mut writer: MessageWriter<ToggleMenu>| {
                writer.write(ToggleMenu(menu_id.to_string()));
            });
        }

        if !spec.bindings.is_empty() {
            self.register_gameplay_action::<A, _, _, _>(
                spec.hotkey_label,
                spec.bindings,
                move |_ev: On<Start<A>>, mut writer: MessageWriter<ToggleMenu>| {
                    writer.write(ToggleMenu(menu_id.to_string()));
                },
            );
        }

        self
    }
}

#[derive(InputAction)]
#[action_output(bool)]
struct CloseTopMenu;

fn setup_menu_input(mut commands: Commands) {
    let menu_ctx = commands
        .spawn((
            MenuInput,
            ContextActive(false),
            ContextActivity::<MenuInput>::INACTIVE,
        ))
        .id();

    let close_action = commands
        .spawn((
            Action::<CloseTopMenu>::new(),
            ActionOf::<MenuInput>::new(menu_ctx),
            ActionSettings {
                consume_input: true,
                require_reset: true,
                ..default()
            },
            HotkeyLabel("Close Menu"),
        ))
        .id();

    commands.spawn((Binding::from(KeyCode::Escape), BindingOf(close_action)));
}

fn on_close_top_menu(_event: On<Start<CloseTopMenu>>, mut writer: MessageWriter<MenuCmd>) {
    writer.write(MenuCmd::Close(None));
}

fn push_menu_cmd(ctx: &FunctionCallContext, cmd: MenuCmd) -> Result<(), InteropError> {
    ctx.world()?
        .with_resource_mut(|mut writer: Mut<Messages<MenuCmd>>| {
            writer.write(cmd);
        })
}

fn push_dom_op(ctx: &FunctionCallContext, op: DomOp) -> Result<(), InteropError> {
    ctx.world()?
        .with_resource_mut(|mut rt: Mut<DomRuntime>| rt.ops.push(op))
}

fn register_menu_namespaces(app: &mut App) {
    NamespaceBuilder::in_namespace(app.world_mut(), "menus")
        .register("open", |ctx: FunctionCallContext, name: String| {
            push_menu_cmd(&ctx, MenuCmd::Open(name))
        })
        .register("close", |ctx: FunctionCallContext, name: String| {
            push_menu_cmd(&ctx, MenuCmd::Close(Some(name)))
        })
        .register("close_top", |ctx: FunctionCallContext| {
            push_menu_cmd(&ctx, MenuCmd::Close(None))
        });

    NamespaceBuilder::in_namespace(app.world_mut(), "dom")
        .register(
            "get_by_id",
            |ctx: FunctionCallContext, scope_id: i64, id: String| -> Result<ScriptValue, InteropError> {
                ctx.world()?.with_resource(|rt: &DomRuntime| {
                    rt.find_by_html_id(scope_id, &id)
                        .map_or(ScriptValue::Unit, ScriptValue::Integer)
                })
            },
        )
        .register(
            "create",
            |ctx: FunctionCallContext,
             parent_id: i64,
             tag: String,
             opts: ScriptValue|
             -> Result<ScriptValue, InteropError> {
                let spec = NodeSpec::from_script_opts(tag, opts);
                ctx.world()?.with_resource_mut(|mut rt: Mut<DomRuntime>| {
                    let Some(menu_root) = rt.nodes.get(&parent_id).map(|n| n.menu_root) else {
                        return ScriptValue::Unit;
                    };
                    let id = rt.queue_node(menu_root, parent_id, spec);
                    ScriptValue::Integer(id)
                })
            },
        )
        .register(
            "html",
            |ctx: FunctionCallContext,
             parent_id: i64,
             html: String|
             -> Result<ScriptValue, InteropError> {
                let doc = roxmltree::Document::parse(&html)
                    .map_err(|e| InteropError::external_boxed(Box::new(e)))?;
                ctx.world()?.with_resource_mut(|mut rt: Mut<DomRuntime>| {
                    let Some(menu_root) = rt.nodes.get(&parent_id).map(|n| n.menu_root) else {
                        return ScriptValue::Unit;
                    };
                    let id = queue_xml_subtree(&mut rt, menu_root, parent_id, doc.root_element());
                    ScriptValue::Integer(id)
                })
            },
        )
        .register("clear", |ctx: FunctionCallContext, id: i64| {
            push_dom_op(&ctx, DomOp::Clear(id))
        })
        .register("remove", |ctx: FunctionCallContext, id: i64| {
            push_dom_op(&ctx, DomOp::Remove(id))
        })
        .register("set_text", |ctx: FunctionCallContext, id: i64, text: String| {
            push_dom_op(&ctx, DomOp::SetText { id, text })
        })
        .register("set_value", |ctx: FunctionCallContext, id: i64, text: String| {
            push_dom_op(&ctx, DomOp::SetValue { id, text })
        })
        .register("add_class", |ctx: FunctionCallContext, id: i64, cls: String| {
            push_dom_op(&ctx, DomOp::SetClass { id, cls, add: true })
        })
        .register("remove_class", |ctx: FunctionCallContext, id: i64, cls: String| {
            push_dom_op(&ctx, DomOp::SetClass { id, cls, add: false })
        })
        .register(
            "has_class",
            |ctx: FunctionCallContext, id: i64, cls: String| -> Result<bool, InteropError> {
                ctx.world()?.with_resource(|rt: &DomRuntime| {
                    rt.nodes
                        .get(&id)
                        .is_some_and(|n| n.classes.iter().any(|c| c == &cls))
                })
            },
        )
        .register(
            "on",
            |ctx: FunctionCallContext, id: i64, event: String, handler: String| {
                push_dom_op(&ctx, DomOp::BindEvent { id, event, handler })
            },
        )
        .register("focus", |ctx: FunctionCallContext, id: i64| {
            push_dom_op(&ctx, DomOp::Focus(id))
        })
        .register("blur", |ctx: FunctionCallContext| {
            push_dom_op(&ctx, DomOp::Blur)
        });
}

#[derive(Message, Debug, Clone)]
pub struct ToggleMenu(pub String);

#[derive(Message, Debug, Clone)]
enum MenuCmd {
    Open(String),
    Close(Option<String>),
}

#[derive(Component)]
struct MenuRoot {
    name: String,
    dom_id: i64,
    html_path: PathBuf,
    html_modified: Option<SystemTime>,
    script: Option<Handle<ScriptAsset>>,
    ready_sent: bool,
}

#[derive(Component)]
struct DomHandle(i64);

#[derive(Component, Default)]
struct DomHandlers(HashMap<String, String>);

#[derive(Component, Default)]
struct MenuTextInput(String);

#[derive(Resource, Default)]
pub struct MenuFocusState {
    pub last_focused: Option<Entity>,
}

#[derive(Resource, Default)]
pub struct MenuManager {
    open_menus: Vec<(String, Entity)>,
    last_mods: (bool, bool, bool, bool),
    last_pressed: Vec<String>,
}

impl MenuManager {
    pub fn any_open(&self) -> bool {
        !self.open_menus.is_empty()
    }
}

#[derive(Resource)]
struct HtmlHotReloadState {
    timer: Timer,
}

impl Default for HtmlHotReloadState {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(0.25, TimerMode::Repeating),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct NodeSpec {
    tag: String,
    id: Option<String>,
    classes: Vec<String>,
    text: String,
    autofocus: bool,
    mouse_input: bool,
}

impl NodeSpec {
    fn from_xml(node: roxmltree::Node) -> Self {
        let tag = node.tag_name().name().to_ascii_lowercase();
        let attr = |k| node.attribute(k);
        let text = attr("text").map(str::to_string).unwrap_or_else(|| {
            if matches!(tag.as_str(), "text" | "span") {
                node.children()
                    .filter_map(|c| c.is_text().then(|| c.text()).flatten())
                    .collect::<String>()
                    .trim()
                    .to_string()
            } else {
                String::new()
            }
        });

        let mouse_input = match attr("mouse") {
            Some("true") => true,
            Some("false") => false,
            _ => matches!(tag.as_str(), "input" | "button"),
        };

        Self {
            tag,
            id: attr("id").map(str::to_string),
            classes: attr("class").map(split_classes).unwrap_or_default(),
            text,
            autofocus: attr("autofocus") == Some("true"),
            mouse_input,
        }
    }

    fn from_script_opts(tag: String, opts: ScriptValue) -> Self {
        let tag_lower = tag.to_ascii_lowercase();
        let default_mouse = matches!(tag_lower.as_str(), "input" | "button");
        let mut spec = Self {
            tag: tag_lower,
            mouse_input: default_mouse,
            ..default()
        };
        let ScriptValue::Map(map) = opts else {
            return spec;
        };
        if let Some(ScriptValue::String(id)) = map.get("id") {
            spec.id = Some(id.to_string());
        }
        if let Some(ScriptValue::String(cls)) = map.get("class") {
            spec.classes = split_classes(cls);
        }
        if let Some(ScriptValue::String(text)) = map.get("text") {
            spec.text = text.to_string();
        }
        if let Some(ScriptValue::Bool(af)) = map.get("autofocus") {
            spec.autofocus = *af;
        }
        if let Some(ScriptValue::Bool(m)) = map.get("mouse") {
            spec.mouse_input = *m;
        }
        spec
    }
}

fn split_classes(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_string).collect()
}

#[derive(Debug)]
enum DomOp {
    Create { id: i64, parent_id: i64, spec: NodeSpec },
    Clear(i64),
    Remove(i64),
    SetText { id: i64, text: String },
    SetValue { id: i64, text: String },
    SetClass { id: i64, cls: String, add: bool },
    BindEvent { id: i64, event: String, handler: String },
    Focus(i64),
    Blur,
}

struct DomNode {
    entity: Entity,
    menu_root: Entity,
    html_id: Option<String>,
    classes: Vec<String>,
}

#[derive(Resource, Default)]
struct DomRuntime {
    next_id: i64,
    nodes: HashMap<i64, DomNode>,
    ops: Vec<DomOp>,
}

impl DomRuntime {
    fn alloc_node(
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

    fn queue_node(&mut self, menu_root: Entity, parent_id: i64, spec: NodeSpec) -> i64 {
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

    fn find_by_html_id(&self, scope_id: i64, target_id: &str) -> Option<i64> {
        let menu_root = self.nodes.get(&scope_id)?.menu_root;
        self.nodes.iter().find_map(|(&id, n)| {
            (n.menu_root == menu_root && n.html_id.as_deref() == Some(target_id)).then_some(id)
        })
    }
}

fn queue_xml_subtree(
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

fn resolve_menu_file(relative_path: &str) -> Option<(PathBuf, String)> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok().map(PathBuf::from);
    for (folder, prefix) in [("mods", "mods://"), ("assets", "")] {
        let candidates = [PathBuf::from(folder), Path::new("..").join(folder)]
            .into_iter()
            .chain(manifest_dir.as_ref().map(|m| m.join(folder)));
        for base in candidates {
            let full = base.join(relative_path);
            if full.is_file() {
                return Some((full, format!("{prefix}{relative_path}")));
            }
        }
    }
    None
}

#[derive(SystemParam)]
struct DomEventCtx<'w, 's> {
    handlers: Query<'w, 's, &'static DomHandlers>,
    parents: Query<'w, 's, &'static ChildOf>,
    roots: Query<'w, 's, &'static MenuRoot>,
    handles: Query<'w, 's, &'static DomHandle>,
    callbacks: MessageWriter<'w, ScriptCallbackMessage>,
}

impl DomEventCtx<'_, '_> {
    fn has_handler(&self, target: Entity, event: &str) -> bool {
        std::iter::once(target)
            .chain(self.parents.iter_ancestors(target))
            .any(|e| self.handlers.get(e).is_ok_and(|h| h.0.contains_key(event)))
    }

    fn emit(&mut self, target: Entity, event: &str, payload: ScriptValue) -> bool {
        let target_with_handler = std::iter::once(target)
            .chain(self.parents.iter_ancestors(target))
            .find(|&e| self.handlers.get(e).is_ok_and(|h| h.0.contains_key(event)));

        let Some(target_entity) = target_with_handler else {
            return false;
        };

        let handler = self.handlers.get(target_entity).unwrap().0.get(event).unwrap().clone();

        let Some((menu_entity, root)) = std::iter::once(target_entity)
            .chain(self.parents.iter_ancestors(target_entity))
            .find_map(|e| self.roots.get(e).ok().map(|r| (e, r)))
        else {
            return false;
        };
        if !root.ready_sent || root.script.is_none() {
            return false;
        }
        let script = root.script.clone().unwrap();
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

fn on_dom_click(mut event: On<PointerClick>, mut ctx: DomEventCtx) {
    if ctx.emit(event.event_target(), "click", ScriptValue::Unit) {
        event.propagate(false);
    }
}

fn on_dom_hover_enter(event: On<PointerOver>, mut ctx: DomEventCtx) {
    ctx.emit(event.event_target(), "hover_enter", ScriptValue::Unit);
}

fn on_dom_hover_exit(event: On<PointerOut>, mut ctx: DomEventCtx) {
    ctx.emit(event.event_target(), "hover_exit", ScriptValue::Unit);
}

fn on_dom_key_input(
    mut event: On<FocusedInput<KeyboardInput>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut ctx: DomEventCtx,
) {
    let target = event.event_target();
    if !event.input.state.is_pressed() || !ctx.has_handler(target, "key_down") {
        return;
    }
    if matches!(
        event.input.key_code,
        KeyCode::ArrowDown | KeyCode::ArrowUp | KeyCode::Tab | KeyCode::Enter
    ) {
        event.propagate(false);
        if event.input.repeat && matches!(event.input.key_code, KeyCode::Tab | KeyCode::Enter) {
            return;
        }
    }

    let mut payload = HashMap::default();
    payload.insert(
        "key".to_string(),
        ScriptValue::String(format!("{:?}", event.input.key_code).into()),
    );
    payload.insert(
        "ctrl".to_string(),
        ScriptValue::Bool(keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])),
    );
    payload.insert(
        "shift".to_string(),
        ScriptValue::Bool(keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])),
    );
    payload.insert(
        "alt".to_string(),
        ScriptValue::Bool(keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight])),
    );
    payload.insert("repeat".to_string(), ScriptValue::Bool(event.input.repeat));

    ctx.emit(target, "key_down", ScriptValue::Map(payload));
}

fn on_dom_text_change(
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

fn strip_menu_nav_text_edits(mut q_editable: Query<&mut EditableText, With<MenuTextInput>>) {
    for mut editable in &mut q_editable {
        editable
            .pending_edits
            .retain(|edit| !matches!(edit, TextEdit::Up(_) | TextEdit::Down(_)));
    }
}

fn track_and_maintain_menu_focus(
    manager: Res<MenuManager>,
    mut focus: ResMut<InputFocus>,
    mut state: ResMut<MenuFocusState>,
    q_inputs: Query<Entity, With<TextInput>>,
    q_autofocus: Query<Entity, (With<TextInput>, With<AutoFocus>)>,
) {
    if !manager.any_open() {
        state.last_focused = None;
        return;
    }

    if let Some(current) = focus.get() {
        if q_inputs.contains(current) {
            state.last_focused = Some(current);
        }
    } else if let Some(last) = state.last_focused {
        if q_inputs.contains(last) {
            focus.set(last, FocusCause::Navigated);
        } else {
            state.last_focused = None;
        }
    } else if let Some(af) = q_autofocus.iter().next() {
        focus.set(af, FocusCause::Auto);
        state.last_focused = Some(af);
    }
}

fn reload_menus_on_asset_change(
    time: Res<Time>,
    mut hot: ResMut<HtmlHotReloadState>,
    mut script_events: MessageReader<AssetEvent<ScriptAsset>>,
    q_roots: Query<&MenuRoot>,
    manager: Res<MenuManager>,
    mut commands: Commands,
) {
    let modified_scripts: Vec<AssetId<ScriptAsset>> = script_events
        .read()
        .filter_map(|e| match e {
            AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();

    let script_modified = !modified_scripts.is_empty()
        && q_roots.iter().any(|r| {
            r.script
                .as_ref()
                .is_some_and(|h| modified_scripts.contains(&h.id()))
        });

    let html_modified = hot.timer.tick(time.delta()).just_finished()
        && q_roots.iter().any(|r| {
            std::fs::metadata(&r.html_path)
                .ok()
                .and_then(|m| m.modified().ok())
                != r.html_modified
        });

    if (script_modified || html_modified) && manager.any_open() {
        commands.queue(|world: &mut World| {
            let count = world.resource::<MenuManager>().open_menus.len();
            let names: Vec<String> = (0..count).rev().map(|i| close_menu_at(world, i)).collect();
            for name in names.into_iter().rev() {
                open_menu_in_world(world, &name);
            }
        });
    }
}

fn handle_menu_commands(
    mut toggles: MessageReader<ToggleMenu>,
    mut cmds: MessageReader<MenuCmd>,
    mut commands: Commands,
) {
    let toggle_list: Vec<String> = toggles.read().map(|m| m.0.clone()).collect();
    let cmd_list: Vec<MenuCmd> = cmds.read().cloned().collect();
    if toggle_list.is_empty() && cmd_list.is_empty() {
        return;
    }

    commands.queue(move |world: &mut World| {
        for cmd in cmd_list {
            match cmd {
                MenuCmd::Open(name) => open_menu_in_world(world, &name),
                MenuCmd::Close(Some(name)) => {
                    if let Some(i) = world
                        .resource::<MenuManager>()
                        .open_menus
                        .iter()
                        .rposition(|(n, _)| n == &name)
                    {
                        close_menu_at(world, i);
                    }
                }
                MenuCmd::Close(None) => {
                    if let Some(top) = world
                        .resource::<MenuManager>()
                        .open_menus
                        .len()
                        .checked_sub(1)
                    {
                        close_menu_at(world, top);
                    }
                }
            }
        }
        for name in toggle_list {
            let pos = world
                .resource::<MenuManager>()
                .open_menus
                .iter()
                .rposition(|(n, _)| n == &name);
            match pos {
                Some(i) => {
                    close_menu_at(world, i);
                }
                None => open_menu_in_world(world, &name),
            }
        }
    });
}

fn dispatch_menu_ready(
    mut q_roots: Query<(Entity, &mut MenuRoot)>,
    script_contexts: Res<ScriptContexts<LuaScriptingPlugin>>,
    mut callbacks: MessageWriter<ScriptCallbackMessage>,
) {
    for (menu_entity, mut root) in &mut q_roots {
        if root.ready_sent {
            continue;
        }
        let Some(script) = root.script.clone() else {
            continue;
        };
        let loaded = script_contexts
            .read()
            .get_context(&ScriptAttachment::EntityScript(menu_entity, script.clone()))
            .is_some_and(|c| c.as_loaded().is_some());

        if loaded {
            root.ready_sent = true;
            callbacks.write(ScriptCallbackMessage::new_for_script_entity(
                OnMenuReady::into_callback_label(),
                vec![
                    ScriptValue::String(root.name.clone().into()),
                    ScriptValue::Integer(root.dom_id),
                ],
                script,
                menu_entity,
            ));
        }
    }
}

fn dispatch_modifier_changes(
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut mouse_motion: MessageReader<bevy::input::mouse::MouseMotion>,
    mut manager: ResMut<MenuManager>,
    q_roots: Query<(Entity, &MenuRoot)>,
    mut callbacks: MessageWriter<ScriptCallbackMessage>,
) {
    if !manager.any_open() {
        return;
    }

    let mods = crate::input::current_modifier_flags(&keys);
    let mut pressed: Vec<String> = keys
        .get_pressed()
        .map(|k| format!("{k:?}"))
        .collect();

    for mb in mouse_buttons.get_pressed() {
        let s = match mb {
            MouseButton::Left => "MouseLeft",
            MouseButton::Right => "MouseRight",
            MouseButton::Middle => "MouseMiddle",
            MouseButton::Back => "MouseBack",
            MouseButton::Forward => "MouseForward",
            _ => continue,
        };
        pressed.push(s.to_string());
    }

    if mouse_wheel.read().count() > 0 {
        pressed.push("MouseWheel".to_string());
    }
    // We intentionally drain mouse motion so message reader does not accumulate,
    // but do not add MouseMotion to pressed keys (Aim/Look should never glow)
    let _ = mouse_motion.read().count();

    pressed.sort();
    pressed.dedup();

    let mut just_pressed: Vec<String> = keys
        .get_just_pressed()
        .map(|k| format!("{k:?}"))
        .collect();
    for mb in mouse_buttons.get_just_pressed() {
        let s = match mb {
            MouseButton::Left => "MouseLeft",
            MouseButton::Right => "MouseRight",
            MouseButton::Middle => "MouseMiddle",
            MouseButton::Back => "MouseBack",
            MouseButton::Forward => "MouseForward",
            _ => continue,
        };
        just_pressed.push(s.to_string());
    }

    if mods == manager.last_mods && pressed == manager.last_pressed && just_pressed.is_empty() {
        return;
    }

    manager.last_mods = mods;
    manager.last_pressed = pressed.clone();

    let pressed_val = ScriptValue::List(
        pressed
            .into_iter()
            .map(|s| ScriptValue::String(Cow::Owned(s)))
            .collect(),
    );
    let just_pressed_val = ScriptValue::List(
        just_pressed
            .into_iter()
            .map(|s| ScriptValue::String(Cow::Owned(s)))
            .collect(),
    );

    for (entity, root) in &q_roots {
        if !root.ready_sent || root.script.is_none() {
            continue;
        }
        let script = root.script.clone().unwrap();
        callbacks.write(ScriptCallbackMessage::new_for_script_entity(
            OnMenuModifiersChanged::into_callback_label(),
            vec![
                crate::input::modifier_map(mods.0, mods.1, mods.2, mods.3),
                pressed_val.clone(),
                just_pressed_val.clone(),
            ],
            script,
            entity,
        ));
    }
}

fn dispatch_remap_results(
    remap_status: Res<RemapStatus>,
    manager: Res<MenuManager>,
    q_roots: Query<(Entity, &MenuRoot)>,
    mut callbacks: MessageWriter<ScriptCallbackMessage>,
) {
    if !remap_status.is_changed() || !manager.any_open() || remap_status.version == 0 {
        return;
    }
    for (entity, root) in &q_roots {
        if !root.ready_sent || root.script.is_none() {
            continue;
        }
        let script = root.script.clone().unwrap();
        callbacks.write(ScriptCallbackMessage::new_for_script_entity(
            OnMenuRemapResult::into_callback_label(),
            vec![
                ScriptValue::Bool(remap_status.success),
                ScriptValue::String(Cow::Owned(remap_status.message.clone())),
            ],
            script,
            entity,
        ));
    }
}

fn apply_dom_ops(world: &mut World) {
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
                    && let Ok(mut e) = world.get_entity_mut(entity)
                {
                    e.despawn_children();
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
                    let changed = if add {
                        !node.classes.contains(&cls) && {
                            node.classes.push(cls);
                            true
                        }
                    } else {
                        let len = node.classes.len();
                        node.classes.retain(|c| c != &cls);
                        node.classes.len() != len
                    };
                    changed.then(|| node.classes.clone())
                };
                if let Some(classes) = updated
                    && let Ok(mut e) = world.get_entity_mut(entity)
                {
                    e.insert(ClassList::new_with_classes(classes));
                }
            }
            DomOp::BindEvent { id, event, handler } => {
                if let Some(entity) = entity_of(world, id)
                    && let Some(mut handlers) = world.get_mut::<DomHandlers>(entity)
                {
                    let key = if event == "press" {
                        "click".into()
                    } else {
                        event
                    };
                    handlers.0.insert(key, handler);
                }
            }
            DomOp::Focus(id) => {
                if let Some(entity) = entity_of(world, id) {
                    world.resource_mut::<MenuFocusState>().last_focused = Some(entity);
                    world.resource_mut::<InputFocus>().set(entity, FocusCause::Navigated);
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

fn prune_dead_nodes(world: &mut World) {
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

fn spawn_node_entity(world: &mut World, id: i64, parent: Entity, spec: NodeSpec) -> Entity {
    let mut e = world.spawn((DomHandle(id), DomHandlers::default(), ChildOf(parent)));
    if let Some(name) = spec.id.clone() {
        e.insert(Name::new(name));
    }
    if !spec.classes.is_empty() {
        e.insert(ClassList::new_with_classes(spec.classes));
    }

    let is_input = spec.tag == "input";
    let is_interactive = is_input || spec.mouse_input || spec.id.is_some() || spec.tag == "button";

    match spec.tag.as_str() {
        "text" => {
            e.insert((
                Text::new(spec.text),
                TextLayout::default(),
                Pickable::IGNORE,
            ));
        }
        "span" => {
            e.insert((TextSpan::new(spec.text), Pickable::IGNORE));
        }
        "button" => {
            e.insert((Node::default(), UiButton, Hovered::default()));
        }
        "input" => {
            e.insert((
                Node::default(),
                MenuTextInput::default(),
                TextInput,
                EditableText {
                    cursor_width: 0.3,
                    ..default()
                },
                TextLayout {
                    linebreak: LineBreak::NoWrap,
                    ..default()
                },
                TextCursorStyle {
                    color: Color::WHITE,
                    selection_color: Color::srgba(0.3, 0.5, 0.9, 0.4),
                    ..default()
                },
            ));
            if !spec.mouse_input {
                e.insert(InteractionDisabled);
            }
            if spec.autofocus {
                e.insert(AutoFocus);
            }
        }
        _ => {
            e.insert((Node::default(), Hovered::default()));
        }
    }

    if is_interactive {
        e.observe(on_dom_click)
            .observe(on_dom_hover_enter)
            .observe(on_dom_hover_exit);
    }

    if is_input {
        e.observe(on_dom_key_input)
            .observe(on_dom_text_change);
    }

    e.id()
}

fn close_menu_at(world: &mut World, pos: usize) -> String {
    let (name, root) = world.resource_mut::<MenuManager>().open_menus.remove(pos);
    if let Ok(mut e) = world.get_entity_mut(root) {
        e.despawn_children();
        e.despawn();
    }
    prune_dead_nodes(world);
    if world.resource::<MenuManager>().open_menus.is_empty() {
        world.resource_mut::<InputFocus>().clear();
        world.resource_mut::<MenuFocusState>().last_focused = None;
    }
    name
}

fn open_menu_in_world(world: &mut World, name: &str) {
    let Some((html_path, _)) = resolve_menu_file(&format!("menus/{name}/index.html")) else {
        warn!("Menu HTML not found for '{name}'");
        return;
    };
    let Ok(html_source) = std::fs::read_to_string(&html_path) else {
        warn!("Failed to read menu HTML at {}", html_path.display());
        return;
    };
    let html_modified = std::fs::metadata(&html_path)
        .ok()
        .and_then(|m| m.modified().ok());

    let asset_server = world.resource::<AssetServer>();
    let css_handle = resolve_menu_file(&format!("menus/{name}/style.css"))
        .map(|(_, asset)| asset_server.load(asset));
    let script_handle = resolve_menu_file(&format!("menus/{name}/script.lua"))
        .map(|(_, asset)| asset_server.load(asset));
    if script_handle.is_none() {
        warn!("Menu script not found for '{name}'");
    }

    let z = 100 + world.resource::<MenuManager>().open_menus.len() as i32;
    let mut root_mut = world.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        GlobalZIndex(z),
    ));

    if let Some(css) = css_handle {
        root_mut.insert(Styled::new(css));
    }

    if let Some(handle) = script_handle.clone() {
        root_mut.insert(ScriptComponent(vec![handle]));
    }
    let root_entity = root_mut.id();

    let root_dom_id =
        world
            .resource_mut::<DomRuntime>()
            .alloc_node(root_entity, root_entity, None, Vec::new());

    if let Ok(mut e) = world.get_entity_mut(root_entity) {
        e.insert((
            DomHandle(root_dom_id),
            MenuRoot {
                name: name.to_string(),
                dom_id: root_dom_id,
                html_path,
                html_modified,
                script: script_handle,
                ready_sent: false,
            },
        ));
    }

    match roxmltree::Document::parse(&html_source) {
        Ok(doc) => {
            let mut rt = world.resource_mut::<DomRuntime>();
            queue_xml_subtree(&mut rt, root_entity, root_dom_id, doc.root_element());
        }
        Err(err) => error!("Failed to parse HTML for menu '{name}': {err}"),
    }

    world
        .resource_mut::<MenuManager>()
        .open_menus
        .push((name.to_string(), root_entity));

    apply_dom_ops(world);
}