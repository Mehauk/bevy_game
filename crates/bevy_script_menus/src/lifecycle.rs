use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy_flair::prelude::*;
use bevy_mod_scripting::{
    asset::ScriptAsset,
    bindings::script_value::ScriptValue,
    core::{
        event::{CallbackLabel, IntoCallbackLabel, ScriptCallbackMessage},
        script::{ScriptComponent, ScriptContexts},
    },
    script::ScriptAttachment,
};

use crate::MenuScriptingPlugin;
use crate::OnMenuReady;
use crate::assets::resolve_menu_file;
use crate::config::MenuConfig;
use crate::dom::{
    DomHandle, DomHandlers, DomRuntime, apply_dom_ops, prune_dead_nodes, queue_xml_subtree,
};
use crate::focus::MenuFocusState;
use crate::state::{MenuCmd, MenuManager, ToggleMenu};

#[derive(Component)]
pub struct MenuRoot {
    pub(crate) name: String,
    pub(crate) dom_id: i64,
    pub(crate) html_path: std::path::PathBuf,
    pub(crate) html_modified: Option<std::time::SystemTime>,
    pub(crate) script: Option<Handle<ScriptAsset>>,
    pub(crate) ready_sent: bool,
}

impl MenuRoot {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dom_id(&self) -> i64 {
        self.dom_id
    }

    pub fn script(&self) -> Option<Handle<ScriptAsset>> {
        self.script.clone()
    }

    pub fn ready_script(&self) -> Option<Handle<ScriptAsset>> {
        self.ready_sent.then(|| self.script.clone()).flatten()
    }
}

pub fn broadcast_menu_callback(
    roots: &Query<(Entity, &MenuRoot)>,
    callbacks: &mut MessageWriter<ScriptCallbackMessage>,
    label: CallbackLabel,
    payload: &[ScriptValue],
) {
    for (entity, root) in roots.iter() {
        let Some(script) = root.ready_script() else {
            continue;
        };
        callbacks.write(ScriptCallbackMessage::new_for_script_entity(
            label.clone(),
            payload.to_vec(),
            script,
            entity,
        ));
    }
}

pub(crate) fn handle_menu_commands(
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
                    close_menu_by_name(world, &name);
                }
                MenuCmd::Close(None) => {
                    close_top_menu(world);
                }
            }
        }
        for name in toggle_list {
            if !close_menu_by_name(world, &name) {
                open_menu_in_world(world, &name);
            }
        }
    });
}

pub(crate) fn dispatch_menu_ready(
    mut q_roots: Query<(Entity, &mut MenuRoot)>,
    script_contexts: Res<ScriptContexts<MenuScriptingPlugin>>,
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

fn close_menu_at(world: &mut World, pos: usize) {
    let (_, root) = world.resource_mut::<MenuManager>().remove_at(pos);
    if let Ok(e) = world.get_entity_mut(root) {
        e.despawn();
    }
    prune_dead_nodes(world);
    if !world.resource::<MenuManager>().any_open() {
        world.resource_mut::<InputFocus>().clear();
        world.resource_mut::<MenuFocusState>().last_focused = None;
    }
}

pub(crate) fn close_top_menu(world: &mut World) -> bool {
    let Some(top) = world.resource::<MenuManager>().depth().checked_sub(1) else {
        return false;
    };
    close_menu_at(world, top);
    true
}

fn close_menu_by_name(world: &mut World, name: &str) -> bool {
    let Some(pos) = world.resource::<MenuManager>().index_of(name) else {
        return false;
    };
    close_menu_at(world, pos);
    true
}

pub(crate) fn open_menu_in_world(world: &mut World, name: &str) {
    let config = world.resource::<MenuConfig>().clone();
    let Some((html_path, _)) = resolve_menu_file(&config, &format!("menus/{name}/index.html"))
    else {
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
    let css_handle = resolve_menu_file(&config, &format!("menus/{name}/style.css"))
        .map(|(_, asset)| asset_server.load(asset));
    let script_handle = resolve_menu_file(&config, &format!("menus/{name}/script.lua"))
        .map(|(_, asset)| asset_server.load(asset));
    if script_handle.is_none() {
        warn!("Menu script not found for '{name}'");
    }

    let z = config.base_z_index + world.resource::<MenuManager>().depth() as i32;
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
            DomHandlers::default(),
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
        .push(name.to_string(), root_entity);

    apply_dom_ops(world);
}
