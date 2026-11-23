mod camera;
mod character;
mod commands;
mod dev;
mod docgen;
mod input;
mod integration;
mod profiler;
mod screenshot;
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
use bevy_script_menus::{MenuAssetRoot, MenuConfig, MenuPlugin};

use camera::CameraPlugin;
use character::CharacterPlugin;
use commands::CommandsPlugin;
use dev::DevUtilsPlugin;
use docgen::DocgenPlugin;
use input::InputPlugin;
use integration::IntegrationPlugin;
use profiler::ProfilerPlugin;
use screenshot::ScreenshotPlugin;
use state::StatePlugin;
use terrain::TerrainPlugin;

fn main() {
    let _ = std::fs::create_dir_all("mods");

    App::new()
        .register_asset_source("mods", AssetSourceBuilder::platform_default("mods", None))
        .insert_resource(MenuConfig {
            roots: vec![
                MenuAssetRoot::new("mods", "mods://"),
                MenuAssetRoot::new("assets", ""),
            ],
            ..default()
        })
        .add_plugins((
            DefaultPlugins,
            BrpExtrasPlugin::default(),
            PhysicsPlugins::default(),
            FlairPlugin,
            BMSPlugin,
            FpsOverlayPlugin::default(),
            CommandsPlugin,
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
            ScreenshotPlugin,
            DocgenPlugin,
            IntegrationPlugin,
        ))
        .run();
}