pub mod command_menu;

use std::path::Path;

use bevy::{input_focus::InputFocus, prelude::*};
use bevy_hui::prelude::*;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuManager>()
            .add_message::<OpenMenu>()
            .add_message::<CloseMenu>()
            .add_message::<ToggleMenu>()
            .add_systems(Startup, setup_menu_hui)
            .add_systems(Update, (handle_menu_messages, close_top_menu_on_escape));
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
    #[allow(dead_code)]
    pub name: String,
}

#[derive(Resource, Default)]
pub struct MenuManager {
    /// Stack of open menus in open order: `(menu_name, root_entity)`.
    open_menus: Vec<(String, Entity)>,
}

impl MenuManager {
    #[allow(dead_code)]
    pub fn is_open(&self, name: &str) -> bool {
        self.open_menus.iter().any(|(n, _)| n == name)
    }

    #[allow(dead_code)]
    pub fn any_open(&self) -> bool {
        !self.open_menus.is_empty()
    }
}

/// Resolves `mods/<relative_path>` if present, falling back to `assets/<relative_path>`.
pub fn resolve_asset_path(relative_path: &str) -> String {
    if Path::new("mods").join(relative_path).is_file() {
        format!("mods://{relative_path}")
    } else {
        relative_path.to_string()
    }
}

pub fn resolve_menu_file(menu_name: &str, relative_file: &str) -> String {
    resolve_asset_path(&format!("menus/{menu_name}/{relative_file}"))
}

fn setup_menu_hui(mut html_funcs: HtmlFunctions) {
    html_funcs.register(
        "menu_close",
        |In(_entity): In<Entity>,
         mut commands: Commands,
         mut manager: ResMut<MenuManager>,
         mut input_focus: ResMut<InputFocus>| {
            close_top_menu(&mut commands, &mut manager, &mut input_focus);
        },
    );
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
    let z_index = 100 + manager.open_menus.len() as i32;

    let root = commands
        .spawn((
            MenuRoot {
                name: name.to_string(),
            },
            GlobalZIndex(z_index),
            HtmlNode(server.load(index_path)),
        ))
        .id();

    manager.open_menus.push((name.to_string(), root));
}

fn close_menu(
    commands: &mut Commands,
    manager: &mut MenuManager,
    input_focus: &mut InputFocus,
    name: &str,
) {
    if let Some(pos) = manager.open_menus.iter().rposition(|(n, _)| n == name) {
        let (_, root) = manager.open_menus.remove(pos);
        commands.entity(root).despawn();
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
        commands.entity(root).despawn();
        if manager.open_menus.is_empty() {
            input_focus.clear();
        }
    }
}