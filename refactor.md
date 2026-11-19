# Refactoring Log

## Standing instruction (verbatim, preserved across context compression)

> START OF PROMPT
> Im going to sleep. I want you to iteratively refactor this codebase to follow best practices, good design patterns, clean architecture to achieve grater flexibility, maintainability, and scalability.
>
> 1. Read the codebase
> 2. Formulate a well reasoned plan
> 3 Audit and update the plan until correct
> 4. Implement that plan
> 5. Audit and resolve the implementation until correct.
> 6. Reduce the implementation by taking advantage of DRY and other principles.
> 7. Record and justify your doings in refactor.md
> 7. Repeat.
>
> on context compression, preserve this message exactly.
> END OF PROMPT

---

## 0. Codebase audit (baseline)

3,191 lines of Rust across 10 files, plus a Lua/HTML menu layer (`assets/menus/`) and
`docs/lua-hooks.md` documenting the Lua↔Rust contract.

| File | LOC | Concern |
| --- | --- | --- |
| `src/menus/mod.rs` | 1176 | **god module**: menu stack, DOM runtime, HTML hot reload, focus, script dispatch, 2 Lua namespaces, DOM event bridge |
| `src/input.rs` | 608 | hotkey contexts, key parsing, hotkey registry, remapping, `input` Lua namespace |
| `src/character.rs` | 512 | character controller + 3 console commands |
| `src/camera.rs` | 332 | camera rig, cursor lock, view modes, zoom |
| `src/terrain.rs` | 234 | terrain mesh gen, atmosphere/fog |
| `src/utils/commands.rs` | 195 | command registry + `commands` Lua namespace |
| `src/utils/mod.rs` | 42 | script value helpers + dev wireframe command |
| `src/main.rs` | 47 | plugin wiring |
| `src/menus/{command,keyboard}_menu.rs` | 45 | two menu registrations |

### Findings

**F1 — `menus/mod.rs` is a god module (severity: high).**
At least 9 distinct responsibilities live in one file. Adding a DOM node type, a menu
hotkey, a Lua function, or a dispatch callback all mean editing the same 1176-line file.
This is the dominant maintainability cost in the repo.

**F2 — Duplicated "broadcast to ready menu scripts" loop (high).**
`dispatch_modifier_changes` and `dispatch_remap_results` repeat an identical
`for (entity, root) in &q_roots { if !root.ready_sent {continue}; let Some(script) = root.script.clone() else {continue}; callbacks.write(...) }`
loop; `dispatch_menu_ready` is a third variant. Any change to script-readiness semantics
must be made in three places.

**F3 — Two parallel gameplay-action spawners (high).**
`camera.rs::spawn_gameplay_action` (private, `Vec2`-capable, configurable `require_reset`,
spawns its *own* `GameplayInput` entity) duplicates `input.rs::InputAppExt::register_gameplay_action`
(`bool`-only, hardcoded `ActionSettings`, uses the `GameplayInputEntity` resource).
Result: two gameplay-context entities exist and the action-spawning rules can drift.

**F4 — `MenuManager` mixes the menu stack with modifier-dispatch caches (medium).**
`last_mods` / `last_pressed` are internal state of one dispatcher but live on the shared
resource that `camera` and `input` read to ask "is UI open?".

**F5 — Script-value construction duplicated (medium).**
`modifier_map` builds `{ctrl,shift,alt,super}` while `menus::on_dom_key_input` hand-rolls
`{key,ctrl,shift,alt,repeat}` (no `super`) — same concept, two shapes, silently divergent.
Every Lua-facing struct has a hand-written `script_map([...])` block with no shared
serialization seam.

**F6 — No tests (medium).** Zero `#[cfg(test)]`. All gameplay math (zoom clamping, pitch
clamping, accel ramping, ground-gap tolerance, key-code parsing, command name derivation,
HTML→NodeSpec) is embedded inside systems and therefore untestable.

**F7 — Local DRY violations (low).**
`set_character_speed` / `increase_character_speed` share a preamble and a report;
`toggle_atmospheric_fog` / `toggle_fog_light_influence` share a toggle-alpha idiom;
terrain colour band constants are hand-maintained to sum to 1.0 (`HILLSIDE_BAND` is
`SLOPE_BAND - VALLEY_BAND` written out by hand).

**F8 — Dead code (low).** `CommandArgs.raw` is written every command run and never read.
`Character`'s `Crouch` action is spawned, labelled, and read into `_crouch_action` but has
no effect.

**F9 — Implicit user-facing command names (low).** The `command!` macro derives the palette
name from the module path (`character>respawn_character`). Refactoring module layout
silently renames user-facing commands.

**F10 — Generated bundle committed (info).** `repomix-output.xml` (167 KB) is a generated
source snapshot that is already stale; it should be gitignored, not tracked.

### Constraints (must not break)

- **C1** The Lua API in `docs/lua-hooks.md` is a public contract: namespace/function names,
  callback names, and `ScriptValue` shapes. Two Lua scripts in `assets/menus/` depend on it.
- **C2** Console command names typed by the user are the *last* `>`-separated segment
  (`respawn`, `speed`, `toggle_view_mode`); those must stay stable.
- **C3** No runtime verification is possible (headless session, user asleep). Every step
  must keep `cargo check` green and must be behaviour-preserving by construction.
- **C4** `git HEAD` == baseline, so `git checkout -- src/` is always a valid recovery path.

---

## 1. Plan

Ordering principle: mechanical, behaviour-preserving decomposition first (low risk, high
value, makes everything after it easier), then API unification, then logic extraction with
tests, then reduction. Each stage ends with a green `cargo check`.

### Iteration 1 — Decompose by concern, kill the dispatch duplication

1.1 Split `src/menus/mod.rs` into cohesive submodules:

| New file | Contents |
| --- | --- |
| `menus/mod.rs` | `MenuPlugin`, `MenuUpdateSystems`, callback labels, `MenuSpec`, `MenuAppExt`, `ToggleMenu`/`MenuCmd`, re-exports |
| `menus/state.rs` | `MenuManager` — encapsulated menu stack (methods, not raw `Vec` access) |
| `menus/lifecycle.rs` | `MenuRoot`, open/close world fns, `handle_menu_commands`, `dispatch_menu_ready`, `CloseTopMenu` input |
| `menus/assets.rs` | `resolve_menu_file`, `HtmlHotReloadState`, `reload_menus_on_asset_change` |
| `menus/focus.rs` | `MenuFocusState`, `track_and_maintain_menu_focus` |
| `menus/dispatch.rs` | `dispatch_modifier_changes`, `dispatch_remap_results`, `ModifierTracking`, **`broadcast_to_menus`** (fixes F2) |
| `menus/dom/mod.rs` | `DomRuntime`, `DomNode`, `DomOp`, `DomHandle`, `DomHandlers`, `apply_dom_ops`, `prune_dead_nodes`, `queue_xml_subtree` |
| `menus/dom/node.rs` | `NodeSpec`, `split_classes`, `spawn_node_entity`, `MenuTextInput` |
| `menus/dom/events.rs` | `DomEventCtx` + click/hover/key/text observers, `strip_menu_nav_text_edits` |
| `menus/scripting.rs` | `register_menu_namespaces` (`dom` + `menus` namespaces) |

1.2 Split `src/input.rs` into `src/input/`: `mod.rs` (plugin + contexts + `InputAppExt`),
`keys.rs` (parsing/formatting + modifier helpers), `hotkeys.rs` (registry + sync),
`remap.rs` (remap message/status/handler), `scripting.rs` (`input` namespace).

1.3 Move `script_str`/`script_map` from `utils/mod.rs` to `utils/script.rs`; dev wireframe
command moves to `utils/dev.rs`. `utils/mod.rs` becomes wiring only.

1.4 Fix F4: `MenuManager` keeps only the stack + `any_open()`; modifier caches move to a
`ModifierTracking` resource owned by `menus/dispatch.rs`.

1.5 Fix F5 partially: one `modifier_map` used by every modifier-shaped payload (adds the
missing `super` to the `key_down` payload — additive, existing Lua ignores it).

**Verification:** `cargo check` green; grep-diff that every Lua namespace function name and
every callback label is still registered; command names unchanged.

### Iteration 2 — Unify the input-action API, extract pure tested logic

2.1 Fix F3: generalise `InputAppExt::register_gameplay_action` to any `InputAction` output
and explicit `ActionSettings`; delete `camera.rs::spawn_gameplay_action`; camera actions
attach to the shared `GameplayInputEntity` so exactly one camera-side gameplay context exists.

2.2 Fix F6: extract pure functions from systems and unit-test them —
`CameraRig::{apply_look, apply_zoom}`, character accel ramp, ground-gap tolerance,
`parse_key_code` round-trip, `CommandRegistry` lookup, `NodeSpec::from_xml`, terrain band
interpolation. Systems become thin adapters over tested logic.

2.3 Fix F7: shared command preamble/report helper; shared toggle-alpha helper; derive
terrain band widths instead of hand-copying them.

2.4 Fix F9: give `command!` an explicit-name form and use it wherever a module move would
otherwise rename a user-facing command.

### Iteration 3 — Audit, reduce, document

3.1 Re-read every changed file against the original for behaviour drift; hunt for
left-behind `pub(crate)` that can be private, unused imports, stale doc comments.
3.2 Fix F8 (dead code) — remove `CommandArgs.raw`; decide on `Crouch` (keep the binding as
a user-visible placeholder but stop reading it into an unused param, or implement crouch;
default is to leave behaviour untouched and drop the dead read).
3.3 Update `docs/lua-hooks.md` / `README.md` if any documented surface moved; record F10.
3.4 Final reduction pass: anything that became duplicated *because of* the split gets a
shared helper; anything over-abstracted gets inlined back.

### Explicitly out of scope

- Changing rendering, physics tuning, terrain look, or any gameplay feel.
- Rewriting the Lua menu scripts (they are the consumer contract, not the target).
- Introducing new crates, feature flags, or an ECS-framework abstraction layer.
- Installing toolchain components (clippy) without asking.
- **Data-driven menu registration.** `README.md` lists "a hook to read a list of menus, add
  new menus, and bind menus" as future work. Auto-registering menus by scanning
  `assets/menus/*/` would be a real scalability win, but it is a *feature* (it changes which
  hotkeys/commands exist and how they are declared), not a behaviour-preserving refactor.
  Deferred to the user. The `MenuSpec` / `MenuAppExt` seam is what such a feature would plug into.

---

## 1.1 Plan audit (corrections made before implementing)

Self-review of the plan above; each item is a gap found and closed.

- **A1 — Import churn.** Splitting modules must not force every *consumer* to change.
  Correction: `menus/mod.rs` and `input/mod.rs` re-export everything external code used
  (`pub use state::MenuManager;`, `pub use keys::{...}` etc.), so `crate::menus::MenuManager`
  and `crate::input::GameplayInput` keep resolving. Only intra-module visibility widens,
  via `pub(crate)` — never `pub` by accident.
- **A2 — `MenuRoot` field access.** Several new modules need `ready_sent`/`script`/`dom_id`.
  Correction: expose `MenuRoot::ready_script() -> Option<Handle<ScriptAsset>>` (encodes the
  "ready *and* has a script" invariant that three dispatchers currently re-implement) and keep
  the remaining fields `pub(crate)`.
- **A3 — `broadcast_to_menus` needs `ScriptValue: Clone`.** Already true: the existing
  dispatchers call `.clone()` on `ScriptValue` values. No blocker.
- **A4 — Unifying action spawning would silently change input consumption.**
  `register_gameplay_action` hardcodes `consume_input: true`; `camera.rs`'s private spawner
  uses `consume_input: false` with a per-action `require_reset`. If the unified API keeps the
  hardcoded settings, camera hotkeys would start swallowing input. Correction: the unified
  API takes `ActionSettings` explicitly, and behaviour is pinned by this table:

  | action | output | `require_reset` | `consume_input` |
  | --- | --- | --- | --- |
  | `Look` | `Vec2` | false | false |
  | `ToggleViewMode` | bool | true | false |
  | `GrabCursor` | bool | true | false |
  | `ReleaseCursor` | bool | true | false |
  | `ZoomCamera` | `Vec2` | true | false |
  | menu toggles (`MenuSpec`) | bool | true | true |
  | character `bsn!` actions | mixed | true | false (default) |

- **A5 — Camera must stop owning a second gameplay context, but ordering matters.**
  `setup_gameplay_input` runs in `PreStartup`, `setup_camera` in `Startup`, so the
  `GameplayInputEntity` resource is guaranteed to exist when camera actions are spawned.
  Removing `GameplayInput`/`ContextActive`/`ContextActivity` from the camera entity is safe
  because the actions will point at the shared entity instead.
- **A6 — Adding `super` to the `key_down` payload (F5) must be checked against both Lua
  scripts.** `command_palette/script.lua:on_search_key` reads only `ev.key`;
  `keyboard/script.lua` dispatches handlers that ignore the payload. Additive ⇒ safe.
- **A7 — Verification is weaker than a test suite.** clippy is not installed for this
  toolchain and the app cannot be run headlessly, so the safety net is: (a) `cargo check`
  after every stage with zero new warnings, (b) an explicit name-by-name diff of the Lua
  surface against `docs/lua-hooks.md`, (c) unit tests added in iteration 2 for extracted
  logic, (d) `git HEAD` as the revert point.
- **A8 — Scope discipline.** Iteration 1 is *pure movement + one dedup*. No signature changes
  to systems, no logic edits. If a change needs logic edits during iteration 1, it belongs to
  iteration 2.

---

## 2. Iteration 1 — decomposition by concern (implemented, verified)

### What changed

`src/menus/mod.rs` **1176 → 150 lines**; `src/input.rs` **608 → 268 lines**. The code moved
into files that each own one concept:

| New file | Owns |
| --- | --- |
| `menus/mod.rs` | plugin wiring, callback labels, `MenuSpec`/`MenuAppExt` |
| `menus/state.rs` | the open-menu stack (`MenuManager`), `ToggleMenu`, `MenuCmd` |
| `menus/lifecycle.rs` | open/close, `MenuRoot` + the script-ready handshake |
| `menus/dom/mod.rs` | `DomRuntime`, the `DomOp` queue, `apply_dom_ops`, pruning |
| `menus/dom/node.rs` | `NodeSpec` (HTML/Lua → components), `spawn_node_entity` |
| `menus/dom/events.rs` | pointer/key/text events → Lua handlers |
| `menus/dispatch.rs` | modifier + remap-result push, `broadcast_to_menus` |
| `menus/assets.rs` | file resolution (mods override assets), hot reload |
| `menus/focus.rs` | keyboard focus retention |
| `menus/scripting.rs` | the `dom` and `menus` namespaces |
| `input/keys.rs` | key/button parsing, canonical names, modifier flags |
| `input/hotkeys.rs` | the derived `HotkeyRegistry` |
| `input/remap.rs` | remap message/status/application |
| `input/scripting.rs` | the `input` namespace |
| `utils/script.rs` | `script_str` / `script_map` / `script_str_list` |
| `utils/dev.rs` | the dev-only wireframe command |

Duplication removed:

- **F2** — the "for each ready menu script, send this callback" loop existed three times; it is
  now `broadcast_to_menus`, and the readiness rule it encoded (`ready_sent && script.is_some()`)
  is named once as `MenuRoot::ready_script()`.
- **F4** — `MenuManager` no longer carries `last_mods`/`last_pressed`; those moved to a
  `ModifierTracking` resource owned by the only system that reads them. `MenuManager` is now a
  stack behind methods, so `camera`/`input` cannot accidentally couple to menu internals.
- **F5** — one `MODIFIER_FLAGS` table defines the modifier shape, and `modifier_entries` lets a
  payload embed modifiers instead of re-typing four booleans. The `key_down` payload gained
  `super` as a result (doc updated).

### Justification

Every split is on a seam that already existed but was implicit: the DOM op queue, the menu
stack and the script bridge are separate state machines that happened to share a file. Naming
them makes the dependency direction visible — `state` depends on nothing, `lifecycle` depends
on `state` + `dom` + `assets`, `dispatch` depends on `lifecycle` — which is what turns the next
feature (data-driven menus, a second scripting language, a HUD menu) into a local change
instead of a grep through 1176 lines.

The `pub(crate)` widening is deliberate: these modules are crate-internal, and `pub(crate)`
says so without pretending the API is public.

### Verification

- `cargo check --all-targets`: clean, **zero warnings**.
- Item inventory diffed against `git HEAD`: no function or type lost in either split.
- Lua surface diffed against `docs/lua-hooks.md`: all 22 namespace functions and all 4
  callback labels present, names and shapes unchanged.
- Two independent adversarial audits of the split against `git HEAD` (system registration
  order, Lua signatures, every moved function body, every intentional deviation).

### Audit findings and resolutions

1. **Real bug, found and fixed.** `NodeSpec::defaults_for_tag` computed `mouse_input` from the
   raw tag while storing the lowercased one, so `dom.create(p, "INPUT", {})` would have silently
   produced a non-picking text field; HEAD lowercased first. Fixed in `menus/dom/node.rs` and
   pinned by the regression test `tag_is_lowercased_and_drives_the_mouse_default`.
   *This is the class of defect a compiler cannot see and a mechanical diff does not catch.*
2. **Real regression, caught before it shipped.** Moving `DevUtilsPlugin` into `utils/dev.rs`
   would have renamed the user-facing command to `utils>dev>dev>toggle_wireframe`, because the
   macro derives the name from `module_path!()` at the call site. Fixed by flattening the nested
   `mod dev`; the audit independently re-derived both spellings and confirmed they are now
   byte-identical (`utils>dev>toggle_wireframe`).
3. **Doc drift, fixed.** `docs/lua-hooks.md` described the `key_down` payload without `super`.
4. **Confirmed non-issues**: `ResMut<MenuManager>` → `Res<MenuManager>` in the modifier
   dispatcher (scheduler parallelism only); `map(…).unwrap_or_else(…)` → `map_or_else(…)` in
   `binding_key_and_mods` (exactly equivalent); `angle_between` on the un-normalized vs.
   normalized hit normal (scale-invariant, identical).

### Deviations from the plan

- `input.rs` was kept as the module root with an `input/` directory beside it (valid Rust 2024
  layout) rather than moved to `input/mod.rs` — smaller diff, identical result.
- `utils/script.rs` gained `script_str_list` in iteration 1 (planned for iteration 2) because
  the dispatch dedup needed it immediately.

---

## 3. Iteration 2 — one way to register an action; pure tested logic (implemented, verified)

### 3.1 One gameplay-action API (fixes F3)

`camera.rs` had its own private `spawn_gameplay_action` duplicating
`InputAppExt::register_gameplay_action`, with three silent differences: it hardcoded
`consume_input: false`, took `require_reset` as a bare `bool`, and — the significant one —
spawned **its own** `GameplayInput` context entity, so the game ran with two gameplay contexts
that `sync_context_activity` had to keep in step.

There is now one entry-point pair on `InputAppExt`, split by what the observer listens for:

| | event | use |
| --- | --- | --- |
| `register_gameplay_hotkey` | `Start<A>` | one-shot toggles (menus, view mode, cursor) |
| `register_gameplay_action` | `Fire<A>` | value actions (mouse look, scroll zoom) |

Both delegate to a single `register_action` helper, so the spawn logic exists once.
`ActionSettings` is an explicit parameter rather than a default — the audit pinned the required
per-action settings and they genuinely differ (`consume_input` is `true` only for menu
hotkeys), so a default would have silently changed input consumption. Splitting on the event
type also makes it impossible to register a `Fire` observer as a one-shot hotkey; the compiler
rejects it.

The camera entity no longer carries `GameplayInput`/`ContextActive`/`ContextActivity`; its five
bindings hang off the shared `GameplayInputEntity`, which the menu hotkeys had already been
using successfully. Net: one gameplay context per concern instead of two.

### 3.2 Pure, tested logic (fixes F6)

Systems were thinned into adapters over free functions, now covered by **36 unit tests**:

| extracted | why it was worth pinning |
| --- | --- |
| `CameraRig::apply_look` | pitch clamp — a flipped camera is a classic bug |
| `CameraRig::apply_zoom` | the third↔first-person handoff had a real dead-zone risk |
| `occluded_distance` | camera-vs-geometry clip margin and floor |
| `accelerate_toward` | the snap-when-close branch is what stops asymptotic crawl |
| `jump_fires` | press-edge + grounded + cooldown, i.e. no bunny-hopping |
| `air_gravity_scale` | variable jump height |
| `ground_touch_distance` / `ground_gap` | probe-vs-capsule radius correction |
| `is_walkable` | slope limit |
| `slope_tangent_y` / `slope_snap_speed` | riding slopes and stepping down |
| `normalized_height` / `height_colour` | colour-band interpolation and edge continuity |
| `toggle_fog_alpha` | fog on/off round trip |
| `NodeSpec::from_script_opts` / `from_xml` | the Lua/HTML option contract |

The zoom test failed on first run because my *expectation* was wrong (collapsing from distance
6.0 takes three steps, not two), not because the code was. The test was corrected to assert the
real transition; behaviour is unchanged from HEAD.

### 3.3 Local DRY (fixes F7)

- `set_character_speed` / `increase_character_speed` share `with_player_character`. It takes a
  callback rather than returning `Mut<Character>`: the borrow must not escape the call, and the
  lifetime-generic version did not compile.
- Terrain band widths are now *derived* from the band edges (`HILLSIDE_BAND = SLOPE_BAND -
  VALLEY_BAND`) instead of hand-copied beside a comment claiming they matched; a test asserts
  they tile 0..1.
- `toggle_atmospheric_fog` / `toggle_fog_light_influence` share `toggle_fog_alpha`.

### Verification

`cargo check --all-targets` clean with zero warnings; `cargo test` → **36 passed, 0 failed**.

---

## 4. Iteration 3 — audit, bug fixes, reduction

### 4.1 A real latent bug fixed: command arguments were shared across a frame

`handle_run_commands` wrote the shared `CommandArgs` resource and then called
`commands.run_system(entry.system_id)`. `Commands` are applied *later*, so with two
`RunCommand` messages in one frame both systems ran **after** both writes and each observed
the *last* command's arguments:

```text
write args=A; queue run(A); write args=B; queue run(B)   →   A sees B's args
```

Reachable from any Lua script that calls `commands.run` twice in one frame (the bundled
command palette happens to run only one, which is why it never surfaced).

Fix: each run is wrapped in `commands.queue(|world| …)`, which writes `CommandArgs`
immediately before *its* `world.run_system`. A command can now only observe its own arguments.
Pinned by `two_commands_in_one_frame_each_see_their_own_args`, which fails against the HEAD
implementation (both entries would read `c,d`).

### 4.2 Dead state removed (fixes F8)

- `CommandArgs.raw` — written on every command run, never read; it was also just `args`
  re-joined.
- `CommandArgs.command` — never read; the compiler flagged it once `raw` went, so it went too.
  `CommandArgs` is now exactly `args: Vec<String>`.
- `_crouch_action: Single<&Action<Crouch>>` — a system parameter that forced a `Single<>`
  query every fixed step to read a value nothing used.

`Crouch` itself was **left in place deliberately**: the action and its Ctrl bindings are still
spawned, so `input.list_bindings()` still reports a "Crouch" row and the keyboard overlay is
unchanged. Removing it would have silently deleted a row the user can currently see. It is now
documented at its declaration as declared-but-unimplemented rather than silently dead — worth a
product decision (implement crouch, or drop the binding) that is the user's, not mine.

### 4.3 More tests

`utils/commands.rs` gained registry tests that were previously untestable because the logic
was welded to `App`:

- case-insensitive lookup; suffix lookup matches **only** the last `>` segment (what the user
  actually types); re-registering replaces rather than duplicates; the registry stays sorted;
  positional arg parsing never panics on a bad or missing index.
- `command_macro_derives_its_name_from_the_module_path` pins the derivation contract that the
  `utils/dev.rs` move nearly broke. Had this test existed during iteration 1, the
  `utils>dev>dev>toggle_wireframe` regression would have failed a build instead of needing an
  audit to notice.

### 4.4 Considered and deliberately rejected

- **Breaking the `menus ⇄ input` module cycle.** `input` needs `MenuManager::any_open()` and
  `MenuUpdateSystems`; `menus` needs `InputAppExt` and the remap/modifier helpers. The usual
  fix — a leaf `ui_state` module, or a mirrored "is UI open" boolean — either moves menu-stack
  operations out of `menus` or introduces a second source of truth that can desync. Within one
  crate this cycle is not costing anything: Rust permits it, the two modules are genuinely
  co-dependent (menus are driven by hotkeys; hotkey activity is gated by menus), and the shim
  would add indirection without adding clarity. Left as-is, documented here so it reads as a
  decision rather than an oversight.
- **Auto-registering menus by scanning `assets/menus/`** — see "out of scope"; it is a feature.
- **`#[lints]` config / `#![warn(missing_docs)]`** — noisy for a game crate; clippy is not
  installed for this toolchain and I did not install toolchain components unasked.

### 4.5 Documentation

- `docs/lua-hooks.md`: `key_down` payload now documents `super`.
- `main.rs`: the load-bearing plugin order is now stated where it can be violated — a plugin
  that registers a command or menu needs the resource/messages its provider's `build()`
  installed, because `register_system` initialises `SystemParam`s immediately.
- `README.md` links (`docs/lua-hooks.md`) still resolve; no documented surface moved.

### 4.6 Recommendation for the user (not done)

`repomix-output.xml` (167 KB) is a generated source snapshot and is already stale relative to
this refactor. It should be gitignored and untracked; I did not delete a file you committed.

---

## 5. Iteration 4 — final reductions and test coverage

### 5.1 The four modifiers were enumerated three times

`MODIFIER_FLAGS` listed `name → flag` for the Lua map shape, `parse_mod_keys` iterated that
table, and `current_modifier_flags` carried its **own** separate list of
`[left, right] → flag` pairs. Adding a modifier (or fixing a name) meant three synchronised
edits in three places, with nothing linking them.

Collapsed into one table `MODIFIER_KEYS: [(&str, ModKeys, [KeyCode; 2]); 4]` that all three
derive from. `parse_mod_keys` is now documented as the inverse of `modifier_map`, and that
inverse property is tested rather than assumed.

### 5.2 Input-name coverage

`input/keys.rs` gained tests for the string contract the Lua layer depends on: every
`KeyCode` name parses and its `Debug` form round-trips to the same string (this is what
`binding_key_and_mods` emits, so a mismatch would mean a remap silently failing), the mouse
aliases the bundled scripts use (`Left`, `MouseButton4`, …), `modifier_map`/`parse_mod_keys`
being inverses over several flag combinations, the exact documented key set of the modifier
map, and a binding surviving serialise → parse → re-serialise.

### 5.3 Considered and rejected: splitting `apply_dom_ops`

`apply_dom_ops` is the longest function left (~120 lines). It is a flat `match` with one arm
per `DomOp` variant — effectively a dispatch table. Extracting `apply_dom_op(op) -> bool`
would move the same lines behind a call, force `continue` → `return false` rewrites in three
arms, and re-indent the whole block, for no gain in how easy the code is to read or extend
(adding an op means adding one arm either way). Left as-is deliberately.

---

## 6. Final state

| | before | after |
| --- | --- | --- |
| Rust files | 10 | 25 |
| Largest file | 1176 (`menus/mod.rs`) | 655 (`character.rs`, of which ~100 is tests) |
| Files over 400 lines | 2 | 2 (`character.rs`, `camera.rs` — both include their test modules) |
| Unit tests | 0 | 48 |
| `cargo check --all-targets` warnings | 0 | 0 |

Verification actually performed:

- `cargo check --all-targets` — clean, zero warnings.
- `cargo test --bin world_generation` — 48 passed, 0 failed.
- `cargo build` — links successfully (a `cargo check` green build does not guarantee the
  binary links, and iteration 2 added generic observer registrations at plugin-build time).
- Independent adversarial audits of the split against `git HEAD`, and of the iteration 2–3
  behaviour claims (camera bindings, gameplay-context move, command runner, dead-field removal).
- **Not performed:** running the game. It grabs the cursor, so launching it unattended would be
  intrusive. The camera hotkeys' move onto the shared gameplay context is therefore verified by
  reasoning and audit only — worth a quick play-test of look, `Shift+F`, scroll-zoom, cursor
  grab/release and the F1 / `` ` `` menus before trusting it.

Behaviour changes made on purpose, all listed here and nowhere else:

1. `key_down` DOM payload gained a `super` field (docs updated).
2. `CommandArgs` lost `raw` and `command` (unread); multiple commands in one frame now each
   receive their own arguments instead of all reading the last one's.
3. Camera hotkeys attach to the shared gameplay context instead of a second context entity the
   camera also owned.

Everything else is intended to be byte-for-byte equivalent in behaviour.
