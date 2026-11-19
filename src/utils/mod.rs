//! Cross-cutting infrastructure: the console command registry and Lua value helpers.

pub mod commands;
pub mod dev;
pub mod script;

pub use script::{script_map, script_str, script_str_list};

pub use dev::DevUtilsPlugin;
