mod camera;
mod character;
mod input;
mod menus;
mod terrain;
mod utils;

use bevy::{
    asset::io::AssetSourceBuilder,
    dev_tools::fps_overlay::FpsOverlayPlugin,
    prelude::*,
};
use bevy_flair::prelude::*;
use bevy_mod_scripting::BMSPlugin;

use camera::CameraPlugin;
use character::CharacterPlugin;
use input::InputPlugin;
use menus::{
    MenuPlugin, command_menu::CommandMenuPlugin, keyboard_menu::KeyboardMenuPlugin,
};
use terrain::TerrainPlugin;
use utils::{DevUtilsPlugin, commands::CommandRegistryPlugin};
use avian3d::prelude::*;

fn main() {
    let _ = std::fs::create_dir_all("mods");

    App::new()
        .register_asset_source("mods", AssetSourceBuilder::platform_default("mods", None))
        .add_plugins((
            DefaultPlugins,
            PhysicsPlugins::default(),
            FlairPlugin,
            BMSPlugin,
            FpsOverlayPlugin::default(),
            // Plugin order is load-bearing: a plugin that registers a command or a menu
            // requires the resource/messages its provider `build()` installs to already
            // exist, because `register_system` initialises its `SystemParam`s immediately.
            //   CommandRegistryPlugin  -> anything using `command!` or `register_command`
            //   MenuPlugin             -> the per-menu plugins (they write `ToggleMenu`)
            CommandRegistryPlugin,
            InputPlugin,
            MenuPlugin,
            CommandMenuPlugin,
            KeyboardMenuPlugin,
            DevUtilsPlugin,
            CameraPlugin,
            CharacterPlugin,
            TerrainPlugin,
        ))
        .run();
}