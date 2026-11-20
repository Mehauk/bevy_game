//! Registries that decouple *declaring* something from *registering* it, so a feature
//! plugin never has to be ordered against the subsystem that consumes its declarations.

pub mod commands;
