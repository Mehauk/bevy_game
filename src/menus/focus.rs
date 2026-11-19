//! Keyboard focus inside menus.
//!
//! Bevy clears `InputFocus` whenever the focused entity is despawned, which happens
//! constantly while a Lua script rebuilds a list. [`track_and_maintain_menu_focus`]
//! remembers the last text field that had focus and restores it, so a script that
//! re-creates its search box on every keystroke does not lose the caret.

use bevy::{input_focus::{AutoFocus, FocusCause, InputFocus}, prelude::*, ui_widgets::TextInput};

use crate::menus::state::MenuManager;

/// The text field the menu last had keyboard focus on, if it still exists.
#[derive(Resource, Default)]
pub(crate) struct MenuFocusState {
    pub last_focused: Option<Entity>,
}

pub(crate) fn track_and_maintain_menu_focus(
    manager: Res<MenuManager>,
    mut focus: ResMut<InputFocus>,
    mut state: ResMut<MenuFocusState>,
    q_inputs: Query<Entity, With<TextInput>>,
    q_autofocus: Query<Entity, (With<TextInput>, With<AutoFocus>)>,
) {
    if !manager.any_open() {
        state.last_focused = None;
        return;
    }

    if let Some(current) = focus.get().filter(|&e| q_inputs.contains(e)) {
        state.last_focused = Some(current);
    } else if let Some(last) = state.last_focused.filter(|&e| q_inputs.contains(e)) {
        focus.set(last, FocusCause::Navigated);
    } else if let Some(af) = q_autofocus.iter().next() {
        focus.set(af, FocusCause::Auto);
        state.last_focused = Some(af);
    } else {
        state.last_focused = None;
    }
}