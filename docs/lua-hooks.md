# Lua ↔ Engine Hooks

Menu scripts (`assets/menus/<menu>/script.lua`, or `mods/menus/<menu>/script.lua`) talk to the
engine through four namespaces obtained with `import_namespace(...)`, and receive engine
callbacks as global functions.

```lua
local dom      = import_namespace("dom")
local input    = import_namespace("input")
local commands = import_namespace("commands")
local menus    = import_namespace("menus")
```

## Key names

Keys are canonical strings. Keyboard keys use the `KeyCode` variant names:
`KeyA`–`KeyZ`, `Digit0`–`Digit9`, `F1`–`F12`, `Escape`, `Space`, `Enter`, `Tab`, `Backspace`,
`CapsLock`, `ShiftLeft`/`ShiftRight`, `ControlLeft`/`ControlRight`, `AltLeft`/`AltRight`,
`SuperLeft`/`SuperRight`, `Backquote`, `Minus`, `Equal`, `BracketLeft`/`BracketRight`,
`Backslash`, `Semicolon`, `Quote`, `Comma`, `Period`, `Slash`, `ArrowUp`/`ArrowDown`/`ArrowLeft`/`ArrowRight`.

Mouse inputs: `MouseLeft`, `MouseRight`, `MouseMiddle`, `MouseBack`, `MouseForward`,
`MouseWheel`, `MouseMotion`.

Modifier sets are maps: `{ ctrl = false, shift = false, alt = false, super = false }`.

## Engine → Lua callbacks

| Callback | When it fires |
| --- | --- |
| `on_menu_ready(menu_name, root_id)` | Once, when the menu's script has finished loading. `root_id` is the DOM id of the menu root node. |
| `on_dom_event(handler, node_id, payload)` | For every event bound with `dom.on`. Dispatch by `handler` name. |
| `on_modifiers_changed(mods, pressed_list, just_pressed_list)` | While any menu is open, whenever modifier or pressed-key state changes. `mods` is a modifier map, the lists contain key-name strings. |
| `on_remap_result(success, message)` | After an `input.remap` attempt resolves. |

`on_dom_event` payload by event type:

| Event | Payload |
| --- | --- |
| `click` | none |
| `hover_enter` / `hover_exit` | none |
| `key_down` | `{ key, ctrl, shift, alt, super, repeat }` |
| `text_change` | the new text string |

## `dom` namespace

Operates on the menu's DOM (ids are integers returned by `get_by_id`/`create`/`html`).

| Function | Description |
| --- | --- |
| `dom.get_by_id(scope_id, id)` | Find a node by its HTML `id` within the menu of `scope_id`. Returns the node id or `nil`. |
| `dom.create(parent_id, tag, opts)` | Create a node under `parent_id`. `opts`: `{ id, class, text, autofocus, mouse }`. Returns the new node id. |
| `dom.html(parent_id, html)` | Parse an HTML/XML snippet and queue it under `parent_id`. Returns the root node id of the subtree. |
| `dom.clear(id)` | Remove all children of a node. |
| `dom.remove(id)` | Remove a node (and its subtree). |
| `dom.set_text(id, text)` | Set the text of a `text`/`span` node. |
| `dom.set_value(id, text)` | Replace the contents of an `input` node. |
| `dom.add_class(id, cls)` / `dom.remove_class(id, cls)` | Toggle a CSS class (styling via bevy_flair `ClassList`). |
| `dom.has_class(id, cls)` | Returns `true`/`false`. |
| `dom.on(id, event, handler)` | Bind `click`, `hover_enter`, `hover_exit`, `key_down`, or `text_change` to a global function name, dispatched through `on_dom_event`. `"press"` is accepted as an alias for `"click"`. |
| `dom.focus(id)` / `dom.blur()` | Move / clear keyboard focus. |

Supported tags: `node` (plain container), `text`, `span`, `button`, `input`
(`autofocus="true"` grabs focus when the menu opens; `mouse="false"` disables pointer picking).

## `input` namespace

| Function | Description |
| --- | --- |
| `input.list_bindings()` | List of all bound actions: `{ key, label, context, active, is_movement, ctrl, shift, alt, super }`. `context` is `"gameplay"`, `"menu"`, or `"unknown"`. |
| `input.active_modifiers()` | Modifier map of physically held modifiers. |
| `input.remap(source_key, source_mods, target_key, target_mods)` | Request a remap (or swap, if the target is already bound). Keybinding rules (gameplay-only, Aim/Look lock, movement-key modifier/chain restrictions) are enforced by the menu script. Result arrives via `on_remap_result`. |
| `input.remap_status()` | `{ success, message, version }` of the last remap. |

## `commands` namespace

| Function | Description |
| --- | --- |
| `commands.list()` | List of registered commands: `{ name, description }`. Names are `>`-separated module paths, e.g. `character>respawn_character`. |
| `commands.run(text)` | Run a command by full name, suffix name (last `>` segment), or `name arg1 arg2`. Args are available to the command system via `CommandArgs`. |

## `menus` namespace

| Function | Description |
| --- | --- |
| `menus.open(name)` | Open a registered menu (`assets/menus/<name>/`). |
| `menus.close(name)` | Close one instance of a menu. |
| `menus.close_top()` | Close the topmost open menu. |

## Registering new menus and commands (Rust side)

- A menu is registered with `app.register_menu_with_hotkey::<Action>(MenuSpec { id, hotkey_label, command, bindings })`
  (see `src/menus/keyboard_menu.rs`); `id` selects the `menus/<id>/` asset folder, `command` optionally
  exposes it as a console command.
- Console commands are registered with the `command!(app, "description", system_path)` macro
  (see `src/utils/commands.rs`); any `IntoSystem<(), (), _>` system qualifies and can read
  `CommandArgs` for parameters.
