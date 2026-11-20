mod camera;
mod character;
mod input;
mod integration;
mod menus;
mod state;
mod terrain;
mod utils;

use avian3d::prelude::*;
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
use integration::IntegrationPlugin;
use menus::MenuPlugin;
use state::StatePlugin;
use terrain::TerrainPlugin;
use utils::{
    DevUtilsPlugin,
    ProfilerPlugin,
    registry::commands::CommandRegistryPlugin,
};

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
            // Plugin order is load-bearing: a plugin that registers a command needs the
            // command registry's resource to exist first, because `register_system`
            // initialises its `SystemParam`s immediately. `IntegrationPlugin` is added
            // last — it wires the feature modules together, so it needs all of them built.
            //   CommandRegistryPlugin  -> anything using `command!` or `register_command`
            CommandRegistryPlugin,
            InputPlugin,
            MenuPlugin,
            StatePlugin,
            DevUtilsPlugin,
            ProfilerPlugin,
            CameraPlugin,
            CharacterPlugin,
            TerrainPlugin,
            IntegrationPlugin,
        ))
        .run();
}