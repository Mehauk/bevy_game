use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_enhanced_input::prelude::*;

use super::HotkeyLabel;
use crate::utils::keys::{binding_key_and_mods, parse_input_code};
use crate::utils::RemapStatus;

#[derive(Message, Clone, Debug)]
pub struct RemapBindingMessage {
    pub source_key: String,
    pub source_mods: ModKeys,
    pub target_key: String,
    pub target_mods: ModKeys,
}

#[derive(Clone)]
struct BindingSnapshot {
    entity: Entity,
    key: String,
    mods: ModKeys,
    label: Option<String>,
}

impl BindingSnapshot {
    fn display_label(&self) -> String {
        self.label.clone().unwrap_or_else(|| self.key.clone())
    }
}

pub(super) fn handle_remap_bindings(
    mut messages: MessageReader<RemapBindingMessage>,
    q_bindings: Query<(Entity, &Binding, &BindingOf, Option<&HotkeyLabel>)>,
    q_actions: Query<Option<&HotkeyLabel>>,
    mut commands: Commands,
    mut remap_status: ResMut<RemapStatus>,
) {
    for msg in messages.read() {
        let binding_snapshots: Vec<BindingSnapshot> = q_bindings
            .iter()
            .filter_map(|(e, b, b_of, lbl)| {
                let (key, mods) = binding_key_and_mods(b)?;
                let action_label = q_actions.get(b_of.get()).ok().flatten();
                Some(BindingSnapshot {
                    entity: e,
                    key,
                    mods,
                    label: lbl.or(action_label).map(|l| l.0.to_string()),
                })
            })
            .collect();

        let find = |key: &str, mods: ModKeys, exclude: Option<Entity>| {
            binding_snapshots.iter().find(|s| {
                Some(s.entity) != exclude
                    && s.key.eq_ignore_ascii_case(key)
                    && s.mods == mods
            })
        };

        let Some(source) = find(&msg.source_key, msg.source_mods, None).cloned() else {
            remap_status.set_error(format!("Source binding '{}' not found", msg.source_key));
            continue;
        };

        let Some(target_code) = parse_input_code(&msg.target_key) else {
            remap_status.set_error(format!("Unknown key or button '{}'", msg.target_key));
            continue;
        };

        let target = find(&msg.target_key, msg.target_mods, Some(source.entity)).cloned();
        let source_label = source.display_label();

        let result = match target {
            None => {
                commands
                    .entity(source.entity)
                    .insert(target_code.to_binding(msg.target_mods));
                Ok(format!("Remapped '{source_label}' to '{}'", msg.target_key))
            }
            Some(target) => match parse_input_code(&source.key) {
                Some(source_code) => {
                    commands
                        .entity(source.entity)
                        .insert(target_code.to_binding(target.mods));
                    commands
                        .entity(target.entity)
                        .insert(source_code.to_binding(source.mods));
                    Ok(format!(
                        "Swapped '{source_label}' ({}) <-> '{}' ({})",
                        source.key,
                        target.display_label(),
                        target.key
                    ))
                }
                None => Err(format!("Unknown key or button '{}'", source.key)),
            },
        };

        match result {
            Ok(message) => {
                info!("{message}");
                remap_status.set_success(message);
            }
            Err(message) => {
                warn!("{message}");
                remap_status.set_error(message);
            }
        }
    }
}
