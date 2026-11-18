mod character;
mod input;
mod menus;
mod utils;
mod terrain;

use bevy::{
    asset::io::AssetSourceBuilder,
    dev_tools::fps_overlay::FpsOverlayPlugin,
    prelude::*,
};
use bevy_flair::prelude::*;
use bevy_mod_scripting::BMSPlugin;

use character::CharacterPlugin;
use input::InputPlugin;
use menus::{
    MenuPlugin, command_menu::CommandMenuPlugin, keyboard_menu::KeyboardMenuPlugin,
};
use terrain::TerrainPlugin;
use utils::{DevUtilsPlugin, commands::CommandRegistryPlugin};

fn main() {
    let _ = std::fs::create_dir_all("mods");

    App::new()
        .register_asset_source("mods", AssetSourceBuilder::platform_default("mods", None))
        .add_plugins((
            DefaultPlugins,
            FlairPlugin,
            BMSPlugin,
            FpsOverlayPlugin::default(),
            CommandRegistryPlugin,
            InputPlugin,
            MenuPlugin,
            CommandMenuPlugin,
            KeyboardMenuPlugin,
            DevUtilsPlugin,
            CharacterPlugin,
            TerrainPlugin,
        ))
        .run();
}