//! Key/button parsing, canonical key names, and modifier-flag conversion.
//!
//! This is the single source of truth for the string form of an input that crosses the
//! Lua boundary (see `docs/lua-hooks.md` "Key names"): [`binding_key_and_mods`] produces
//! it, [`parse_input_code`] consumes it, and [`modifier_map`] / [`parse_mod_keys`] do the
//! same round trip for modifier sets.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_mod_scripting::bindings::script_value::ScriptValue;

use crate::utils::script_map;

/// A bindable physical input, in the subset the remapper understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputCode {
    Key(KeyCode),
    Mouse(MouseButton),
    MouseWheel,
    MouseMotion,
}

impl InputCode {
    pub fn to_binding(&self, mod_keys: ModKeys) -> Binding {
        match *self {
            InputCode::Key(key) => Binding::Keyboard { key, mod_keys },
            InputCode::Mouse(button) => Binding::MouseButton { button, mod_keys },
            InputCode::MouseWheel => Binding::MouseWheel { mod_keys },
            InputCode::MouseMotion => Binding::MouseMotion { mod_keys },
        }
    }
}

pub fn parse_input_code(code: &str) -> Option<InputCode> {
    match code {
        "MouseLeft" | "Left" | "MouseButton1" => Some(InputCode::Mouse(MouseButton::Left)),
        "MouseRight" | "Right" | "MouseButton2" => Some(InputCode::Mouse(MouseButton::Right)),
        "MouseMiddle" | "Middle" | "MouseButton3" => Some(InputCode::Mouse(MouseButton::Middle)),
        "MouseBack" | "Back" | "MouseButton4" => Some(InputCode::Mouse(MouseButton::Back)),
        "MouseForward" | "Forward" | "MouseButton5" => {
            Some(InputCode::Mouse(MouseButton::Forward))
        }
        "MouseWheel" => Some(InputCode::MouseWheel),
        "MouseMotion" => Some(InputCode::MouseMotion),
        key_str => parse_key_code(key_str).map(InputCode::Key),
    }
}

/// Generates `string => KeyCode` arms from bare variant names, so the parse string
/// can never drift from the variant it maps to.
macro_rules! parse_key_variants {
    ($s:expr, $($variant:ident),+ $(,)?) => {
        match $s {
            $(stringify!($variant) => Some(KeyCode::$variant),)+
            _ => None,
        }
    };
}

pub fn parse_key_code(s: &str) -> Option<KeyCode> {
    parse_key_variants!(s,
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM,
        KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
        Escape, Space, Enter, Tab, Backspace, CapsLock,
        ShiftLeft, ShiftRight, ControlLeft, ControlRight,
        AltLeft, AltRight, SuperLeft, SuperRight,
        Backquote, Minus, Equal, BracketLeft, BracketRight, Backslash,
        Semicolon, Quote, Comma, Period, Slash,
        ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    )
}

/// Canonical display name for a mouse button, shared by binding serialization and
/// the menu input dispatcher. `Other(4)`/`Other(5)` are treated as Back/Forward.
pub fn mouse_button_name(button: MouseButton) -> Option<&'static str> {
    match button {
        MouseButton::Left => Some("MouseLeft"),
        MouseButton::Right => Some("MouseRight"),
        MouseButton::Middle => Some("MouseMiddle"),
        MouseButton::Back | MouseButton::Other(4) => Some("MouseBack"),
        MouseButton::Forward | MouseButton::Other(5) => Some("MouseForward"),
        MouseButton::Other(_) => None,
    }
}

/// Canonical `(key name, modifiers)` for a binding, or `None` for bindings the
/// remapper cannot express (gamepad, etc.).
pub fn binding_key_and_mods(binding: &Binding) -> Option<(String, ModKeys)> {
    match binding {
        Binding::Keyboard { key, mod_keys } => Some((format!("{key:?}"), *mod_keys)),
        Binding::MouseButton { button, mod_keys } => {
            let s = mouse_button_name(*button).map_or_else(
                || match *button {
                    MouseButton::Other(n) => format!("MouseButton{n}"),
                    _ => "MouseButton".to_string(),
                },
                str::to_string,
            );
            Some((s, *mod_keys))
        }
        Binding::MouseWheel { mod_keys } => Some(("MouseWheel".to_string(), *mod_keys)),
        Binding::MouseMotion { mod_keys } => Some(("MouseMotion".to_string(), *mod_keys)),
        _ => None,
    }
}

/// The four modifiers, in exactly one table: the Lua-facing name, the `ModKeys` flag, and
/// the physical left/right key codes.
///
/// Everything that speaks "modifiers" — the Lua map shape, the Lua parser, and physical-key
/// polling — iterates this, so adding a modifier is a one-line change instead of three
/// synchronised edits in three places.
pub const MODIFIER_KEYS: [(&str, ModKeys, [KeyCode; 2]); 4] = [
    (
        "ctrl",
        ModKeys::CONTROL,
        [KeyCode::ControlLeft, KeyCode::ControlRight],
    ),
    (
        "shift",
        ModKeys::SHIFT,
        [KeyCode::ShiftLeft, KeyCode::ShiftRight],
    ),
    ("alt", ModKeys::ALT, [KeyCode::AltLeft, KeyCode::AltRight]),
    (
        "super",
        ModKeys::SUPER,
        [KeyCode::SuperLeft, KeyCode::SuperRight],
    ),
];

/// The `{ ctrl, shift, alt, super }` entries on their own, so a caller can embed modifiers
/// in a larger payload instead of hand-rolling the same four booleans.
pub fn modifier_entries(
    mods: ModKeys,
) -> impl Iterator<Item = (&'static str, ScriptValue)> + Clone {
    MODIFIER_KEYS
        .iter()
        .map(move |(name, flag, _)| (*name, ScriptValue::Bool(mods.contains(*flag))))
}

pub fn modifier_map(mods: ModKeys) -> ScriptValue {
    script_map(modifier_entries(mods))
}

/// Inverse of [`modifier_map`]: read a modifier set back out of a Lua map.
pub fn parse_mod_keys(val: &ScriptValue) -> ModKeys {
    let mut mods = ModKeys::empty();
    if let ScriptValue::Map(map) = val {
        for (name, flag, _) in MODIFIER_KEYS {
            if map.get(name).is_some_and(|v| matches!(v, ScriptValue::Bool(true))) {
                mods |= flag;
            }
        }
    }
    mods
}

/// Modifier flags currently held down on the physical keyboard.
pub fn current_modifier_flags(keys: &ButtonInput<KeyCode>) -> ModKeys {
    let mut mods = ModKeys::empty();
    for (_, flag, codes) in MODIFIER_KEYS {
        if keys.any_pressed(codes) {
            mods |= flag;
        }
    }
    mods
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_round_trip() {
        // The Lua side echoes back exactly the names we serialise out, so any name that does
        // not parse back is a remap that would silently fail at runtime.
        for name in [
            "KeyA", "KeyZ", "Digit0", "Digit9", "F1", "F12", "Escape", "Space", "Enter", "Tab",
            "Backspace", "CapsLock", "ShiftLeft", "ControlRight", "AltLeft", "SuperRight",
            "Backquote", "Minus", "Equal", "BracketLeft", "BracketRight", "Backslash",
            "Semicolon", "Quote", "Comma", "Period", "Slash", "ArrowUp", "ArrowDown",
            "ArrowLeft", "ArrowRight",
        ] {
            let code = parse_key_code(name).unwrap_or_else(|| panic!("`{name}` must parse"));
            assert_eq!(format!("{code:?}"), name, "Debug form must equal the variant name");
        }
        assert_eq!(parse_key_code("NotAKey"), None);
    }

    #[test]
    fn mouse_names_round_trip_including_aliases() {
        for (name, button) in [
            ("MouseLeft", MouseButton::Left),
            ("MouseRight", MouseButton::Right),
            ("MouseMiddle", MouseButton::Middle),
            ("MouseBack", MouseButton::Back),
            ("MouseForward", MouseButton::Forward),
        ] {
            assert_eq!(mouse_button_name(button), Some(name));
            assert_eq!(parse_input_code(name), Some(InputCode::Mouse(button)));
        }
        // Aliases the bundled Lua scripts rely on.
        assert_eq!(parse_input_code("Left"), Some(InputCode::Mouse(MouseButton::Left)));
        assert_eq!(
            parse_input_code("MouseButton4"),
            Some(InputCode::Mouse(MouseButton::Back))
        );
        assert_eq!(parse_input_code("MouseWheel"), Some(InputCode::MouseWheel));
        assert_eq!(parse_input_code("MouseMotion"), Some(InputCode::MouseMotion));
    }

    #[test]
    fn modifier_map_and_parser_are_inverses() {
        for mods in [
            ModKeys::empty(),
            ModKeys::CONTROL,
            ModKeys::SHIFT | ModKeys::CONTROL,
            ModKeys::ALT | ModKeys::SHIFT | ModKeys::CONTROL | ModKeys::SUPER,
        ] {
            assert_eq!(
                parse_mod_keys(&modifier_map(mods)),
                mods,
                "what we send to Lua must parse back unchanged"
            );
        }
    }

    #[test]
    fn modifier_map_has_exactly_the_documented_keys() {
        let ScriptValue::Map(map) = modifier_map(ModKeys::SHIFT) else {
            panic!("modifier_map must return a map");
        };
        let mut names: Vec<&str> = map.keys().map(String::as_str).collect();
        names.sort();
        assert_eq!(names, vec!["alt", "ctrl", "shift", "super"]);
        assert!(matches!(map.get("shift"), Some(ScriptValue::Bool(true))));
        assert!(matches!(map.get("ctrl"), Some(ScriptValue::Bool(false))));
    }

    #[test]
    fn binding_serialisation_round_trips() {
        let binding = Binding::Keyboard {
            key: KeyCode::KeyF,
            mod_keys: ModKeys::SHIFT,
        };
        let (key, mods) = binding_key_and_mods(&binding).expect("keyboard binding");
        assert_eq!(key, "KeyF");
        assert_eq!(mods, ModKeys::SHIFT);

        let rebuilt = parse_input_code(&key).unwrap().to_binding(mods);
        assert_eq!(binding_key_and_mods(&rebuilt).unwrap(), (key, mods));
    }
}
