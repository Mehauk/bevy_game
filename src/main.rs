mod camera;
mod character;
mod dev;
mod docgen;
mod input;
mod integration;
mod menus;
mod profiler;
mod state;
mod terrain;
mod utils;

use avian3d::prelude::*;
use bevy::{
    asset::io::AssetSourceBuilder,
    dev_tools::fps_overlay::FpsOverlayPlugin,
    prelude::*,
};
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_flair::prelude::*;
use bevy_mod_scripting::BMSPlugin;

use camera::CameraPlugin;
use character::CharacterPlugin;
use dev::DevUtilsPlugin;
use docgen::DocgenPlugin;
use input::InputPlugin;
use integration::IntegrationPlugin;
use menus::MenuPlugin;
use profiler::ProfilerPlugin;
use state::StatePlugin;
use terrain::TerrainPlugin;
use utils::registry::commands::CommandRegistryPlugin;

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
            BrpExtrasPlugin::default(),
            CommandRegistryPlugin,
            InputPlugin,
            MenuPlugin,
        ))
        .add_plugins((
            StatePlugin,
            DevUtilsPlugin,
            ProfilerPlugin,
            CameraPlugin,
            CharacterPlugin,
            TerrainPlugin,
            DocgenPlugin,
            IntegrationPlugin,
        ))
        .run();
}