//! Read-only projection of every live binding, rebuilt from the ECS and exposed to Lua.
//!
//! The ECS (binding entities + their owning actions) is the source of truth; this registry
//! is a derived cache that exists so the Lua menu layer can read all hotkeys in one call
//! instead of walking entities.

use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_enhanced_input::prelude::*;

use super::{ActionContext, HotkeyLabel, MovementBinding};
use crate::input::keys::binding_key_and_mods;

/// One row of [`HotkeyRegistry`]: a binding plus the display/state metadata the
/// keyboard overlay needs.
#[derive(Clone, Debug)]
pub struct HotkeyEntry {
    pub key: String,
    pub label: String,
    pub context: String,
    pub active: bool,
    pub is_movement: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[derive(Resource, Default)]
pub struct HotkeyRegistry {
    pub entries: Vec<HotkeyEntry>,
}

/// Rebuild [`HotkeyRegistry`] whenever bindings, their owners, labels, contexts or
/// movement markers change.
///
/// `Changed<T>` also fires on insertion, so `Added<T>` arms would be redundant.
pub(super) fn sync_hotkey_registry(
    q_changed: Query<
        (),
        Or<(
            Changed<Binding>,
            Changed<BindingOf>,
            Changed<HotkeyLabel>,
            Changed<ActionContext>,
            Changed<MovementBinding>,
        )>,
    >,
    mut removed_bindings: RemovedComponents<Binding>,
    mut removed_labels: RemovedComponents<HotkeyLabel>,
    q_bindings: Query<(
        &Binding,
        &BindingOf,
        Option<&HotkeyLabel>,
        Option<&MovementBinding>,
    )>,
    q_actions: Query<(Option<&HotkeyLabel>, Option<&ActionContext>)>,
    mut registry: ResMut<HotkeyRegistry>,
) {
    if q_changed.is_empty()
        && removed_bindings.read().next().is_none()
        && removed_labels.read().next().is_none()
    {
        return;
    }

    let mut entries = Vec::new();
    for (binding, binding_of, binding_label, movement) in &q_bindings {
        let Some((key_str, mod_keys)) = binding_key_and_mods(binding) else {
            continue;
        };

        let (action_label, action_ctx) = q_actions
            .get(binding_of.get())
            .ok()
            .unwrap_or((None, None));
        let label = binding_label
            .or(action_label)
            .map(|l| l.0)
            .unwrap_or("Bound");
        let (context, active) = action_ctx.map_or(("unknown", true), |c| (c.name, c.active));

        entries.push(HotkeyEntry {
            key: key_str,
            label: label.to_string(),
            context: context.to_string(),
            active,
            is_movement: movement.is_some(),
            ctrl: mod_keys.contains(ModKeys::CONTROL),
            shift: mod_keys.contains(ModKeys::SHIFT),
            alt: mod_keys.contains(ModKeys::ALT),
            super_key: mod_keys.contains(ModKeys::SUPER),
        });
    }
    registry.entries = entries;
}
