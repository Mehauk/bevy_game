//! Cross-cutting infrastructure: the registries (console commands, deferred menu
//! declarations) and Lua value helpers.

pub mod dev;
pub mod registry;
pub mod script;

pub use script::{script_map, script_str, script_str_list};

pub use dev::DevUtilsPlugin;
