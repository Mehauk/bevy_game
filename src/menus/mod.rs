pub mod command_menu;

use std::path::{Path, PathBuf};

use bevy::{
    input::keyboard::KeyboardInput,
    input_focus::{AutoFocus, FocusedInput, InputFocus, tab_navigation::TabIndex},
    platform::collections::HashMap,
    prelude::*,
    text::{EditableText, LineBreak, TextCursorStyle, TextEdit, TextEditChange},
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
                "set_child_background",
                |ctx: FunctionCallContext,
                 menu_name: String,
                 container_id: String,
                 index: i64,
                 hex_color: String|
                 -> Result<(), InteropError> {
                    if index < 0 {
                        return Ok(());
                    }
                    ctx.world()?
                        .with_resource_mut(|mut ops: Mut<MenuUiOps>| {
                            ops.0.push(MenuUiOp::SetChildBackground {
                                menu_name,
                                container_id,
                                index: index as usize,
                                hex_color,
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
    SetChildBackground {
        menu_name: String,
        container_id: String,
        index: usize,
        hex_color: String,
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
                    TabIndex(0),
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
    q_editable: Query<&EditableText>,
    q_parents: Query<&ChildOf>,
    q_roots: Query<&MenuRoot>,
    mut callbacks: MessageWriter<ScriptCallbackMessage>,
) {
    let target = event.event_target();
    let Ok(editable) = q_editable.get(target) else {
        return;
    };
    let Some((menu_entity, root)) = find_menu_root(target, &q_parents, &q_roots) else {
        return;
    };
    if !root.ready_sent {
        return;
    }
    let Some(script) = root.script.clone() else {
        return;
    };

    let value = editable.value().to_string();
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
) {
    if ops.0.is_empty() {
        return;
    }

    let mut deferred = Vec::new();
    let mut cleared_containers = Vec::new();

    for op in ops.0.drain(..) {
        match op {
            MenuUiOp::CloseTop => {
                close_top_menu(&mut commands, &mut manager, &mut input_focus);
            }
            MenuUiOp::ClearChildren {
                menu_name,
                container_id,
            } => {
                cleared_containers.push((menu_name.clone(), container_id.clone()));
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
                let Some(container) =
                    find_ui_id_in_menu(root, &container_id, &q_children, &q_ui_ids)
                else {
                    deferred.push(MenuUiOp::SpawnComponent {
                        menu_name,
                        container_id,
                        relative_template,
                        props,
                    });
                    continue;
                };
                let mut template_props = TemplateProperties::default();
                for (k, v) in props {
                    template_props.set(&k, &v);
                }
                let path = resolve_menu_file(&menu_name, &relative_template);
                if let Ok(mut entity_cmd) = commands.get_entity(container) {
                    entity_cmd.with_child((
                        Node {
                            width: Val::Percent(100.0),
                            ..default()
                        },
                        HtmlNode(server.load(path)),
                        template_props,
                    ));
                }
            }
            MenuUiOp::SetChildBackground {
                menu_name,
                container_id,
                index,
                hex_color,
            } => {
                if cleared_containers
                    .iter()
                    .any(|(m, c)| m == &menu_name && c == &container_id)
                {
                    continue;
                }
                let Some(root) = manager.entity_for(&menu_name) else {
                    continue;
                };
                let Some(container) =
                    find_ui_id_in_menu(root, &container_id, &q_children, &q_ui_ids)
                else {
                    deferred.push(MenuUiOp::SetChildBackground {
                        menu_name,
                        container_id,
                        index,
                        hex_color,
                    });
                    continue;
                };
                if let Ok(children) = q_children.get(container)
                    && let Some(&child) = children.get(index)
                    && let Ok(srgba) = Srgba::hex(&hex_color)
                {
                    let target = q_children
                        .get(child)
                        .ok()
                        .and_then(|c| c.first().copied())
                        .unwrap_or(child);

                    if let Ok(mut entity_cmd) = commands.get_entity(target) {
                        entity_cmd.insert(BackgroundColor(srgba.into()));
                    }
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