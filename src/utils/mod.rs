//! Cross-cutting infrastructure: the command registry, shared world facts, and Lua value helpers.

pub mod dev;
pub mod profiler;
pub mod registry;
pub mod script;
pub mod world;

pub use script::{script_map, script_str, script_str_list};
pub use world::SpawnGroundHeight;

pub use dev::DevUtilsPlugin;
pub use profiler::ProfilerPlugin;