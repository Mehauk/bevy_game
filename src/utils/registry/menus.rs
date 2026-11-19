//! Deferred menu registration.
//!
//! Declaring a menu ends up calling `register_command` and spawning hotkey observers,
//! which needs the command registry and the menu system's messages to already exist —
//! so menu-declaring plugins had to be added *after* `MenuPlugin`, an invisible ordering
//! rule every new menu had to know.
//!
//! [`MenuRegistry`] decouples *declaring* from *registering*: any plugin's `build()` may
//! queue a menu regardless of plugin order, and `MenuPlugin::build` drains the queue
//! once its own messages exist. The drain still runs at build time, so registration
//! behaves exactly as if the declarer were a plugin added after `MenuPlugin`.
//!
//! This module is deliberately free of any knowledge of what a menu is — it stores
//! opaque `App` configurations. The typed seam is `crate::menus::MenuAppExt::register_menu`.
//! `MenuRegistryPlugin` must be added before the first plugin that queues.

use bevy::prelude::*;

/// One deferred `App` configuration, queued by [`MenuRegistryExt::queue_menu_registration`].
/// `Send + Sync` because it lives inside a `Resource`; the queued closures only capture
/// `MenuSpec` data, which satisfies both.
type PendingRegistration = Box<dyn FnOnce(&mut App) + Send + Sync>;

/// Queue of menu registrations awaiting `MenuPlugin::build`.
#[derive(Resource, Default)]
pub struct MenuRegistry {
    pending: Vec<PendingRegistration>,
}

impl MenuRegistry {
    /// Take every queued registration, in queue order.
    pub fn take_pending(&mut self) -> Vec<PendingRegistration> {
        std::mem::take(&mut self.pending)
    }
}

pub trait MenuRegistryExt {
    /// Queue an `App` configuration to run when `MenuPlugin` builds.
    ///
    /// Prefer `crate::menus::MenuAppExt::register_menu`, which queues a typed menu
    /// declaration; this raw form exists so the queue itself stays menu-agnostic.
    fn queue_menu_registration(
        &mut self,
        register: impl FnOnce(&mut App) + Send + Sync + 'static,
    ) -> &mut Self;
}

impl MenuRegistryExt for App {
    fn queue_menu_registration(
        &mut self,
        register: impl FnOnce(&mut App) + Send + Sync + 'static,
    ) -> &mut Self {
        self.world_mut()
            .resource_mut::<MenuRegistry>()
            .pending
            .push(Box::new(register));
        self
    }
}

/// Installs [`MenuRegistry`]. Must be added before any plugin that queues a menu.
pub struct MenuRegistryPlugin;

impl Plugin for MenuRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuRegistry>();
    }
}
