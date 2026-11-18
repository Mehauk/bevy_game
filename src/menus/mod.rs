pub mod command_menu;

use std::path::{Path, PathBuf};

use bevy::{
    input::keyboard::KeyboardInput,
    input_focus::{AutoFocus, FocusedInput, InputFocus},
    prelude::*,
    text::{
        EditableText, EditableTextSystems, LineBreak, TextCursorStyle, TextEdit, TextEditChange,
    },
    ui::InteractionDisabled,
    ui_widgets::TextInput,
};
use bevy_hui::prelude::*;
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

callback_labels!(
    OnMenuReady => "on_menu_ready",
    OnMenuTextChanged => "on_menu_text_changed",
    OnMenuKey => "on_menu_key",
);

const MAX_UI_OP_RETRIES: u8 = 60;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuManager>()
            .init_resource::<MenuUiOps>()
            .add_message::<ToggleMenu>()
            .add_systems(Startup, setup_menu_hui)
            .add_systems(
                Update,
                (
                    reload_menus_on_asset_change,
                    handle_toggle_menus,
                    close_top_menu_on_escape,
                    dispatch_menu_ready,
                    event_handler::<OnMenuReady, LuaScriptingPlugin>,
                    event_handler::<OnMenuTextChanged, LuaScriptingPlugin>,
                    event_handler::<OnMenuKey, LuaScriptingPlugin>,
                    apply_menu_ui_ops,
                )
                    .chain(),
            )
            .add_systems(PostUpdate, strip_menu_nav_text_edits.before(EditableTextSystems));

        NamespaceBuilder::in_namespace(app.world_mut(), "menus")
            .register("close_top", |ctx: FunctionCallContext| {
                queue_op(&ctx, MenuOp::CloseTop)
            })
            .register(
                "clear_children",
                |ctx: FunctionCallContext, menu: String, container: String| {
                    queue_op(&ctx, MenuOp::Container { menu, container, action: ContainerOp::Clear })
                },
            )
            .register(
                "spawn_component",
                |ctx: FunctionCallContext,
                 menu: String,
                 container: String,
                 template: String,
                 props_val: ScriptValue| {
                    queue_op(
                        &ctx,
                        MenuOp::Container {
                            menu,
                            container,
                            action: ContainerOp::Spawn { template, props: parse_props(props_val) },
                        },
                    )
                },
            )
            .register(
                "set_property",
                |ctx: FunctionCallContext,
                 menu: String,
                 container: String,
                 index: i64,
                 key: String,
                 value: String| {
                    queue_row(&ctx, menu, container, index, |i| ContainerOp::SetProp {
                        index: i,
                        key,
                        value,
                    })
                },
            )
            .register(
                "set_child_text",
                |ctx: FunctionCallContext,
                 menu: String,
                 container: String,
                 index: i64,
                 target: String,
                 text: String| {
                    queue_row(&ctx, menu, container, index, |i| ContainerOp::SetText {
                        index: i,
                        target,
                        text,
                    })
                },
            )
            .register(
                "set_child_rich_text",
                |ctx: FunctionCallContext,
                 menu: String,
                 container: String,
                 index: i64,
                 target: String,
                 segments_val: ScriptValue,
                 font_size_val: ScriptValue| {
                    let font_size = match font_size_val {
                        ScriptValue::Float(f) => f as f32,
                        ScriptValue::Integer(i) => i as f32,
                        _ => 14.0,
                    };
                    queue_row(&ctx, menu, container, index, |i| ContainerOp::SetRichText {
                        index: i,
                        target,
                        segments: parse_rich_segments(segments_val),
                        font_size,
                    })
                },
            )
            .register("set_input_text", |ctx: FunctionCallContext, menu: String, text: String| {
                queue_op(&ctx, MenuOp::SetInputText { menu, text })
            });
    }
}

#[derive(Message, Debug, Clone)]
pub struct ToggleMenu(pub String);

#[derive(Component)]
struct MenuRoot {
    name: String,
    script: Option<Handle<ScriptAsset>>,
    ready_sent: bool,
}

#[derive(Component, Default)]
struct MenuTextInput(String);

#[derive(Resource, Default)]
pub struct MenuManager {
    open_menus: Vec<(String, Entity)>,
}

impl MenuManager {
    pub fn any_open(&self) -> bool {
        !self.open_menus.is_empty()
    }

    fn entity_for(&self, name: &str) -> Option<Entity> {
        self.open_menus.iter().rev().find(|(n, _)| n == name).map(|(_, e)| *e)
    }
}

#[derive(Resource, Default)]
struct MenuUiOps(Vec<(u8, MenuOp)>);

#[derive(Debug)]
enum ContainerOp {
    Clear,
    Spawn { template: String, props: Vec<(String, String)> },
    SetProp { index: usize, key: String, value: String },
    SetText { index: usize, target: String, text: String },
    SetRichText { index: usize, target: String, segments: Vec<(String, Color)>, font_size: f32 },
}

impl ContainerOp {
    fn row_target(&self) -> Option<(usize, Option<&str>)> {
        match self {
            Self::Clear | Self::Spawn { .. } => None,
            Self::SetProp { index, .. } => Some((*index, None)),
            Self::SetText { index, target, .. } | Self::SetRichText { index, target, .. } => {
                Some((*index, Some(target)))
            }
        }
    }
}

#[derive(Debug)]
enum MenuOp {
    Toggle(String),
    Reload,
    CloseTop,
    SetInputText { menu: String, text: String },
    Container { menu: String, container: String, action: ContainerOp },
}

fn queue_op(ctx: &FunctionCallContext, op: MenuOp) -> Result<(), InteropError> {
    ctx.world()?
        .with_resource_mut(|mut ops: Mut<MenuUiOps>| ops.0.push((MAX_UI_OP_RETRIES, op)))
}

fn queue_row(
    ctx: &FunctionCallContext,
    menu: String,
    container: String,
    index: i64,
    make: impl FnOnce(usize) -> ContainerOp,
) -> Result<(), InteropError> {
    if index < 0 {
        return Ok(());
    }
    queue_op(ctx, MenuOp::Container { menu, container, action: make(index as usize) })
}

fn parse_props(val: ScriptValue) -> Vec<(String, String)> {
    let ScriptValue::Map(map) = val else { return Vec::new() };
    map.into_iter()
        .filter_map(|(k, v)| match v {
            ScriptValue::String(s) => Some((k, s.into_owned())),
            ScriptValue::Integer(i) => Some((k, i.to_string())),
            ScriptValue::Float(f) => Some((k, f.to_string())),
            ScriptValue::Bool(b) => Some((k, b.to_string())),
            _ => None,
        })
        .collect()
}

fn parse_rich_segments(val: ScriptValue) -> Vec<(String, Color)> {
    let ScriptValue::List(list) = val else { return Vec::new() };
    list.into_iter()
        .filter_map(|item| {
            let ScriptValue::Map(map) = item else { return None };
            let ScriptValue::String(text) = map.get("text")? else { return None };
            let color = match map.get("color") {
                Some(ScriptValue::String(hex)) => Srgba::hex(hex.as_ref()).map(Color::from).unwrap_or(Color::WHITE),
                _ => Color::WHITE,
            };
            Some((text.clone().into_owned(), color))
        })
        .collect()
}

fn get_base_dir(folder: &str) -> PathBuf {
    [PathBuf::from(folder), Path::new("..").join(folder)]
        .into_iter()
        .chain(std::env::var("CARGO_MANIFEST_DIR").ok().map(|m| Path::new(&m).join(folder)))
        .find(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(folder))
}

fn resolve_asset_path(relative_path: &str) -> String {
    if get_base_dir("mods").join(relative_path).is_file() {
        format!("mods://{relative_path}")
    } else {
        relative_path.to_string()
    }
}

fn resolve_menu_file(menu_name: &str, relative_file: &str) -> String {
    resolve_asset_path(&format!("menus/{menu_name}/{relative_file}"))
}

fn find_descendant(
    world: &World,
    root: Entity,
    pred: &impl Fn(&World, Entity) -> bool,
) -> Option<Entity> {
    for child in world.get::<Children>(root)?.iter() {
        if pred(world, child) {
            return Some(child);
        }
        if let Some(found) = find_descendant(world, child, pred) {
            return Some(found);
        }
    }
    None
}

fn find_ui_id(world: &World, root: Entity, target_id: &str) -> Option<Entity> {
    find_descendant(world, root, &|w, e| w.get::<UiId>(e).is_some_and(|id| id.id() == target_id))
}

fn emit_menu_callback<L: IntoCallbackLabel>(
    target: Entity,
    extra_arg: ScriptValue,
    q_parents: &Query<&ChildOf>,
    q_roots: &Query<&MenuRoot>,
    callbacks: &mut MessageWriter<ScriptCallbackMessage>,
) {
    let Some((menu_entity, root)) = std::iter::once(target)
        .chain(q_parents.iter_ancestors(target))
        .find_map(|e| q_roots.get(e).ok().map(|r| (e, r)))
    else {
        return;
    };
    let (true, Some(script)) = (root.ready_sent, root.script.clone()) else {
        return;
    };
    callbacks.write(ScriptCallbackMessage::new_for_script_entity(
        L::into_callback_label(),
        vec![ScriptValue::String(root.name.clone().into()), extra_arg],
        script,
        menu_entity,
    ));
}

fn on_menu_text_input_key(
    mut event: On<FocusedInput<KeyboardInput>>,
    q_parents: Query<&ChildOf>,
    q_roots: Query<&MenuRoot>,
    mut callbacks: MessageWriter<ScriptCallbackMessage>,
) {
    let label = match event.input.key_code {
        KeyCode::ArrowDown => "ArrowDown",
        KeyCode::ArrowUp => "ArrowUp",
        KeyCode::Tab => "Tab",
        KeyCode::Enter => "Enter",
        _ => return,
    };
    event.propagate(false);
    if !event.input.state.is_pressed()
        || (event.input.repeat && matches!(event.input.key_code, KeyCode::Tab | KeyCode::Enter))
    {
        return;
    }
    emit_menu_callback::<OnMenuKey>(
        event.event_target(),
        ScriptValue::String(label.into()),
        &q_parents,
        &q_roots,
        &mut callbacks,
    );
}

fn on_menu_text_edit_change(
    event: On<TextEditChange>,
    mut q_inputs: Query<(&EditableText, &mut MenuTextInput)>,
    q_parents: Query<&ChildOf>,
    q_roots: Query<&MenuRoot>,
    mut callbacks: MessageWriter<ScriptCallbackMessage>,
) {
    let target = event.event_target();
    let Ok((editable, mut last_value)) = q_inputs.get_mut(target) else { return };
    let value = editable.value().to_string();
    if last_value.0 == value {
        return;
    }
    last_value.0 = value.clone();
    emit_menu_callback::<OnMenuTextChanged>(
        target,
        ScriptValue::String(value.into()),
        &q_parents,
        &q_roots,
        &mut callbacks,
    );
}

fn strip_menu_nav_text_edits(mut q_editable: Query<&mut EditableText, With<MenuTextInput>>) {
    for mut editable in &mut q_editable {
        editable.pending_edits.retain(|edit| !matches!(edit, TextEdit::Up(_) | TextEdit::Down(_)));
    }
}

fn setup_menu_hui(
    server: Res<AssetServer>,
    mut html_comps: HtmlComponents,
    mut html_funcs: HtmlFunctions,
) {
    for base in ["assets", "mods"] {
        for entry in std::fs::read_dir(get_base_dir(base).join("menus")).into_iter().flatten().flatten() {
            for comp in std::fs::read_dir(entry.path().join("components")).into_iter().flatten().flatten() {
                let path = comp.path();
                if path.extension().and_then(|e| e.to_str()) == Some("html")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    let rel = format!(
                        "menus/{}/components/{}",
                        entry.file_name().to_string_lossy(),
                        path.file_name().unwrap().to_string_lossy()
                    );
                    html_comps.register(stem.to_string(), server.load(resolve_asset_path(&rel)));
                }
            }
        }
    }

    html_funcs.register("text_input", |In(entity): In<Entity>, mut commands: Commands| {
        commands
            .entity(entity)
            .insert((
                MenuTextInput::default(),
                TextInput,
                EditableText { cursor_width: 0.3, ..default() },
                TextLayout { linebreak: LineBreak::NoWrap, ..default() },
                TextCursorStyle {
                    color: Color::WHITE,
                    selection_color: Color::srgba(0.3, 0.5, 0.9, 0.4),
                    ..default()
                },
                InteractionDisabled,
                AutoFocus,
            ))
            .observe(on_menu_text_input_key)
            .observe(on_menu_text_edit_change);
    });

    html_funcs.register("menu_close", |In(_): In<Entity>, mut ops: ResMut<MenuUiOps>| {
        ops.0.push((0, MenuOp::CloseTop));
    });
}

fn reload_menus_on_asset_change(
    mut html_events: MessageReader<AssetEvent<HtmlTemplate>>,
    mut script_events: MessageReader<AssetEvent<ScriptAsset>>,
    manager: Res<MenuManager>,
    mut ops: ResMut<MenuUiOps>,
) {
    let modified = html_events.read().any(|e| matches!(e, AssetEvent::Modified { .. }))
        || script_events.read().any(|e| matches!(e, AssetEvent::Modified { .. }));
    if modified && manager.any_open() {
        ops.0.clear();
        ops.0.push((0, MenuOp::Reload));
    }
}

fn handle_toggle_menus(mut reader: MessageReader<ToggleMenu>, mut ops: ResMut<MenuUiOps>) {
    for msg in reader.read() {
        ops.0.push((0, MenuOp::Toggle(msg.0.clone())));
    }
}

fn close_top_menu_on_escape(
    keys: Res<ButtonInput<KeyCode>>,
    manager: Res<MenuManager>,
    mut ops: ResMut<MenuUiOps>,
) {
    if keys.just_pressed(KeyCode::Escape) && manager.any_open() {
        ops.0.push((0, MenuOp::CloseTop));
    }
}

fn dispatch_menu_ready(
    mut q_roots: Query<(Entity, &mut MenuRoot)>,
    q_children: Query<&Children>,
    script_contexts: Res<ScriptContexts<LuaScriptingPlugin>>,
    mut callbacks: MessageWriter<ScriptCallbackMessage>,
) {
    for (menu_entity, mut root) in &mut q_roots {
        if root.ready_sent {
            continue;
        }
        let Some(script) = root.script.clone() else { continue };
        let ui_built = q_children.get(menu_entity).is_ok_and(|c| !c.is_empty());
        let script_loaded = script_contexts
            .read()
            .get_context(&ScriptAttachment::EntityScript(menu_entity, script.clone()))
            .is_some_and(|c| c.as_loaded().is_some());

        if ui_built && script_loaded {
            root.ready_sent = true;
            callbacks.write(ScriptCallbackMessage::new_for_script_entity(
                OnMenuReady::into_callback_label(),
                vec![ScriptValue::String(root.name.clone().into())],
                script,
                menu_entity,
            ));
        }
    }
}

fn apply_menu_ui_ops(world: &mut World) {
    let ops = std::mem::take(&mut world.resource_mut::<MenuUiOps>().0);
    if ops.is_empty() {
        return;
    }

    let mut deferred = Vec::new();
    let retry = |deferred: &mut Vec<(u8, MenuOp)>, retries: u8, op: MenuOp| {
        if retries > 0 {
            deferred.push((retries - 1, op));
        } else {
            warn!("Dropped menu UI op after exceeding retry limit: {op:?}");
        }
    };

    for (retries, op) in ops {
        match &op {
            MenuOp::Toggle(name) => {
                let existing = world
                    .resource::<MenuManager>()
                    .open_menus
                    .iter()
                    .rposition(|(n, _)| n == name);
                if let Some(pos) = existing {
                    close_menu_at(world, pos);
                } else {
                    open_menu_in_world(world, name);
                }
            }
            MenuOp::Reload => {
                let count = world.resource::<MenuManager>().open_menus.len();
                let names: Vec<String> = (0..count).rev().map(|i| close_menu_at(world, i)).collect();
                for name in names.into_iter().rev() {
                    open_menu_in_world(world, &name);
                }
            }
            MenuOp::CloseTop => {
                if let Some(top) = world.resource::<MenuManager>().open_menus.len().checked_sub(1) {
                    close_menu_at(world, top);
                }
            }
            MenuOp::SetInputText { menu, text } => {
                let Some(root) = world.resource::<MenuManager>().entity_for(menu) else { continue };
                if let Some(input) = find_descendant(world, root, &|w, e| w.get::<EditableText>(e).is_some())
                    && let Some(mut editable) = world.get_mut::<EditableText>(input)
                {
                    editable.queue_edit(TextEdit::SelectAll);
                    editable.queue_edit(TextEdit::Insert(text.clone().into()));
                }
            }
            MenuOp::Container { menu, container, action } => {
                let Some(root) = world.resource::<MenuManager>().entity_for(menu) else { continue };
                let Some(container_entity) = find_ui_id(world, root, container) else {
                    retry(&mut deferred, retries, op);
                    continue;
                };

                let target = if let Some((index, target_id)) = action.row_target() {
                    let Some(row) = world.get::<Children>(container_entity).and_then(|c| c.get(index).copied()) else {
                        continue;
                    };
                    match target_id {
                        Some(id) => match find_ui_id(world, row, id) {
                            Some(t) => t,
                            None => {
                                retry(&mut deferred, retries, op);
                                continue;
                            }
                        },
                        None => row,
                    }
                } else {
                    Entity::PLACEHOLDER
                };

                match action {
                    ContainerOp::Clear => {
                        if let Ok(mut e) = world.get_entity_mut(container_entity) {
                            e.despawn_children();
                        }
                    }
                    ContainerOp::Spawn { template, props } => {
                        let mut template_props = TemplateProperties::default();
                        for (k, v) in props {
                            template_props.set(k, v);
                        }
                        let handle: Handle<HtmlTemplate> =
                            world.resource::<AssetServer>().load(resolve_menu_file(menu, template));

                        if let Ok(mut e) = world.get_entity_mut(container_entity) {
                            let mut row = Entity::PLACEHOLDER;
                            e.with_children(|parent| {
                                let mut child = parent.spawn((
                                    Node {
                                        width: Val::Percent(100.0),
                                        flex_direction: FlexDirection::Column,
                                        border_radius: BorderRadius::all(CornerRadius::circular(Val::Px(4.0))),
                                        ..default()
                                    },
                                    BackgroundColor(Color::NONE),
                                ));
                                row = child.id();
                                child.with_child((
                                    Node { width: Val::Percent(100.0), ..default() },
                                    HtmlNode(handle),
                                    template_props,
                                ));
                            });
                            for (k, v) in props {
                                set_row_prop(world, row, k, v);
                            }
                        }
                    }
                    ContainerOp::SetProp { key, value, .. } => set_row_prop(world, target, key, value),
                    ContainerOp::SetText { text, .. } => {
                        if let Some(mut t) = world.get_mut::<Text>(target)
                            && t.0 != *text
                        {
                            t.0 = text.clone();
                        }
                    }
                    ContainerOp::SetRichText { segments, font_size, .. } => {
                        if let Ok(mut e) = world.get_entity_mut(target) {
                            e.despawn_children().with_children(|parent| {
                                parent
                                    .spawn((
                                        Text::default(),
                                        TextFont::from_font_size(*font_size),
                                        TextLayout { linebreak: LineBreak::NoWrap, ..default() },
                                    ))
                                    .with_children(|text_parent| {
                                        for (seg_text, seg_color) in segments {
                                            text_parent.spawn((
                                                TextSpan::new(seg_text.clone()),
                                                TextFont::from_font_size(*font_size),
                                                TextColor(*seg_color),
                                            ));
                                        }
                                    });
                            });
                        }
                    }
                }
            }
        }
    }

    world.resource_mut::<MenuUiOps>().0.extend(deferred);
}

fn set_row_prop(world: &mut World, row: Entity, key: &str, value: &str) {
    match key {
        "bg" => {
            if let Ok(srgba) = Srgba::hex(value) {
                let color = Color::from(srgba);
                if let Some(mut bg) = world.get_mut::<BackgroundColor>(row)
                    && bg.0 != color
                {
                    bg.0 = color;
                }
            }
        }
        "display" => {
            let display = if value == "none" { Display::None } else { Display::Flex };
            if let Some(mut node) = world.get_mut::<Node>(row)
                && node.display != display
            {
                node.display = display;
            }
        }
        _ => {}
    }
}

fn close_menu_at(world: &mut World, pos: usize) -> String {
    let (name, root) = world.resource_mut::<MenuManager>().open_menus.remove(pos);
    if let Ok(e) = world.get_entity_mut(root) {
        e.despawn();
    }
    if world.resource::<MenuManager>().open_menus.is_empty() {
        world.resource_mut::<InputFocus>().clear();
    }
    name
}

fn open_menu_in_world(world: &mut World, name: &str) {
    let index_path = resolve_menu_file(name, &format!("{name}_index.html"));
    let script_rel = format!("menus/{name}/{name}_script.lua");
    let script_handle = ["mods", "assets"]
        .iter()
        .any(|base| get_base_dir(base).join(&script_rel).is_file())
        .then(|| world.resource::<AssetServer>().load(resolve_asset_path(&script_rel)));

    if script_handle.is_none() {
        warn!("Menu script not found at assets/{script_rel} or mods/{script_rel}");
    }

    let z = 100 + world.resource::<MenuManager>().open_menus.len() as i32;
    let html = world.resource::<AssetServer>().load(index_path);
    let mut entity = world.spawn((
        MenuRoot { name: name.to_string(), script: script_handle.clone(), ready_sent: false },
        GlobalZIndex(z),
        HtmlNode(html),
    ));
    if let Some(handle) = script_handle {
        entity.insert(ScriptComponent(vec![handle]));
    }
    let id = entity.id();
    world.resource_mut::<MenuManager>().open_menus.push((name.to_string(), id));
}