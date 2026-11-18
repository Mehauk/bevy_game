pub mod command_menu;

use std::path::{Path, PathBuf};

use bevy::{
    input::keyboard::KeyboardInput,
    input_focus::{AutoFocus, FocusedInput, InputFocus},
    platform::collections::HashMap,
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
    OnMenuAction => "on_menu_action",
    OnMenuTextChanged => "on_menu_text_changed",
    OnMenuKey => "on_menu_key",
);

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuManager>()
            .init_resource::<MenuUiOps>()
            .add_message::<OpenMenu>()
            .add_message::<CloseMenu>()
            .add_message::<ToggleMenu>()
            .add_systems(Startup, setup_menu_hui)
            .add_systems(
                Update,
                (
                    reload_menus_on_asset_change,
                    handle_menu_messages,
                    dispatch_menu_ready,
                    forward_menu_keys,
                    close_top_menu_on_escape,
                    event_handler::<OnMenuReady, LuaScriptingPlugin>,
                    event_handler::<OnMenuAction, LuaScriptingPlugin>,
                    event_handler::<OnMenuTextChanged, LuaScriptingPlugin>,
                    event_handler::<OnMenuKey, LuaScriptingPlugin>,
                    apply_menu_ui_ops,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                strip_menu_nav_text_edits.before(EditableTextSystems),
            );

        NamespaceBuilder::in_namespace(app.world_mut(), "menus")
            .register(
                "open",
                |ctx: FunctionCallContext, name: String| -> Result<(), InteropError> {
                    ctx.world()?
                        .with_resource_mut(|mut writer: Mut<Messages<OpenMenu>>| {
                            writer.write(OpenMenu(name));
                        })
                },
            )
            .register(
                "close",
                |ctx: FunctionCallContext, name: String| -> Result<(), InteropError> {
                    ctx.world()?
                        .with_resource_mut(|mut writer: Mut<Messages<CloseMenu>>| {
                            writer.write(CloseMenu(name));
                        })
                },
            )
            .register(
                "close_top",
                |ctx: FunctionCallContext| -> Result<(), InteropError> {
                    ctx.world()?
                        .with_resource_mut(|mut ops: Mut<MenuUiOps>| {
                            ops.0.push(MenuUiOp::CloseTop);
                        })
                },
            )
            .register(
                "clear_children",
                |ctx: FunctionCallContext,
                 menu_name: String,
                 container_id: String|
                 -> Result<(), InteropError> {
                    ctx.world()?
                        .with_resource_mut(|mut ops: Mut<MenuUiOps>| {
                            ops.0.push(MenuUiOp::ClearChildren {
                                menu_name,
                                container_id,
                            });
                        })
                },
            )
            .register(
                "spawn_component",
                |ctx: FunctionCallContext,
                 menu_name: String,
                 container_id: String,
                 relative_template: String,
                 props_val: ScriptValue|
                 -> Result<(), InteropError> {
                    let mut props = Vec::new();
                    if let ScriptValue::Map(map) = props_val {
                        for (k, v) in map {
                            let s = match v {
                                ScriptValue::String(s) => s.into_owned(),
                                ScriptValue::Integer(i) => i.to_string(),
                                ScriptValue::Float(f) => f.to_string(),
                                ScriptValue::Bool(b) => b.to_string(),
                                _ => continue,
                            };
                            props.push((k, s));
                        }
                    }

                    ctx.world()?
                        .with_resource_mut(|mut ops: Mut<MenuUiOps>| {
                            ops.0.push(MenuUiOp::SpawnComponent {
                                menu_name,
                                container_id,
                                relative_template,
                                props,
                            });
                        })
                },
            )
            .register(
                "set_property",
                |ctx: FunctionCallContext,
                 menu_name: String,
                 container_id: String,
                 index: i64,
                 key: String,
                 value: String|
                 -> Result<(), InteropError> {
                    if index < 0 {
                        return Ok(());
                    }
                    ctx.world()?
                        .with_resource_mut(|mut ops: Mut<MenuUiOps>| {
                            ops.0.push(MenuUiOp::SetProperty {
                                menu_name,
                                container_id,
                                index: index as usize,
                                key,
                                value,
                            });
                        })
                },
            )
            .register(
                "set_child_text",
                |ctx: FunctionCallContext,
                 menu_name: String,
                 container_id: String,
                 index: i64,
                 target_id: String,
                 text: String|
                 -> Result<(), InteropError> {
                    if index < 0 {
                        return Ok(());
                    }
                    ctx.world()?
                        .with_resource_mut(|mut ops: Mut<MenuUiOps>| {
                            ops.0.push(MenuUiOp::SetChildText {
                                menu_name,
                                container_id,
                                index: index as usize,
                                target_id,
                                text,
                            });
                        })
                },
            )
            .register(
                "set_child_rich_text",
                |ctx: FunctionCallContext,
                 menu_name: String,
                 container_id: String,
                 index: i64,
                 target_id: String,
                 segments_val: ScriptValue,
                 font_size_val: ScriptValue|
                 -> Result<(), InteropError> {
                    if index < 0 {
                        return Ok(());
                    }

                    let font_size = match font_size_val {
                        ScriptValue::Float(f) => f as f32,
                        ScriptValue::Integer(i) => i as f32,
                        _ => 14.0,
                    };

                    let mut segments = Vec::new();
                    if let ScriptValue::List(list) = segments_val {
                        for item in list {
                            let ScriptValue::Map(map) = item else {
                                continue;
                            };
                            let Some(ScriptValue::String(text)) = map.get("text") else {
                                continue;
                            };
                            let color = match map.get("color") {
                                Some(ScriptValue::String(hex)) => Srgba::hex(hex.as_ref())
                                    .map(Color::from)
                                    .unwrap_or(Color::WHITE),
                                _ => Color::WHITE,
                            };
                            segments.push((text.clone().into_owned(), color));
                        }
                    }

                    ctx.world()?
                        .with_resource_mut(|mut ops: Mut<MenuUiOps>| {
                            ops.0.push(MenuUiOp::SetChildRichText {
                                menu_name,
                                container_id,
                                index: index as usize,
                                target_id,
                                segments,
                                font_size,
                            });
                        })
                },
            )
            .register(
                "set_input_text",
                |ctx: FunctionCallContext,
                 menu_name: String,
                 text: String|
                 -> Result<(), InteropError> {
                    ctx.world()?
                        .with_resource_mut(|mut ops: Mut<MenuUiOps>| {
                            ops.0.push(MenuUiOp::SetInputText { menu_name, text });
                        })
                },
            );
    }
}

#[derive(Message, Debug, Clone)]
pub struct OpenMenu(pub String);

#[derive(Message, Debug, Clone)]
pub struct CloseMenu(pub String);

#[derive(Message, Debug, Clone)]
pub struct ToggleMenu(pub String);

#[derive(Component)]
pub struct MenuRoot {
    pub name: String,
    pub script: Option<Handle<ScriptAsset>>,
    pub ready_sent: bool,
}

#[derive(Component)]
struct MenuTextInput;

#[derive(Component, Default)]
struct LastMenuTextValue(String);

#[derive(Resource, Default)]
pub struct MenuManager {
    open_menus: Vec<(String, Entity)>,
}

impl MenuManager {
    pub fn is_open(&self, name: &str) -> bool {
        self.open_menus.iter().any(|(n, _)| n == name)
    }

    pub fn any_open(&self) -> bool {
        !self.open_menus.is_empty()
    }

    pub fn entity_for(&self, name: &str) -> Option<Entity> {
        self.open_menus
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, e)| *e)
    }

    pub fn top(&self) -> Option<(&str, Entity)> {
        self.open_menus
            .last()
            .map(|(name, entity)| (name.as_str(), *entity))
    }
}

#[derive(Resource, Default)]
struct MenuUiOps(Vec<MenuUiOp>);

enum MenuUiOp {
    CloseTop,
    ClearChildren {
        menu_name: String,
        container_id: String,
    },
    SpawnComponent {
        menu_name: String,
        container_id: String,
        relative_template: String,
        props: Vec<(String, String)>,
    },
    SetProperty {
        menu_name: String,
        container_id: String,
        index: usize,
        key: String,
        value: String,
    },
    SetChildText {
        menu_name: String,
        container_id: String,
        index: usize,
        target_id: String,
        text: String,
    },
    SetChildRichText {
        menu_name: String,
        container_id: String,
        index: usize,
        target_id: String,
        segments: Vec<(String, Color)>,
        font_size: f32,
    },
    SetInputText {
        menu_name: String,
        text: String,
    },
}

fn get_base_dir(folder: &str) -> PathBuf {
    let p = Path::new(folder);
    if p.exists() {
        p.to_path_buf()
    } else if Path::new("..").join(folder).exists() {
        Path::new("..").join(folder)
    } else if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        Path::new(&manifest).join(folder)
    } else {
        p.to_path_buf()
    }
}

pub fn resolve_asset_path(relative_path: &str) -> String {
    let rel = Path::new(relative_path);
    if get_base_dir("mods").join(rel).is_file() {
        format!("mods://{relative_path}")
    } else {
        relative_path.to_string()
    }
}

pub fn resolve_menu_file(menu_name: &str, relative_file: &str) -> String {
    resolve_asset_path(&format!("menus/{menu_name}/{relative_file}"))
}

fn menu_asset_exists(relative_path: &str) -> bool {
    let rel = Path::new(relative_path);
    get_base_dir("mods").join(rel).is_file() || get_base_dir("assets").join(rel).is_file()
}

fn find_menu_root<'a>(
    entity: Entity,
    q_parents: &Query<&ChildOf>,
    q_roots: &'a Query<&MenuRoot>,
) -> Option<(Entity, &'a MenuRoot)> {
    if let Ok(root) = q_roots.get(entity) {
        return Some((entity, root));
    }
    for ancestor in q_parents.iter_ancestors(entity) {
        if let Ok(root) = q_roots.get(ancestor) {
            return Some((ancestor, root));
        }
    }
    None
}

fn find_ui_id_in_menu(
    root: Entity,
    target_id: &str,
    q_children: &Query<&Children>,
    q_ui_ids: &Query<&UiId>,
) -> Option<Entity> {
    for child in q_children.iter_descendants(root) {
        if let Ok(ui_id) = q_ui_ids.get(child)
            && ui_id.id() == target_id
        {
            return Some(child);
        }
    }
    None
}

fn prevent_text_input_keys(mut event: On<FocusedInput<KeyboardInput>>) {
    match event.input.key_code {
        KeyCode::Tab | KeyCode::ArrowUp | KeyCode::ArrowDown => {
            event.propagate(false);
        }
        _ => {}
    }
}

fn strip_menu_nav_text_edits(mut q_editable: Query<&mut EditableText, With<MenuTextInput>>) {
    for mut editable in &mut q_editable {
        editable
            .pending_edits
            .retain(|edit| !matches!(edit, TextEdit::Up(_) | TextEdit::Down(_)));
    }
}

fn setup_menu_hui(
    server: Res<AssetServer>,
    mut html_comps: HtmlComponents,
    mut html_funcs: HtmlFunctions,
) {
    for base_name in ["assets", "mods"] {
        let menu_base = get_base_dir(base_name).join("menus");
        let Ok(menu_dirs) = std::fs::read_dir(&menu_base) else {
            continue;
        };
        for entry in menu_dirs.flatten() {
            let comp_dir = entry.path().join("components");
            let Ok(comp_files) = std::fs::read_dir(comp_dir) else {
                continue;
            };
            for comp in comp_files.flatten() {
                let path = comp.path();
                if path.extension().and_then(|e| e.to_str()) == Some("html")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    let menu_folder = entry.file_name();
                    let rel_path = format!(
                        "menus/{}/components/{}",
                        menu_folder.to_string_lossy(),
                        path.file_name().unwrap().to_string_lossy()
                    );
                    let template: Handle<HtmlTemplate> = server.load(resolve_asset_path(&rel_path));
                    html_comps.register(stem.to_string(), template);
                }
            }
        }
    }

    html_funcs.register(
        "text_input",
        |In(entity): In<Entity>, mut commands: Commands| {
            commands
                .entity(entity)
                .insert((
                    MenuTextInput,
                    LastMenuTextValue::default(),
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
                    InteractionDisabled,
                    AutoFocus,
                ))
                .observe(prevent_text_input_keys)
                .observe(on_menu_text_edit_change);
        },
    );

    html_funcs.register(
        "menu_close",
        |In(_entity): In<Entity>,
         mut commands: Commands,
         mut manager: ResMut<MenuManager>,
         mut input_focus: ResMut<InputFocus>| {
            close_top_menu(&mut commands, &mut manager, &mut input_focus);
        },
    );

    html_funcs.register(
        "lua_press",
        |In(entity): In<Entity>,
         q_tags: Query<&Tags>,
         q_parents: Query<&ChildOf>,
         q_roots: Query<&MenuRoot>,
         mut callbacks: MessageWriter<ScriptCallbackMessage>| {
            let Some((menu_entity, root)) = find_menu_root(entity, &q_parents, &q_roots) else {
                return;
            };
            let Some(script) = root.script.clone() else {
                return;
            };

            let mut tag_map = HashMap::default();
            if let Ok(tags) = q_tags.get(entity) {
                for (k, v) in tags.tags() {
                    tag_map.insert(k.clone(), ScriptValue::String(v.clone().into()));
                }
            }

            callbacks.write(ScriptCallbackMessage::new_for_script_entity(
                OnMenuAction::into_callback_label(),
                vec![
                    ScriptValue::String(root.name.clone().into()),
                    ScriptValue::Map(tag_map),
                ],
                script,
                menu_entity,
            ));
        },
    );

    html_funcs.register(
        "lua_spawn",
        |In(entity): In<Entity>,
         q_tags: Query<&Tags>,
         q_parents: Query<&ChildOf>,
         q_roots: Query<&MenuRoot>,
         mut callbacks: MessageWriter<ScriptCallbackMessage>| {
            let Some((menu_entity, root)) = find_menu_root(entity, &q_parents, &q_roots) else {
                return;
            };
            let Some(script) = root.script.clone() else {
                return;
            };

            let mut tag_map = HashMap::default();
            if let Ok(tags) = q_tags.get(entity) {
                for (k, v) in tags.tags() {
                    tag_map.insert(k.clone(), ScriptValue::String(v.clone().into()));
                }
            }
            tag_map.insert(
                "target".to_string(),
                ScriptValue::String(format!("@{}", entity.to_bits()).into()),
            );

            callbacks.write(ScriptCallbackMessage::new_for_script_entity(
                OnMenuAction::into_callback_label(),
                vec![
                    ScriptValue::String(root.name.clone().into()),
                    ScriptValue::Map(tag_map),
                ],
                script,
                menu_entity,
            ));
        },
    );
}

fn reload_menus_on_asset_change(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut manager: ResMut<MenuManager>,
    mut input_focus: ResMut<InputFocus>,
    mut ops: ResMut<MenuUiOps>,
    mut html_events: MessageReader<AssetEvent<HtmlTemplate>>,
    mut script_events: MessageReader<AssetEvent<ScriptAsset>>,
) {
    let html_modified = html_events
        .read()
        .any(|e| matches!(e, AssetEvent::Modified { .. }));
    let script_modified = script_events
        .read()
        .any(|e| matches!(e, AssetEvent::Modified { .. }));

    if (!html_modified && !script_modified) || manager.open_menus.is_empty() {
        return;
    }

    let open_names: Vec<String> = manager
        .open_menus
        .iter()
        .map(|(name, _)| name.clone())
        .collect();

    for (_, root) in manager.open_menus.drain(..) {
        if let Ok(mut entity_cmd) = commands.get_entity(root) {
            entity_cmd.despawn();
        }
    }
    input_focus.clear();
    ops.0.clear();

    for name in open_names {
        open_menu(&mut commands, &server, &mut manager, &name);
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
        let Some(script) = root.script.clone() else {
            root.ready_sent = true;
            continue;
        };

        let ui_built = q_children
            .get(menu_entity)
            .is_ok_and(|children| !children.is_empty());
        if !ui_built {
            continue;
        }

        let attachment = ScriptAttachment::EntityScript(menu_entity, script.clone());
        let script_loaded = script_contexts
            .read()
            .get_context(&attachment)
            .is_some_and(|c| c.as_loaded().is_some());
        if !script_loaded {
            continue;
        }

        root.ready_sent = true;
        callbacks.write(ScriptCallbackMessage::new_for_script_entity(
            OnMenuReady::into_callback_label(),
            vec![ScriptValue::String(root.name.clone().into())],
            script,
            menu_entity,
        ));
    }
}

fn on_menu_text_edit_change(
    event: On<TextEditChange>,
    mut q_editable: Query<(&EditableText, &mut LastMenuTextValue)>,
    q_parents: Query<&ChildOf>,
    q_roots: Query<&MenuRoot>,
    mut callbacks: MessageWriter<ScriptCallbackMessage>,
) {
    let target = event.event_target();
    let Ok((editable, mut last_value)) = q_editable.get_mut(target) else {
        return;
    };
    let value = editable.value().to_string();
    if last_value.0 == value {
        return;
    }
    last_value.0 = value.clone();

    let Some((menu_entity, root)) = find_menu_root(target, &q_parents, &q_roots) else {
        return;
    };
    if !root.ready_sent {
        return;
    }
    let Some(script) = root.script.clone() else {
        return;
    };

    callbacks.write(ScriptCallbackMessage::new_for_script_entity(
        OnMenuTextChanged::into_callback_label(),
        vec![
            ScriptValue::String(root.name.clone().into()),
            ScriptValue::String(value.into()),
        ],
        script,
        menu_entity,
    ));
}

fn forward_menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    manager: Res<MenuManager>,
    q_roots: Query<&MenuRoot>,
    mut callbacks: MessageWriter<ScriptCallbackMessage>,
) {
    let Some((_menu_name, menu_entity)) = manager.top() else {
        return;
    };
    let Ok(root) = q_roots.get(menu_entity) else {
        return;
    };
    let Some(script) = root.script.clone() else {
        return;
    };

    for (code, label) in [
        (KeyCode::ArrowDown, "ArrowDown"),
        (KeyCode::ArrowUp, "ArrowUp"),
        (KeyCode::Tab, "Tab"),
        (KeyCode::Enter, "Enter"),
    ] {
        if keys.just_pressed(code) {
            callbacks.write(ScriptCallbackMessage::new_for_script_entity(
                OnMenuKey::into_callback_label(),
                vec![
                    ScriptValue::String(root.name.clone().into()),
                    ScriptValue::String(label.into()),
                ],
                script.clone(),
                menu_entity,
            ));
        }
    }
}

fn handle_menu_messages(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut manager: ResMut<MenuManager>,
    mut input_focus: ResMut<InputFocus>,
    mut open_reader: MessageReader<OpenMenu>,
    mut close_reader: MessageReader<CloseMenu>,
    mut toggle_reader: MessageReader<ToggleMenu>,
) {
    for msg in toggle_reader.read() {
        if manager.is_open(&msg.0) {
            close_menu(&mut commands, &mut manager, &mut input_focus, &msg.0);
        } else {
            open_menu(&mut commands, &server, &mut manager, &msg.0);
        }
    }

    for msg in open_reader.read() {
        if !manager.is_open(&msg.0) {
            open_menu(&mut commands, &server, &mut manager, &msg.0);
        }
    }

    for msg in close_reader.read() {
        close_menu(&mut commands, &mut manager, &mut input_focus, &msg.0);
    }
}

fn apply_menu_ui_ops(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut ops: ResMut<MenuUiOps>,
    mut manager: ResMut<MenuManager>,
    mut input_focus: ResMut<InputFocus>,
    q_children: Query<&Children>,
    q_ui_ids: Query<&UiId>,
    mut q_editable: Query<&mut EditableText>,
    mut q_template_props: Query<&mut TemplateProperties>,
    mut q_nodes: Query<&mut Node>,
    mut q_bg: Query<&mut BackgroundColor>,
    mut q_text: Query<&mut Text>,
) {
    if ops.0.is_empty() {
        return;
    }

    let mut deferred = Vec::new();
    let mut modified_containers: Vec<(String, String)> = Vec::new();

    for op in ops.0.drain(..) {
        match op {
            MenuUiOp::CloseTop => {
                close_top_menu(&mut commands, &mut manager, &mut input_focus);
            }
            MenuUiOp::ClearChildren {
                menu_name,
                container_id,
            } => {
                let Some(root) = manager.entity_for(&menu_name) else {
                    continue;
                };
                let Some(container) =
                    find_ui_id_in_menu(root, &container_id, &q_children, &q_ui_ids)
                else {
                    deferred.push(MenuUiOp::ClearChildren {
                        menu_name,
                        container_id,
                    });
                    continue;
                };
                modified_containers.push((menu_name, container_id));
                if let Ok(children) = q_children.get(container) {
                    for child in children.iter() {
                        if let Ok(mut entity_cmd) = commands.get_entity(child) {
                            entity_cmd.despawn();
                        }
                    }
                }
            }
            MenuUiOp::SpawnComponent {
                menu_name,
                container_id,
                relative_template,
                props,
            } => {
                let Some(root) = manager.entity_for(&menu_name) else {
                    continue;
                };
                let container = if let Some(bits_str) = container_id.strip_prefix('@') {
                    bits_str
                        .parse::<u64>()
                        .ok()
                        .and_then(Entity::try_from_bits)
                } else {
                    find_ui_id_in_menu(root, &container_id, &q_children, &q_ui_ids)
                };
                let Some(container) = container else {
                    deferred.push(MenuUiOp::SpawnComponent {
                        menu_name,
                        container_id,
                        relative_template,
                        props,
                    });
                    continue;
                };
                modified_containers.push((menu_name.clone(), container_id.clone()));

                let mut display = Display::Flex;
                let mut bg_color = Color::NONE;
                let mut template_props = TemplateProperties::default();
                for (k, v) in props {
                    if k == "display" && v == "none" {
                        display = Display::None;
                    } else if k == "bg"
                        && let Ok(srgba) = Srgba::hex(&v)
                    {
                        bg_color = Color::from(srgba);
                    }
                    template_props.set(&k, &v);
                }

                let path = resolve_menu_file(&menu_name, &relative_template);
                let is_direct_entity = container_id.starts_with('@');
                if let Ok(mut entity_cmd) = commands.get_entity(container) {
                    if is_direct_entity {
                        entity_cmd.with_child((
                            Node::default(),
                            HtmlNode(server.load(path)),
                            template_props,
                        ));
                    } else {
                        entity_cmd.with_children(|parent| {
                            parent
                                .spawn((
                                    Node {
                                        width: Val::Percent(100.0),
                                        display,
                                        flex_direction: FlexDirection::Column,
                                        border_radius: BorderRadius::all(CornerRadius::circular(Val::Px(4.0))),
                                        ..default()
                                    },
                                    BackgroundColor(bg_color),
                                ))
                                .with_child((
                                    Node {
                                        width: Val::Percent(100.0),
                                        ..default()
                                    },
                                    HtmlNode(server.load(path)),
                                    template_props,
                                ));
                        });
                    }
                }
            }
            MenuUiOp::SetProperty {
                menu_name,
                container_id,
                index,
                key,
                value,
            } => {
                let Some(root) = manager.entity_for(&menu_name) else {
                    continue;
                };
                if modified_containers
                    .iter()
                    .any(|(m, c)| m == &menu_name && c == &container_id)
                {
                    deferred.push(MenuUiOp::SetProperty {
                        menu_name,
                        container_id,
                        index,
                        key,
                        value,
                    });
                    continue;
                }
                let Some(container) =
                    find_ui_id_in_menu(root, &container_id, &q_children, &q_ui_ids)
                else {
                    deferred.push(MenuUiOp::SetProperty {
                        menu_name,
                        container_id,
                        index,
                        key,
                        value,
                    });
                    continue;
                };
                let Ok(children) = q_children.get(container) else {
                    continue;
                };
                let Some(&row_entity) = children.get(index) else {
                    continue;
                };

                if key == "bg" {
                    if let Ok(srgba) = Srgba::hex(&value) {
                        let color = Color::from(srgba);
                        if let Ok(mut bg) = q_bg.get_mut(row_entity) {
                            if bg.0 != color {
                                bg.0 = color;
                            }
                        } else if let Ok(mut entity_cmd) = commands.get_entity(row_entity) {
                            entity_cmd.insert(BackgroundColor(color));
                        }
                    }
                } else if key == "display" {
                    let target_display = if value == "none" {
                        Display::None
                    } else {
                        Display::Flex
                    };
                    if let Ok(mut node) = q_nodes.get_mut(row_entity)
                        && node.display != target_display
                    {
                        node.display = target_display;
                    }
                } else if let Ok(mut props) = q_template_props.get_mut(row_entity) {
                    props.set(&key, &value);
                }
            }
            MenuUiOp::SetChildText {
                menu_name,
                container_id,
                index,
                target_id,
                text,
            } => {
                let Some(root) = manager.entity_for(&menu_name) else {
                    continue;
                };
                if modified_containers
                    .iter()
                    .any(|(m, c)| m == &menu_name && c == &container_id)
                {
                    deferred.push(MenuUiOp::SetChildText {
                        menu_name,
                        container_id,
                        index,
                        target_id,
                        text,
                    });
                    continue;
                }
                let Some(container) =
                    find_ui_id_in_menu(root, &container_id, &q_children, &q_ui_ids)
                else {
                    deferred.push(MenuUiOp::SetChildText {
                        menu_name,
                        container_id,
                        index,
                        target_id,
                        text,
                    });
                    continue;
                };
                let Some(row_entity) = q_children
                    .get(container)
                    .ok()
                    .and_then(|c| c.get(index).copied())
                else {
                    continue;
                };
                let Some(target) = find_ui_id_in_menu(row_entity, &target_id, &q_children, &q_ui_ids)
                else {
                    deferred.push(MenuUiOp::SetChildText {
                        menu_name,
                        container_id,
                        index,
                        target_id,
                        text,
                    });
                    continue;
                };
                if let Ok(mut text_comp) = q_text.get_mut(target)
                    && text_comp.0 != text
                {
                    text_comp.0 = text;
                }
            }
            MenuUiOp::SetChildRichText {
                menu_name,
                container_id,
                index,
                target_id,
                segments,
                font_size,
            } => {
                let Some(root) = manager.entity_for(&menu_name) else {
                    continue;
                };
                if modified_containers
                    .iter()
                    .any(|(m, c)| m == &menu_name && c == &container_id)
                {
                    deferred.push(MenuUiOp::SetChildRichText {
                        menu_name,
                        container_id,
                        index,
                        target_id,
                        segments,
                        font_size,
                    });
                    continue;
                }
                let Some(container) =
                    find_ui_id_in_menu(root, &container_id, &q_children, &q_ui_ids)
                else {
                    deferred.push(MenuUiOp::SetChildRichText {
                        menu_name,
                        container_id,
                        index,
                        target_id,
                        segments,
                        font_size,
                    });
                    continue;
                };
                let Some(row_entity) = q_children
                    .get(container)
                    .ok()
                    .and_then(|c| c.get(index).copied())
                else {
                    continue;
                };
                let Some(target) = find_ui_id_in_menu(row_entity, &target_id, &q_children, &q_ui_ids)
                else {
                    deferred.push(MenuUiOp::SetChildRichText {
                        menu_name,
                        container_id,
                        index,
                        target_id,
                        segments,
                        font_size,
                    });
                    continue;
                };

                if let Ok(children) = q_children.get(target) {
                    for child in children.iter() {
                        if let Ok(mut entity_cmd) = commands.get_entity(child) {
                            entity_cmd.despawn();
                        }
                    }
                }
                if let Ok(mut target_cmd) = commands.get_entity(target) {
                    target_cmd.with_children(|parent| {
                        parent
                            .spawn((
                                Text::default(),
                                TextFont::from_font_size(font_size),
                                TextLayout {
                                    linebreak: LineBreak::NoWrap,
                                    ..default()
                                },
                            ))
                            .with_children(|text_parent| {
                                for (seg_text, seg_color) in &segments {
                                    text_parent.spawn((
                                        TextSpan::new(seg_text.clone()),
                                        TextFont::from_font_size(font_size),
                                        TextColor(*seg_color),
                                    ));
                                }
                            });
                    });
                }
            }
            MenuUiOp::SetInputText { menu_name, text } => {
                if let Some(root) = manager.entity_for(&menu_name) {
                    for desc in q_children.iter_descendants(root) {
                        if let Ok(mut editable) = q_editable.get_mut(desc) {
                            editable.queue_edit(TextEdit::SelectAll);
                            editable.queue_edit(TextEdit::Insert(text.into()));
                            break;
                        }
                    }
                }
            }
        }
    }

    ops.0.extend(deferred);
}

fn close_menu(
    commands: &mut Commands,
    manager: &mut MenuManager,
    input_focus: &mut InputFocus,
    name: &str,
) {
    if let Some(pos) = manager.open_menus.iter().rposition(|(n, _)| n == name) {
        let (_, root) = manager.open_menus.remove(pos);
        if let Ok(mut entity_cmd) = commands.get_entity(root) {
            entity_cmd.despawn();
        }
        if manager.open_menus.is_empty() {
            input_focus.clear();
        }
    }
}

fn close_top_menu(
    commands: &mut Commands,
    manager: &mut MenuManager,
    input_focus: &mut InputFocus,
) {
    if let Some((_, root)) = manager.open_menus.pop() {
        if let Ok(mut entity_cmd) = commands.get_entity(root) {
            entity_cmd.despawn();
        }
        if manager.open_menus.is_empty() {
            input_focus.clear();
        }
    }
}

fn close_top_menu_on_escape(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut manager: ResMut<MenuManager>,
    mut input_focus: ResMut<InputFocus>,
) {
    if keys.just_pressed(KeyCode::Escape) && !manager.open_menus.is_empty() {
        close_top_menu(&mut commands, &mut manager, &mut input_focus);
    }
}

fn open_menu(
    commands: &mut Commands,
    server: &AssetServer,
    manager: &mut MenuManager,
    name: &str,
) {
    let index_path = resolve_menu_file(name, &format!("{name}_index.html"));
    let script_rel = format!("menus/{name}/{name}_script.lua");
    let script_handle = if menu_asset_exists(&script_rel) {
        Some(server.load(resolve_asset_path(&script_rel)))
    } else {
        warn!("Menu script not found at assets/{script_rel} or mods/{script_rel}");
        None
    };
    let z_index = 100 + manager.open_menus.len() as i32;

    let mut entity_cmds = commands.spawn((
        MenuRoot {
            name: name.to_string(),
            script: script_handle.clone(),
            ready_sent: false,
        },
        GlobalZIndex(z_index),
        HtmlNode(server.load(index_path)),
    ));

    if let Some(handle) = script_handle {
        entity_cmds.insert(ScriptComponent(vec![handle]));
    }

    let root = entity_cmds.id();
    manager.open_menus.push((name.to_string(), root));
}