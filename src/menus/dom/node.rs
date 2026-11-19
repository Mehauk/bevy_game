//! Translating a menu's HTML/`dom.create` description into real UI entities.
//!
//! [`NodeSpec`] is the neutral description produced by either source (XML attribute or
//! Lua options map); [`spawn_node_entity`] is the one place that decides which Bevy
//! components a tag maps to.

use bevy::{
    input_focus::AutoFocus,
    picking::hover::Hovered,
    picking::Pickable,
    prelude::*,
    text::{EditableText, LineBreak, TextCursorStyle},
    ui::InteractionDisabled,
    ui_widgets::{Button as UiButton, TextInput},
};
use bevy_flair::prelude::*;
use bevy_mod_scripting::bindings::script_value::ScriptValue;

use super::{DomHandle, DomHandlers};

/// A menu node before it becomes an entity: tag, `id`, classes, text and interaction flags.
#[derive(Clone, Debug, Default)]
pub(crate) struct NodeSpec {
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub text: String,
    pub autofocus: bool,
    pub mouse_input: bool,
}

impl NodeSpec {
    /// Default spec for a tag: the tag is stored lowercased, and `input`/`button` opt in to
    /// mouse picking unless a caller overrides it.
    fn defaults_for_tag(tag: &str) -> Self {
        let tag = tag.to_ascii_lowercase();
        let mouse_input = matches!(tag.as_str(), "input" | "button");
        Self {
            tag,
            mouse_input,
            ..default()
        }
    }

    pub(crate) fn from_xml(node: roxmltree::Node) -> Self {
        let tag = node.tag_name().name().to_ascii_lowercase();
        let attr = |k| node.attribute(k);
        let text = attr("text").map(str::to_string).unwrap_or_else(|| {
            if matches!(tag.as_str(), "text" | "span") {
                node.children()
                    .filter_map(|c| c.is_text().then(|| c.text()).flatten())
                    .collect::<String>()
                    .trim()
                    .to_string()
            } else {
                String::new()
            }
        });

        let mouse_input = match attr("mouse") {
            Some("true") => true,
            Some("false") => false,
            _ => matches!(tag.as_str(), "input" | "button"),
        };

        Self {
            tag,
            id: attr("id").map(str::to_string),
            classes: attr("class").map(split_classes).unwrap_or_default(),
            text,
            autofocus: attr("autofocus") == Some("true"),
            mouse_input,
        }
    }

    pub(crate) fn from_script_opts(tag: String, opts: ScriptValue) -> Self {
        let mut spec = Self::defaults_for_tag(&tag);
        let ScriptValue::Map(map) = opts else {
            return spec;
        };
        let string_field = |key: &str| {
            map.get(key)
                .and_then(|v| match v {
                    ScriptValue::String(s) => Some(s.to_string()),
                    _ => None,
                })
        };
        let bool_field = |key: &str| {
            map.get(key).and_then(|v| match v {
                ScriptValue::Bool(b) => Some(*b),
                _ => None,
            })
        };

        if let Some(id) = string_field("id") {
            spec.id = Some(id);
        }
        if let Some(class) = string_field("class") {
            spec.classes = split_classes(&class);
        }
        if let Some(text) = string_field("text") {
            spec.text = text;
        }
        if let Some(autofocus) = bool_field("autofocus") {
            spec.autofocus = autofocus;
        }
        if let Some(mouse) = bool_field("mouse") {
            spec.mouse_input = mouse;
        }
        spec
    }
}

fn split_classes(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_string).collect()
}

/// Text input marker; the inner string caches the last emitted value so `text_change`
/// only fires on an actual change.
#[derive(Component, Default)]
pub(crate) struct MenuTextInput(pub String);

/// Spawn the entity for one DOM node and wire up its observers.
///
/// `id` is the Lua-facing DOM id; `spec.tag` decides the component set.
pub(crate) fn spawn_node_entity(
    world: &mut World,
    id: i64,
    parent: Entity,
    spec: NodeSpec,
) -> Entity {
    let mut e = world.spawn((DomHandle(id), DomHandlers::default(), ChildOf(parent)));
    if let Some(name) = spec.id.clone() {
        e.insert(Name::new(name));
    }
    if !spec.classes.is_empty() {
        e.insert(ClassList::new_with_classes(spec.classes));
    }

    let is_input = spec.tag == "input";
    let is_interactive = is_input || spec.mouse_input || spec.id.is_some() || spec.tag == "button";

    match spec.tag.as_str() {
        "text" => {
            e.insert((Text::new(spec.text), TextLayout::default(), Pickable::IGNORE));
        }
        "span" => {
            e.insert((TextSpan::new(spec.text), Pickable::IGNORE));
        }
        "button" => {
            e.insert((Node::default(), UiButton, Hovered::default()));
        }
        "input" => {
            e.insert((
                Node::default(),
                MenuTextInput::default(),
                TextInput,
                EditableText {
                    cursor_width: 0.3,
                    ..default()
                },
                TextLayout {
                    linebreak: LineBreak::NoWrap,
                    ..default()
                },
                TextCursorStyle {
                    color: Color::WHITE,
                    selection_color: Color::srgba(0.3, 0.5, 0.9, 0.4),
                    ..default()
                },
            ));
            if !spec.mouse_input {
                e.insert(InteractionDisabled);
            }
            if spec.autofocus {
                e.insert(AutoFocus);
            }
        }
        _ => {
            e.insert((Node::default(), Hovered::default()));
        }
    }

    if is_interactive {
        e.observe(super::events::on_dom_click)
            .observe(super::events::on_dom_hover_enter)
            .observe(super::events::on_dom_hover_exit);
    }

    if is_input {
        e.observe(super::events::on_dom_key_input)
            .observe(super::events::on_dom_text_change);
    }

    e.id()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{script_map, script_str};

    fn opts(entries: Vec<(&'static str, ScriptValue)>) -> ScriptValue {
        script_map(entries)
    }

    #[test]
    fn tag_is_lowercased_and_drives_the_mouse_default() {
        // Regression: the mouse default must be computed from the *lowercased* tag, so a
        // script may write `dom.create(p, "INPUT", {})` and still get a picking input.
        for tag in ["input", "INPUT", "Input"] {
            let spec = NodeSpec::from_script_opts(tag.into(), ScriptValue::Unit);
            assert_eq!(spec.tag, "input", "tag must be stored lowercased");
            assert!(spec.mouse_input, "`{tag}` must default to mouse-interactive");
        }
        assert!(NodeSpec::from_script_opts("BUTTON".into(), ScriptValue::Unit).mouse_input);
        assert!(!NodeSpec::from_script_opts("node".into(), ScriptValue::Unit).mouse_input);
    }

    #[test]
    fn explicit_mouse_option_overrides_the_tag_default() {
        let on = NodeSpec::from_script_opts(
            "node".into(),
            opts(vec![("mouse", ScriptValue::Bool(true))]),
        );
        assert!(on.mouse_input);

        let off = NodeSpec::from_script_opts(
            "input".into(),
            opts(vec![("mouse", ScriptValue::Bool(false))]),
        );
        assert!(!off.mouse_input);
    }

    #[test]
    fn reads_id_class_text_and_autofocus() {
        let spec = NodeSpec::from_script_opts(
            "text".into(),
            opts(vec![
                ("id", script_str("row")),
                ("class", script_str("row selected")),
                ("text", script_str("hello")),
                ("autofocus", ScriptValue::Bool(true)),
            ]),
        );
        assert_eq!(spec.id.as_deref(), Some("row"));
        assert_eq!(spec.classes, vec!["row".to_string(), "selected".to_string()]);
        assert_eq!(spec.text, "hello");
        assert!(spec.autofocus);
    }

    #[test]
    fn wrongly_typed_options_are_ignored() {
        let spec = NodeSpec::from_script_opts(
            "input".into(),
            opts(vec![("id", ScriptValue::Bool(true)), ("mouse", script_str("false"))]),
        );
        assert_eq!(spec.id, None, "a non-string id must be ignored");
        assert!(spec.mouse_input, "a non-bool `mouse` must not flip the default");
    }

    #[test]
    fn classes_split_on_any_whitespace() {
        assert_eq!(split_classes("  a  b\tc "), vec!["a", "b", "c"]);
        assert!(split_classes("").is_empty());
    }

    #[test]
    fn from_xml_reads_attributes_and_text_children() {
        let doc = roxmltree::Document::parse("<text id=\"t1\" class=\"a b\">Hello</text>").unwrap();
        let spec = NodeSpec::from_xml(doc.root_element());
        assert_eq!(spec.tag, "text");
        assert_eq!(spec.id.as_deref(), Some("t1"));
        assert_eq!(spec.classes, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(spec.text, "Hello");
    }

    #[test]
    fn from_xml_text_attribute_wins_over_children() {
        let doc = roxmltree::Document::parse("<text text=\"explicit\">ignored</text>").unwrap();
        assert_eq!(NodeSpec::from_xml(doc.root_element()).text, "explicit");
    }

    #[test]
    fn from_xml_only_reads_implicit_text_for_text_and_span() {
        let doc = roxmltree::Document::parse("<node>ignored</node>").unwrap();
        assert_eq!(NodeSpec::from_xml(doc.root_element()).text, "");
    }
}
