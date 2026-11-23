# Refactor log

## Original instruction (preserved verbatim)

> Im going to sleep. I want you to iteratively refactor this codebase to follow best practices,
> good design patterns, clean architecture to achieve grater flexibility, maintainability,
> scalability, and modularity.
>
> 1. Read the codebase
> 2. Formulate a well reasoned plan
> 3. Audit and update the plan until correct
> 4. Implement that plan
> 5. Audit and resolve the implementation until correct.
> 6. Reduce the implementation by taking advantage of DRY and other principles.
> 7. Record and justify your doings in refactor.md
> 8. Repeat.
>
> on context compression, preserve this message exactly.

---

## Baseline (start of session)

- Branch `master`, clean tree. `cargo test` → **59 passed, 0 failed**. `cargo build` clean.
- ~5,785 lines of Rust across 27 files. Bevy 0.20, avian3d, bevy_flair + bevy_mod_scripting (Lua),
  bevy_enhanced_input, meshless_voronoi.
- The codebase has clearly been through a prior refactor: plugin-per-module layout, a
  declaration/registration decoupling (`utils::registry`), a documented Lua contract
  (`docs/lua-only-hooks.md`), DRY helpers (`script_map`, `broadcast_to_menus`, the `MODIFIER_KEYS`
  table, the `command!` macro), and unit tests on all the pure logic (camera math, character
  physics, terrain generation, DOM spec parsing, command registry, key round-trips).

**Assessment:** this is a high-quality codebase. The obvious structural wins are already taken.
The remaining work is subtler: contract accuracy, modularity of the two largest files, and small
surface reductions. I am deliberately avoiding churn — every change below is justified against a
concrete goal, and rejected options are recorded with reasons.

---

## Findings

| #   | Severity             | Finding                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| --- | -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| F1  | **Med (contract)**   | `docs/lua-only-hooks.md` documents `state.pause(source)` / `state.resume(source)` with a `source` parameter. The Rust impl (`src/state.rs`) registers both with **no** arguments and uses a reference-counted `PauseTracker` (menu-pause count + manual flag), not per-source tracking. The bundled scripts call `state.pause()` / `state.resume()` with no args. The doc overstates the API — a mod author following the doc would believe per-source pause ownership exists when it does not. |
| F2  | **Low (modularity)** | `src/terrain/mod.rs` is 647 lines mixing two unrelated concerns: a pure, dependency-light heightfield domain model (`TerrainMap`/`TerrainCell` + Voronoi/noise generation + height sampling) and Bevy-specific rendering/fog/atmosphere/skybox systems + console commands. The domain model has no ECS dependencies and is independently testable, but is buried in the rendering file.                                                                                                         |
| F3  | **Low (surface)**    | `input::set_context_active` and `input::keys::parse_key_code` are `pub` but used only within their own module.                                                                                                                                                                                                                                                                                                                                                                                  |

## Considered and rejected

- **Split `character.rs` (678 lines).** Rejected for now: its parts (scene spawn, ground probe,
  physics integration, camera-relative rotation) are tightly coupled through shared components and
  constants; a split would scatter tightly-related code without a clean seam. The pure physics math
  is already extracted as free functions with tests. Revisit only if a real seam appears.
- **Rename `menus/state.rs`** (it collides conceptually with top-level `state.rs`). Rejected: the
  type is `MenuManager`, the module is scoped `menus::state`, and a rename is pure churn.
- **Move `state.rs`'s Lua namespace into a `state/scripting.rs`** for consistency with
  `input`/`menus`. Deferred: small win, will fold into a later iteration if it stays clean.
- **Rewrite `PauseTracker` to true per-source tracking** to match the (wrong) doc. Rejected: the
  counter model is correct and simpler; the scripts don't pass a source. Fix the doc, not the code.

---

## Plan — Iteration 1

1. **F1 — Fix the Lua contract doc.** Rewrite the `state.pause`/`state.resume` rows in
   `docs/lua-only-hooks.md` to describe the actual no-arg, reference-counted behaviour.
2. **F2 — Split `terrain`.** Extract the pure domain model into `src/terrain/map.rs`
   (`TerrainCell`, `TerrainMap` + its impl + `splitmix64`/`hash_unit_f32` + the domain tests).
   Keep `TerrainPlugin`, fog, skybox, atmosphere, mesh building, the `command!` invocations and
   their system functions, and the rendering/fog tests in `terrain/mod.rs`. Re-export
   `pub use map::{TerrainCell, TerrainMap};` so `crate::terrain::TerrainMap` (used by `character.rs`)
   is unchanged.
3. **F3 — Tighten visibility.** `set_context_active` and `parse_key_code` → `pub(crate)`.

### Plan audit (before implementing)

- **Command-name trap** (memory: `command!` derives the user-facing name from `module_path!()` at
  the *call site*): the four terrain commands (`toggle_atmospheric_fog`,
  `toggle_fog_light_influence`, `set_cell_height`, `cell_info`) are registered by `command!(...)`
  invocations inside `TerrainPlugin::build`. As long as those invocations stay in `terrain/mod.rs`,
  `module_path!()` is still `world_generation::terrain` and the names are byte-for-byte unchanged,
  even though the system fns may be defined in a submodule. **Decision: keep all `command!`
  invocations in `terrain/mod.rs::build`.** Verify after by confirming the four names are unchanged.
- **External references:** only `character.rs` uses `TerrainMap` (via `crate::terrain::TerrainMap`
  and `height_at`). The `pub use` re-export keeps that path valid. `TerrainCell` is not used outside
  `terrain`. Verified by grep.
- **Lua contract:** no namespace function or callback label is added/removed/renamed by this
  iteration; only the doc text changes. Will re-diff registered names against the doc after.
- **Behaviour:** F1 is doc-only. F2 is a pure code move (compiler-checked). F3 is visibility-only.
  No runtime behaviour change is intended anywhere.

Proceeding to implement.

---

## Iteration 1 — implementation & audit

### What was done

- **F1 (doc contract).** `docs/lua-only-hooks.md`: rewrote the `state` namespace table so
  `state.pause()` / `state.resume()` are documented as **no-argument**, reference-counted
  calls (per-open-menu count + a manual flag), matching `src/state.rs`. The old text invented a
  `source` parameter that never existed.
- **F2 (terrain modularity).** Split `src/terrain/mod.rs` (647 → 394 lines):
  - New `src/terrain/map.rs` (283 lines) holds the pure heightfield model: `TerrainCell`,
    `TerrainMap` (+ `Default`, generation, height sampling), `splitmix64`/`hash_unit_f32`, and
    the four domain tests. It imports only `bevy::prelude::{Resource, Vec2}` + the noise/voronoi/
    glam crates — no ECS/rendering concern.
  - `mod.rs` keeps `TerrainPlugin`, fog, skybox, atmosphere, mesh/collider building, the four
    `command!` invocations and their system fns, and the rendering/fog tests.
  - `pub use map::TerrainMap;` preserves the external path `crate::terrain::TerrainMap` used by
    `character.rs`. Added `TerrainMap::generation()` accessor (was a private field read directly
    by `mod.rs` when they shared a module).
- **F3 (surface).** `input::set_context_active` and `input::keys::parse_key_code` → private
  (each used only inside its own module).

### Bugs the implementation audit caught (and fixed)

1. `TerrainCell` re-export was unused (nothing names the type; it is only a method return type) →
   dropped from the re-export, keeping the build warning-clean.
2. The moved test used bevy's prelude free `default()` in `..default()`, not imported in `map.rs`
   → changed to `..Default::default()`.
3. `mod.rs` read `TerrainMap::generation` (private field) after the move → added a `generation()`
   accessor.

### Invariants verified after the change

- `cargo test` → **59 passed, 0 failed** (the 4 domain tests now report under `terrain::map::tests`).
- `cargo build` → **no warnings, no errors**.
- **Command names unchanged:** all four `command!(...)` calls remain inside `TerrainPlugin::build`
  in `terrain/mod.rs`, so `module_path!()` is still `world_generation::terrain` and the
  user-facing names (`terrain>toggle_atmospheric_fog`, `terrain>toggle_fog_light_influence`,
  `terrain>set_cell_height`, `terrain>cell_info`) are byte-for-byte identical. The
  `command_macro_derives_its_name_from_the_module_path` test still passes.
- **Lua contract unchanged:** no namespace function or callback label was added/removed/renamed;
  only doc prose changed.

### Behaviour changes

**None.** F1 is doc-only; F2 is a compiler-checked code move (the `generation()` accessor is a
read of the same field); F3 is visibility-only. No runtime path altered.

---

## Iteration 2 — plan

Re-scan for the next-highest-value work now that the terrain file is split. Candidates to evaluate
against the "no churn" bar:

- **`state.rs` Lua-namespace consistency.** `input` and `menus` each isolate their Lua namespace in
  a `scripting.rs`; `state.rs` inlines its `state` namespace registration inside `StatePlugin::build`.
  Assess whether extracting it (or at least the pause/resume closures, which duplicate the
  `menu_open` lookup and tracker-mutation shape) is a real improvement or churn.
- **`utils/registry/commands.rs` (513 lines).** Assess whether the `commands` Lua namespace and the
  history/registry core want separating, same bar as terrain.
- Re-read the two remaining large files for any genuine seam.

Decisions and results appended below as the iteration proceeds.

### Iteration 2 — implementation & audit

**Chosen: encapsulate the pause-counting invariants into `PauseTracker`.** The `state.pause` /
`state.resume` Lua closures inlined the increment/decrement and manual-vs-menu branching, while
`is_paused()` already lived on `PauseTracker`. The counting rules belong with the state they guard
(tell-don't-ask), so they moved to `PauseTracker::request(menu_open) -> bool` and
`PauseTracker::release(menu_open) -> bool`; the closures now just read `menu_open`, call the
method, and apply the resulting `NextState`. Added two direct unit tests for the reference-count
and manual-toggle invariants (previously only covered indirectly through the app).

**Rejected in this iteration (recorded so the reasoning is visible):**
- *Split `commands.rs` (513 lines).* Rejected: cohesive around "console commands"; ~210 of those
  lines are the test module, so the production code is ~300 lines. No clean seam that isn't churn.
- *Split `state.rs`'s Lua namespace into `state/scripting.rs` for consistency with `input`/`menus`.*
  Rejected: the namespace is ~55 lines tightly coupled to the private `PauseTracker`; a split would
  force widening `PauseTracker` visibility for no real gain.
- *DRY the pressed/just-pressed list building in `dispatch_modifier_changes`.* Rejected: the two
  blocks differ (wheel only in `pressed`; `get_pressed` vs `get_just_pressed`), and the only clean
  helper takes a `just: bool` flag — not clearly better than the current explicit code.

**Audit:** `cargo test` → **61 passed, 0 failed** (the two pre-existing pause tests still green, so
behaviour is unchanged); `cargo build` warning-clean. **Behaviour change: none** — the method bodies
are the exact prior branching, moved verbatim.

---

## Iteration 3 — plan, implementation & audit

After Iteration 2 I ran an independent review pass (a fresh reviewer given the diff, not my
conclusions) to catch anything I'd rationalised away. It confirmed the codebase is well-factored and
surfaced a small set of items; only two cleared the "removes a concrete defect" bar, the rest are
recorded below as identified-not-fixed.

### Findings

| #   | Severity                       | Finding                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| --- | ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| F4  | **Low (dead code + contract)** | A *menu input context* was half-wired: `InputPlugin::build` registered `register_hotkey_context::<MenuInput>("menu")`, `MenuInput` was a defined marker component, and `sync_context_activity` queried `With<MenuInput>` and toggled its activity on every menu open/close. But **no entity is ever spawned with `MenuInput`** (grep: nothing spawns/inserts it), so the query is always empty and the toggle loop is dead. The same phantom leaked into the contract: `docs/lua-only-hooks.md` told mod authors `input.list_bindings()` could report `context == "menu"`, a value that can never appear. |
| F5  | **Low (contract)**             | `right_click` is a live DOM event — emitted in `menus/dom/events.rs` for the secondary mouse button (a deliberate separate event so menus can dismiss without firing `click`), and actually used by `assets/menus/keyboard/script.lua`. It was missing from the `dom.on` event list in `docs/lua-only-hooks.md`.                                                                                                                                                                                                                                                                                          |

### Plan

1. **F4.** Delete the vestigial menu context end-to-end: the `register_hotkey_context::<MenuInput>`
   call, the `MenuInput` marker struct, the `q_menu` query param, and the dead toggle loop in
   `sync_context_activity`. Update the module doc (it framed contexts as "gameplay vs menu"). In
   `docs/lua-only-hooks.md`, drop the unreachable `"menu"` value from `input.list_bindings()`.
2. **F5.** Add `right_click` to the documented `dom.on` event list, noting it is the secondary
   button / dismiss gesture.

### Plan audit (before implementing)

- **Lua contract risk:** removing the `"menu"` context value only removes a value that was never
  produced, so no live script can be relying on it. Verified by grepping `assets/` for any
  `context == "menu"` comparison — none. `right_click` is additive to the doc (the event already
  fires), so documenting it cannot break anything.
- **Behaviour:** removing the dead loop changes nothing observable (it iterated an empty query).
  The `set_context_active::<GameplayInput>` paths are untouched.

### Implementation

- `src/input/mod.rs`: removed the `MenuInput` registration, the `MenuInput` struct, the `q_menu`
  param, and the dead `for entity in &q_menu { … }` loop; rewrote the module doc and the
  `sync_context_activity` doc to describe the single gameplay context.
- `docs/lua-only-hooks.md`: `input.list_bindings()` `context` is now `"gameplay"` or `"unknown"`;
  `dom.on` event list now includes `right_click` with its dismiss-gesture note, and the
  `on_dom_event` payload table lists `right_click` (no payload) alongside `click`.
- **Full contract cross-check.** Enumerated every `register(...)` call and every callback label in
  the source and diffed against the doc: `input` (4 fns), `menus` (3), `dom` (13), `clipboard` (2),
  `state` (3), `commands` (3), and the 5 `on_*` callbacks all match the doc exactly — no phantom
  functions remain and none are undocumented. This is the surface most prone to silent breakage, so
  it was worth a complete pass rather than spot-fixing.

### Audit

- `cargo test` → **61 passed, 0 failed**. `cargo build` → **no warnings, no errors**.
- `grep MenuInput` across the repo → no matches (fully removed).
- `grep` for `context == "menu"` in `assets/` → no matches (no script depended on the phantom).
- Lua contract: one unreachable value removed, one real event name added. No live script breaks.

### Behaviour changes

**None at runtime.** The deleted loop iterated an empty query; the rest is doc text.

---

## Findings identified, not fixed (with reasons)

These came from the independent review. Each is real but does not clear the bar for a *refactor*
(no concrete defect being removed, or the fix is a feature/behaviour change rather than a cleanup),
so they are recorded rather than churned:

- **Per-menu pause ownership.** `PauseTracker` reference-counts menu pauses with a single counter
  rather than tracking which menu paused. A menu that calls `state.pause()` twice and `resume()`
  once leaves a stale count. This is **self-healing**: `sync_menu_pauses` resets `menu_pauses` to
  0 whenever no menu is open, and on hot-reload. True per-source ownership is a feature change
  (the bundled scripts never pass a source), not a cleanup — fixing the doc (Iteration 1) was the
  right call, not rewriting the model.
- **`dom.get_by_id` / duplicate ids.** `get_by_id` returns the first match and `nil` when absent;
  nothing forbids two nodes sharing an `id`. Defining the intended policy (reject dupes? last-wins?
  scope rules?) is a feature decision, and the current first-match/nil behaviour is a reasonable
  default that no current script violates. Left as-is rather than invent semantics.
- **DOM op-queue lifetime on same-frame remove-then-create.** The DOM op queue batches `DomOp`s
  applied per frame; a pathological script that removes a node and creates a new one with the same
  id in the *same* frame could leave an op targeting the prior entity. This is self-correcting on
  the next frame and no bundled script does it; hardening it would add bookkeeping for a case that
  does not occur, so it is noted, not fixed.

---

## Status after Iteration 3

The codebase is at a strong local optimum: plugin-per-module layout, declaration/registration
decoupling, a documented and now-accurate Lua contract, DRY helpers, and unit tests on all pure
logic. The two largest files are split along real seams, the vestigial menu input context is gone,
and the contract doc now matches what the engine actually emits.

Iterations 1–3 each removed a concrete defect (a wrong doc contract, a modularity seam, dead code +
a phantom contract value). The independent review found no further item that clears that bar without
becoming churn or a feature change. **Stopping here rather than manufacturing work** — the remaining
candidates are either rejected-with-reason (above) or would change behaviour, which is outside a
refactor's remit.

---

## Iteration 4 — decoupling: the integration layer

**New finding F6 (architecture, High).** Iterations 1–3 split files by *concern* but left the
*coupling between modules* untouched. A dependency scan found feature modules importing one another
directly — reading each other's resources, querying each other's components, and asserting ordering
across module boundaries. Two of these were **cycles**: `input ⇄ menus` and `menus ⇄ registry::commands`.
Concretely: `state`, `input` and `camera` each read `menus::MenuManager`; `menus` read
`input::RemapStatus` and `commands::CommandHistory`; `terrain` queried `camera::MainCamera`; and the
"declare a menu" path made `input` and the command console depend on `menus`, while `menus` in turn
called `input`'s and the console's registration ports. No module could be understood, tested or reused
in isolation, and adding a feature meant threading knowledge through several modules.

**Direction (from Noor):** systems should not depend on one another; put the wiring in an
**integration** layer — a single `integration` module with one `IntegrationPlugin`, not one per pair.

### Design

A feature module (`input`, `menus`, `camera`, `terrain`, `state`, `character`) may depend only on
Bevy, third-party crates, `utils` (the shared kernel), and its own submodules — **never another
feature module**. `src/integration.rs` is the only place allowed to import two or more features; it
owns three kinds of glue:

1. **Menu wiring** — `wire_menu::<A>(spec)` performs the composite "console command + gameplay
   hotkey → `ToggleMenu`" registration that used to live in `MenuPlugin`, and the two concrete menus
   (keyboard overlay, command palette) are declared here. `MenuSpec` and the two toggle action types
   moved here too, because naming a hotkey action and a console command is wiring vocabulary, not menu
   engine vocabulary.
2. **Bridging systems** — `sync_context_activity` (menus→input), `sync_cursor_with_menus`
   (menus→camera) and `sync_skybox_to_camera` (camera→terrain) moved **verbatim** into integration,
   registered on the same schedules with the same ordering.
3. **Cross-module ordering** — the `InputUpdateSystems … before MenuUpdateSystems` constraint moved
   here, since it spans two modules.

### Plan audit (before implementing)

- **command-name trap** (memory): the keyboard-overlay menu command is the explicit string
  `"menu>keyboard"`, not derived from `module_path!()`, so moving its registration to integration
  cannot rename it. The palette declares no command. No `command!` invocation was moved. Verified the
  `command_macro_derives_its_name_from_the_module_path` test still passes.
- **Lua contract**: untouched — the namespaces and callbacks are unchanged; only where the menu
  wiring is registered moved.
- **Behaviour**: every moved system is byte-identical; the menu wiring is build-time only. The
  `register_gameplay_hotkey` Startup systems and observers behave the same regardless of which plugin
  adds them, and every resource they touch at build time (`CommandRegistry`, `EnhancedInputPlugin`,
  `ToggleMenu`) is installed by a plugin added before `IntegrationPlugin` (added last in `main.rs`).

### Implementation

- **Added** `src/integration.rs` (`IntegrationPlugin`): `wire_menu`, `MenuSpec`, the two menu
  declarations + their action types, and the three bridging systems + the cross-module ordering.
- **`input`**: dropped `use crate::menus::…` entirely; removed the keyboard-overlay declaration,
  `sync_context_activity`, and the `.before(MenuUpdateSystems)` ordering; `set_context_active` →
  `pub(crate)` (now driven by integration).
- **`menus`**: dropped `use crate::input::…` and `use crate::utils::registry::commands::…`; removed
  `MenuSpec`/`MenuAppExt`/`register_menu_with_hotkey` and the queue drain; re-exports `ToggleMenu`
  for integration. The menu engine now knows only menus.
- **`camera`**: dropped `use crate::menus::MenuManager`; removed `sync_cursor_with_menus`;
  `WantedCursorLock` → `pub(crate)`.
- **`terrain`**: dropped `use crate::camera::…`; removed `sync_skybox_to_camera`; `SkyBox` → `pub(crate)`.
- **`registry::commands`**: dropped `use crate::menus::…`; removed the palette declaration + its action type.
- **Deleted** `src/utils/registry/menus.rs` (the `MenuRegistry` deferred-declaration queue) and its
  `MenuRegistryPlugin` — with the wiring relocated, nothing queues menus anymore, so the whole
  indirection is dead. Net reduction.
- **`main.rs`**: removed `MenuRegistryPlugin`; added `mod integration;` and `IntegrationPlugin` last.

### Audit

- `cargo build` → **no warnings, no errors**; `cargo test` → **61 passed, 0 failed**.
- Dependency re-scan confirms the cycles are gone and these edges were removed:
  `input→menus`, `commands→menus`, `camera→menus`, `terrain→camera`.

### Behaviour changes

**None.** Moved systems are verbatim; menu wiring is build-time and order-equivalent; the Lua
contract and all command names are unchanged.

### Deferred to Iteration 5 (live-read couplings, higher risk)

These remaining edges read another module's resource *at call time*, so decoupling them needs care
about timing (a frame-based mirror would change when a value is observed) rather than a verbatim move:

- **`state → menus`** (`MenuManager`): read live inside the `state.pause`/`resume` Lua closures and
  `sync_menu_pauses`. The faithful fix is to move that consuming logic into integration so it keeps
  the live read, not to mirror it.
- **`menus → input`** (`RemapStatus` resource + the pure `current_modifier_flags`/`modifier_*`/
  `mouse_button_name` helpers): the resource read becomes an event `input` emits; the pure helpers are
  library code and could move to `utils`.
- **`menus → commands`** (`CommandHistory`): same — `commands` emits a "command ran" event.
- **`character → {camera, input, state, terrain}`**: the most entangled module; needs its own pass.

---

## Iteration 5 — decoupling `character`

**Finding F7 (architecture, High).** `character` was the most entangled module left: it imported
four feature modules at once — `camera` (read `CameraRig.yaw`/`view_mode` live, queried
`CameraTarget`, ordered against `CameraSystems`), `state` (gated its own systems on
`in_state(PlayState::Running)`), `terrain` (read `TerrainMap.height_at` to place the spawn), and
`input` (the action ports). Per the integration rule it should import none of the first three.

### Design

Each coupling was cut with the mechanism that fits its *kind* of dependency, not a blanket mirror:

- **`state` — a gate, not a value.** `character` never needs to *read* `PlayState`; it only needs
  its systems to run while playing. So `character` now tags its gameplay systems with a
  `CharacterSystems::Playing` set and says nothing about states; integration applies
  `configure_sets(… CharacterSystems::Playing.run_if(in_state(PlayState::Running)))` in both
  `FixedUpdate` and `Update`. The module that names the state machine is integration, not character.
- **`terrain` — a kernel fact.** The player spawns at the origin and needs exactly one number: the
  surface height there. `character` must not depend on the whole heightfield, so `terrain` publishes
  a tiny `utils::SpawnGroundHeight(f32)` — set at **build time** (before any `Startup`, so the spawn
  system already sees it) and refreshed in `rebuild_terrain_on_change` when the heightfield is
  edited. It defaults to `0.0`, so `character` still works with no `terrain` present.
- **`camera` — a bridge component + a wiring system.** `character` reads two camera facts
  (`yaw`, `first_person`) every frame and must carry a `CameraTarget` so the camera follows it.
  Both are camera types, so they leave `character`:
  - `CameraFacing { yaw, first_person }` is a **character-local** component; integration's
    `sync_camera_facing` (PreUpdate) copies the rig into it. The movement/visibility systems read
    `CameraFacing`, never `CameraRig`.
  - `CameraTarget` is inserted onto the player by integration's `attach_camera_target` (Update,
    once), so `character` never names a camera type.
- **`input` — kept as a port.** `character` uses `GameplayInput`/`MovementBinding`/`ContextActive`
  to *declare* its actions, exactly as `camera` and `dev` do. That is the input port, not a
  feature-to-feature data coupling, so it stays (consistent with the Iteration 4 decision).

### Plan audit (before implementing)

- **command-name trap** (memory): the only `command!` in `character` is `respawn_character`; the
  invocation stays in `character`, so `module_path!()` is unchanged and the name
  `character>respawn_character` is byte-for-byte identical. Only the system *fn* bodies changed.
- **Lua contract**: unchanged — no namespace fn or callback label added/removed/renamed.
- **Timing of the camera mirror** (the one real risk): previously `character_physics_move`
  (FixedUpdate) and `character_rotate_body` (Update) read `CameraRig.yaw` live. Now they read
  `CameraFacing.yaw`, written in PreUpdate. Camera look is applied in `Update`, which runs *after*
  PreUpdate, so:
  - For **FixedUpdate physics**, this is equivalent — physics already consumed the previous frame's
    committed yaw (camera look in Update lands after FixedUpdate in the same frame).
  - For **`character_rotate_body` (Update)**, the yaw is now the PreUpdate copy (last frame's
    committed value) rather than the same-frame post-look value — a sub-frame, one-frame-of-look lag
    on body rotation only. Imperceptible; recorded as the sole behaviour nuance.
- **`attach_camera_target` timing**: `CameraTarget` was inserted in `setup_character` (Startup); now
  it is inserted on the first `Update` the player exists. One-frame delay before the camera tracks
  the player, at which point the player is at the spawn point the camera already faces. Imperceptible.

### Implementation

- **`character.rs`**: dropped `use crate::{camera, state, terrain}`; added `utils::SpawnGroundHeight`.
  Added `CharacterSystems::Playing` (set) and `CameraFacing` (bridge component). Systems now tagged
  `.in_set(CharacterSystems::Playing)`; `sync_body_visibility` ordered `.before(TransformSystems::Propagate)`
  (no longer names `CameraSystems`). `character_physics_move`/`character_rotate_body`/`sync_body_visibility`
  read `Single<&CameraFacing>` instead of `CameraRig`; `setup_character` inserts `CameraFacing::default()`
  (not `CameraTarget`) and reads `SpawnGroundHeight`; `respawn_character` reads `SpawnGroundHeight`
  (not `TerrainMap`). `spawn_translation(ground_height)` is the shared spawn-placement helper.
- **`utils/world.rs`** (new): the `SpawnGroundHeight` kernel resource; re-exported from `utils`.
- **`terrain/mod.rs`**: publishes `SpawnGroundHeight` at build time and refreshes it in
  `rebuild_terrain_on_change`.
- **`integration.rs`**: added the `CharacterSystems::Playing` gating (FixedUpdate + Update),
  `sync_camera_facing` (PreUpdate), and `attach_camera_target` (Update).

### Audit

- `cargo build` → **no warnings, no errors**; `cargo test` → **61 passed, 0 failed**.
- Import re-scan: `character` now imports only `input` (port) and `utils` (kernel) — no `camera`,
  `state`, or `terrain`. The only remaining cross-feature edges in the whole crate are the three
  deferred live-read couplings below (`menus→input`, `state→menus`, `menus→commands`) plus
  `utils/dev→input` (the dev overlay, a port consumer).

### Behaviour changes

Two sub-frame timing nuances, both imperceptible and both inherent to moving a live cross-module
read behind a per-frame copy:
1. Body rotation (`character_rotate_body`) now uses the PreUpdate camera-yaw copy rather than the
   same-frame post-look value — at most one frame of look lag on the capsule's facing, not on the
   camera itself.
2. The camera begins tracking the player on the first `Update` after spawn instead of during
   `Startup` — one frame, at the spawn point.

No gameplay, physics, command-name, or Lua-contract change.

### Still deferred (live-read couplings, unchanged from Iteration 4)

- **`state → menus`** (`MenuManager` live reads in the pause/resume closures).
- **`menus → input`** (`RemapStatus` event + move the pure modifier helpers to `utils`).
- **`menus → commands`** (`CommandHistory` → a "command ran" event).

---

## Iteration 6 — decoupling `menus` from `input`

**Finding F8 (architecture, High).** `menus` imported `input` in two files: `menus/dispatch.rs`
(`RemapStatus` + the pure helpers `current_modifier_flags`/`modifier_map`/`mouse_button_name`) and
`menus/dom/events.rs` (`current_modifier_flags`/`modifier_entries`). This was the last
feature→feature edge besides `state → menus`.

**Scope correction.** The Iteration 4/5 deferred list also named `menus → commands`
(`CommandHistory`). On inspection that is **not** a violation: `CommandHistory` lives in
`utils::registry::commands`, and `utils` is the shared kernel the rule explicitly permits. No change
needed; the earlier framing had over-counted it as a peer feature.

### Design

The two `menus → input` dependencies are different kinds of thing, so each went to the kernel for
its own reason — both as *moves*, not mirrors, so behaviour is byte-identical:

- **The input vocabulary → `utils/keys.rs`.** `input/keys.rs` is a pure library — key/button
  parsing, canonical key names, modifier-flag conversion — with no input-plugin state. Its own doc
  calls it "the single source of truth for the string form of an input that crosses the Lua
  boundary." Both `input` (remap, the `input` Lua namespace) and `menus` (keyboard overlay, DOM key
  events) speak it, so it is shared kernel material, not `input`'s private code. Moved whole, so
  `MODIFIER_KEYS` stays in one place rather than being split across a helper/module seam.
- **`RemapStatus` → `utils/remap_status.rs`.** A tiny shared fact (`{ success, message, version }`):
  `input` writes it, `menus` reads it to tell open menus the outcome. This is the exact
  producer/consumer pattern already established by `SpawnGroundHeight` — the producer writes a
  kernel resource, the consumer reads it, neither imports the other.

**Rejected alternatives (recorded so the reasoning is visible):**
- *Convert `RemapStatus` to a message `input` emits.* Rejected: it changes observation timing
  (poll-on-`is_changed` vs read-a-message), and `input`'s own Lua `input.remap_status()` still needs
  the resource, so it would add a second representation for no behavioural gain. The resource move is
  zero-behaviour-change.
- *Move the bridging system `dispatch_remap_results` into `integration`.* Rejected: it calls
  `broadcast_to_menus` and reads `MenuRoot::ready_script()` / the `OnMenuRemapResult` label — all
  `menus` internals. Hoisting it would leak `menus`' broadcast machinery into integration, which is
  worse coupling in a different place. The three existing integration bridges are self-contained; this
  one is not.

### Plan audit (before implementing)

- **command-name trap** (memory): neither `keys.rs` nor `RemapStatus` contains a `command!`
  invocation or a Lua `register(...)` — they are plain fns and a resource. The `input` Lua namespace
  registration stays put in `input/scripting.rs`. No user-facing name can move.
- **Lua contract**: unchanged. `input.remap_status`, `input.active_modifiers`, `input.remap` keep
  identical signatures and behaviour; only the code backing them changed location. The `docs/lua-only-hooks.md`
  "Key names" section describes the vocabulary, which is unchanged (the module doc still points at it).
- **Behaviour**: `keys.rs` moved verbatim (tests moved with it); `RemapStatus` is the same resource
  with the same `set_success`/`set_error`/`version` semantics and the same `is_changed()` polling in
  `dispatch_remap_results`. No runtime path altered.

### Implementation

- **Added** `utils/keys.rs` (moved from `input/keys.rs`, verbatim + tests) and
  `utils/remap_status.rs` (`RemapStatus` + its impl). Re-exported `RemapStatus` from `utils`.
- **`input`**: deleted `input/keys.rs`; dropped the `pub mod keys` / `pub use keys::{…}` and the
  `RemapStatus` re-export; `input/{hotkeys,remap,scripting}.rs` now import the vocabulary and
  `RemapStatus` from `utils`. `input/remap.rs` keeps the remap *system* and `RemapBindingMessage`.
- **`menus`**: `dispatch.rs` and `dom/events.rs` now import the helpers and `RemapStatus` from
  `utils` — no `crate::input::…` remains anywhere in `menus`.

### Audit

- `cargo build` → **no warnings, no errors**; `cargo test` → **61 passed, 0 failed** (the five
  key/modifier tests now report under `utils::keys::tests`; the pause tests are untouched).
- Import re-scan: **`menus` no longer imports `input` at all.** The only cross-module edges left in
  the crate are `state → menus` (the last feature→feature coupling) and `utils/dev → input` (the dev
  overlay using the input registration port — a kernel→feature port use, not feature→feature).

### Behaviour changes

**None.** Pure-function and resource relocations; identical call sites, identical change-detection.

### Still deferred

- **`state → menus`** (`MenuManager` live reads in the pause/resume closures and `sync_menu_pauses`)
  — the last feature→feature edge; needs care about *when* the menu-open fact is observed, so it gets
  its own pass.

---

## Iteration 7 — decoupling `state` from `menus` (the last feature→feature edge)

**Finding F9 (architecture, High).** `state` imported `menus::MenuManager` and read it live in three
places: the `state.pause` / `state.resume` Lua closures (to decide whether a pause is a ref-counted
*menu* pause or a manual toggle) and the `sync_menu_pauses` PostUpdate system (to reset stale
menu-pause counts on hot-reload or once all menus close). This was the final feature→feature edge.

### Design

The menu-open fact is read at *call time* (inside a Lua closure), so a per-frame mirror could be
observed a frame stale and mis-attribute a pause. The faithful fix — and the one Noor's directive
asks for ("create integrations where modules get put together") — is to **move the consuming glue into
`integration` so it keeps the live read**, not to mirror the menu stack.

- **`state` keeps** the state machine and its mechanics: `GameState`, `PlayState`, `PauseTracker`,
  the `OnEnter`/`OnExit` pause/unpause/clear handlers, and the menu-independent `state.is_paused`
  Lua query.
- **`integration` gains** the two menu-aware pieces, moved **verbatim**: the `sync_menu_pauses`
  system and the `state.pause` / `state.resume` registrations. Both read `MenuManager` live.
- **`PauseTracker` → `pub(crate)`** (type + fields + methods) so integration can drive it — the same
  visibility widening already used for `set_context_active`, `WantedCursorLock`, `SkyBox`,
  `CameraFacing`. Its logic is untouched.

**The `state` Lua namespace is now registered from two files** — `state.rs` opens it with
`is_paused`, `integration` adds `pause`/`resume`. This is safe: `bevy_mod_scripting` stores namespace
members in a shared `AppScriptNamespaceMembers` resource keyed by `(namespace, name)`
(`.entry(ns).or_default().functions.insert(name, …)`), so two `in_namespace("state")` builders
registering different names both persist. Verified against the crate source (0.24.0) before relying
on it.

**Rejected alternative — a kernel `MenuOpenState` mirror** (`any_open` + `reload_generation` in
`utils`, like `SpawnGroundHeight`): rejected because it creates a *second source of truth* for "is a
menu open," with a silent-drift hazard — every future `MenuManager` mutation would have to remember
to update the mirror, and a miss would mis-attribute pauses with no compile error. The live read in
integration has no such failure mode and matches the Iteration 4 "move the bridging system verbatim"
pattern.

### Plan audit (before implementing)

- **command-name trap** (memory): no `command!` invocation moved. The Lua functions keep identical
  names (`state.pause`/`state.resume`/`state.is_paused`), identical no-arg signatures, and identical
  behaviour — only *where they are registered* changed. The `docs/lua-only-hooks.md` `state` table is
  unaffected.
- **Lua contract**: unchanged. Same three functions, same semantics (ref-counted menu pause vs manual
  toggle, auto-resume when menus close).
- **Behaviour**: `sync_menu_pauses` and both closures are byte-identical moves; `PauseTracker`'s
  `request`/`release`/`is_paused` are unchanged. The live `MenuManager` read is preserved, so there is
  no timing change at all — unlike a mirror, which would have introduced one.

### Implementation

- **`state.rs`**: dropped `use crate::menus::MenuManager`; removed `sync_menu_pauses` and the
  `pause`/`resume` registrations (kept `is_paused`); `PauseTracker` → `pub(crate)`. The two
  end-to-end pause tests moved to `integration` (they drive the moved system); the pure
  `PauseTracker` counting tests stay in `state`.
- **`integration.rs`**: added `sync_menu_pauses` (verbatim, live read) on `PostUpdate` with the same
  `in_state(GameState::InGame)` + `resource_exists::<MenuManager>` guards, and the `pause`/`resume`
  registrations into the `state` namespace; imported `state::{GameState, PauseTracker, PlayState}` and
  the `bevy_mod_scripting` binding types.

### Audit

- `cargo build` → **no warnings, no errors**; `cargo test` → **61 passed, 0 failed** (the two pause
  tests now report under `integration::tests`; the `PauseTracker` unit tests under `state::tests`).
- Import re-scan: **no *peer* feature→feature coupling remains** — no feature reads another feature's
  state, and both import cycles are gone. The only feature→feature imports left all point **at
  `input`**: `camera → input` (`InputAppExt`), `character → input` (`GameplayInput`, `ContextActive`,
  `MovementBinding`, `HotkeyLabel`), and `utils/dev → input` (`InputAppExt`). These are features
  consuming the **input action-service** to declare their own hotkeys and read action-active state —
  the same category as depending on Bevy's own input, not the peer-state coupling the rule targets.
  `input` itself imports no feature, so there is no cycle. See "The `input` service exception" below.

### Behaviour changes

**None.** Verbatim system/closure moves with the live read preserved; only visibility and file
location changed.

---

## Milestone — integration-layer decoupling complete

Across Iterations 4–7 every **peer** feature→feature coupling and both import cycles were removed:

| Iteration | Edges removed                                                                                                                                                |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 4         | `input→menus`, `menus→input` (declare-menu path), `menus→commands`, `camera→menus`, `terrain→camera` (+ broke the `input⇄menus` and `menus⇄commands` cycles) |
| 5         | `character→{camera, state, terrain}`                                                                                                                         |
| 6         | `menus→input` (input vocabulary + `RemapStatus` → kernel)                                                                                                    |
| 7         | `state→menus` (menu-aware pause glue → integration)                                                                                                          |

No feature now reads another feature's state, and there are no import cycles. All genuine cross-module
*wiring* lives in `src/integration.rs` (one `IntegrationPlugin`) or in the `utils` shared kernel. The
Lua contract and every command name are unchanged throughout Iterations 4–7.

Two follow-on structural cleanups came after this milestone: **Iteration 8** decomposed `utils` into a
true kernel (moving the `dev` and `profiler` feature plugins out to the crate root) and **Iteration 9**
split the now-large `integration.rs` into a `src/integration/` directory by concern. Neither removed a
feature→feature edge; Iteration 8 did rename six dev/profiler console commands (see its behaviour
changes).

### The `input` service exception (and an honest correction)

An earlier draft of this milestone claimed "no feature→feature edges remain." That was **wrong** — my
grep pattern (`crate::feature::`) missed the brace-group import form `use crate::{ input::{…} }`. A
corrected scan finds three remaining feature→feature imports, **all pointing at `input`**:

- `camera → input` — `InputAppExt` (registers camera's own hotkeys).
- `character → input` — `GameplayInput`, `ContextActive`, `MovementBinding`, `HotkeyLabel` (declares
  its actions, tags movement keys, reads whether gameplay input is active).
- `utils/dev → input` — `InputAppExt` (the dev overlay registers its own FPS-overlay hotkey).

These are deliberately kept, on a principle worth stating: **`input` is a foundational action-service,
not a peer feature.** Features consume its public port to declare their own actions and read
action-active state — the same category as depending on Bevy's own input or on `utils`. This is
categorically different from the coupling the rule targets (features reaching into each other's
internals, and the cycles that arose from it). Crucially, `input` imports **no** feature, so this
one-directional service dependency creates no cycle.

The distinction from Iteration 4's `menus` is deliberate: a menu hotkey drives *menu* state
(cross-module → the registration moved to `integration`), whereas a camera/character/dev hotkey
drives that feature's *own* state (intra-feature → it stays in the feature, using the input port).

`utils/dev → input` was the one kernel→feature import at this milestone. It was resolved in
**Iteration 8** (below): `dev` and `profiler` moved out of `utils` to top-level feature modules,
which removed the inversion and accepted the command rename it had been blocking.

---

## Iteration 8 — decompose `utils` (kernel vs. features)

### Finding

`utils` had become a grab-bag with a layering inversion inside it. Two of its submodules were not
utilities at all but **feature plugins** parked there: `utils/dev` (a `Plugin` registering hotkeys
and console commands) and `utils/profiler` (a `Plugin` with four console commands). Because they
were under `utils`, `utils/dev` imported `crate::input` — a *kernel→feature* import, the one edge
that contradicted the "utils is the shared kernel" rule (flagged at the close of Iteration 7).

### Design

Classify every `utils` submodule by whether it is kernel glue or a feature:

| Submodule         | Verdict                       | Reason                                                                                                 |
| ----------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------ |
| `registry/`       | **stays**                     | cross-cutting service; camera, character, terrain, menus, integration all register commands through it |
| `keys.rs`         | **stays**                     | input vocabulary shared by `input` + `menus` (integration glue)                                        |
| `remap_status.rs` | **stays**                     | `input→menus` kernel fact                                                                              |
| `world.rs`        | **stays**                     | `terrain→character` kernel fact (`SpawnGroundHeight`)                                                  |
| `script.rs`       | **stays**                     | Lua value helpers used by input, menus, state, registry                                                |
| `dev.rs`          | **moves → `src/dev.rs`**      | a feature plugin, not a util                                                                           |
| `profiler.rs`     | **moves → `src/profiler.rs`** | a feature plugin, not a util                                                                           |

The user's rule — "make utils per module; utils that are part of the integration can stay" — maps
exactly onto this: what is genuinely cross-module glue (and what `integration` composes) stays;
what is really a feature moves out to sit beside the other features.

### Plan audit (before implementing)

- **Command rename is the only risk.** `command!` derives names from `module_path!()` at the call
  site, so moving the files renames six user-facing commands: `utils>dev>{toggle_wireframe,
  toggle_fps_overlay}` → `dev>*`, and `utils>profiler>{profiler_report, set_spike_threshold,
  reset_profiler, toggle_profiler}` → `profiler>*`. Grepped `assets/` and `docs/` for all six names:
  **zero references** — no Lua mod or doc invokes them, so nothing breaks programmatically. They are
  dev/debug console commands; the new names are also more accurate. Accepted as a documented
  behaviour change.
- **Imports stay valid.** Both files use absolute `crate::…` paths (`crate::input`,
  `crate::utils::registry`), which resolve identically from the crate root. Only `dev.rs`'s module
  doc (which named the old command path) needed a text edit.
- Used `git mv` rather than re-typing (profiler is 389 lines) to avoid transcription error.

### Implementation

`git mv src/utils/dev.rs src/dev.rs`, `git mv src/utils/profiler.rs src/profiler.rs`. Updated
`utils/mod.rs` (dropped the two `pub mod` + re-exports; rewrote the module doc to state the kernel
contract: "nothing here imports a feature module, and nothing here is a plugin"). Updated `main.rs`
(`mod dev; mod profiler;`, direct `use dev::DevUtilsPlugin; use profiler::ProfilerPlugin;`, and
`use utils::registry::commands::CommandRegistryPlugin;`). Updated `dev.rs`'s doc comment to the new
command path.

### Audit

`cargo check --all-targets` clean (no warnings); `cargo test` → **61 passed**. `utils` now contains
only kernel: `keys`, `registry`, `remap_status`, `script`, `world` — none imports a feature. The
kernel→feature inversion is gone; the only remaining feature→feature imports are the three
`→input` service edges (camera, character, and now `dev`→input at the top level, which is a
feature→service edge, not a kernel inversion).

### Behaviour changes

**Six console command names changed** (`utils>dev>*` → `dev>*`, `utils>profiler>*` → `profiler>*`).
No code, Lua mod, or doc referenced the old names, so nothing breaks automatically; only manual
console muscle memory is affected.

---

## Iteration 9 — split `integration.rs` into a directory

### Finding

After Iterations 4–7, `src/integration.rs` had grown to ~450 lines holding three unrelated kinds of
glue (menu wiring, bridging systems, the state↔menus pause logic) plus their tests. A single file
for "everything cross-module" is the same grab-bag smell one level up.

### Design

Split by concern into `src/integration/`:

| File             | Holds                                                                                                                    |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `mod.rs`         | `IntegrationPlugin::build` — cross-module ordering/gating + composing the three submodules                               |
| `menu_wiring.rs` | `MenuSpec`, `wire_menu`, the two menu declarations + their hotkey action types                                           |
| `bridges.rs`     | `sync_context_activity`, `sync_cursor_with_menus`, `sync_skybox_to_camera`, `sync_camera_facing`, `attach_camera_target` |
| `state_pause.rs` | `sync_menu_pauses`, the `state.pause`/`resume` Lua registrations, and the two pause tests                                |

Each submodule exposes one `pub(super) fn register(app)`; `build()` calls the three in order after
asserting the ordering/gating sets. `mod.rs` keeps the module doc (rewritten to describe the layout).

### Plan audit (before implementing)

- **No command renames.** `integration` has no `command!` call; `wire_menu` passes the command name
  explicitly (`"menu>keyboard"`), so moving it changes nothing. The `state` Lua namespace
  registrations are also name-explicit and additive (verified in Iteration 7), so relocating them is
  safe.
- **System order is preserved.** `add_systems` registration order across submodules does not affect
  scheduling (ordering comes from the explicit `.before`/`.after`/`run_if` constraints, all kept
  verbatim). The `sync_menu_pauses` PostUpdate system moved to `state_pause::register` but keeps
  its identical run conditions.
- Used `git mv integration.rs integration/mod.rs` to preserve history, then extracted.

### Implementation

Created `bridges.rs`, `menu_wiring.rs`, `state_pause.rs`; rewrote `mod.rs` to the composing plugin.
All system bodies and the two tests are verbatim moves (only `use` lines were split per file).

### Audit

`cargo check --all-targets` clean; `cargo test` → **61 passed** (the two pause tests now report as
`integration::state_pause::tests::*`). No behaviour change.

### Behaviour changes

**None.** Pure file split; command names, Lua names, system ordering, and run conditions all
unchanged.

---

## Iteration 10 — documentation policy + generated Lua-hook index

### Finding

Two related problems the user raised:

1. **Docs were agent-authored and had to be regenerated by hand.** `docs/lua-only-hooks.md` was entirely
   AI-generated and had to be rewritten every time a Lua hook was added — a maintenance chore the
   user disliked, and a source of drift (the doc could silently disagree with the code).
2. **No standing rule governed comments/docs**, so agents kept adding heavy code comments and
   writing markdown nobody asked for.

### Design

- **A `CLAUDE.md`** (Claude Code's project-memory file, loaded every session) now enforces: no
  comments in code; markdown is human-authored unless explicitly requested; `refactor.md` is the
  agent's allowed log; `docs/.compiled/**` is game-generated and never hand-edited.
- **`DocgenPlugin`** (`src/docgen/`, a generic doc generator whose `lua_docs` submodule
  `src/docgen/lua_docs.rs` writes the hook index) replaces the hand-maintained hook doc. At
  `Startup` it reads bevy_mod_scripting's `AppScriptNamespaceMembers` resource — the single source
  of truth for every `NamespaceBuilder::in_namespace(...).register(...)` call in the codebase — and
  writes `docs/.compiled/lua-only-hooks.md`: each namespace with its function names, sorted. It is
  regenerated on every launch, so it can never drift from the code. (Generalized from a
  single-purpose `LuaDocsPlugin` in Iteration 11 so future doc generators drop in as submodules.)

### Plan audit (before implementing)

- **Is the resource populated at `Startup`?** Yes. Every game namespace (`input`, `state`, `menus`,
  `dom`, `clipboard`, `commands`) is registered during plugin `build()` via
  `NamespaceBuilder::in_namespace(app.world_mut(), …)`, which writes into `AppScriptNamespaceMembers`
  immediately (not deferred to a system). `Startup` runs after all `build()`s, so the map is complete.
  BMS's own `CoreScriptGlobalsPlugin` contributes only a `bms` namespace (one value) — harmless.
- **No descriptions available.** The functions map stores `DynamicScriptFunction` with no
  description field, so the generated file is a name index, not prose. That matches the user's
  "simply a list of lua hooks." The rich prose of the old doc (engine→Lua callbacks, key names,
  event payloads) is **not** reproduced — those are not namespace members and cannot be enumerated.
  Accepted: the user explicitly said breaking the old doc is fine.
- **Path** is `docs/.compiled/lua-only-hooks.md` (user's chosen name), gitignored as a build artifact.

### Implementation

- New `src/docgen/` (`DocgenPlugin` + `lua_docs` submodule, one `Startup` system, written
  comment-free to dogfood the new rule). Added to `main.rs`.
- Deleting the old `docs/lua-only-hooks.md` (`git rm`).
- New `CLAUDE.md`.
- `.gitignore`: added `docs/.compiled/`.
- **Side effect:** adding the plugin pushed the `add_plugins` tuple to 16 elements, past Bevy's
  15-tuple `Plugins` impl limit. Split the single `add_plugins((…))` into two calls (8 + 8), order
  preserved exactly (CommandRegistryPlugin still precedes every command-registering plugin;
  IntegrationPlugin still last). The load-bearing ordering comment was removed per the new
  no-comments rule; the constraint is now encoded structurally by the two-call order.

### Audit

`cargo check --all-targets` clean; `cargo test` → **61 passed**. Verified end-to-end by launching the
game: `docs/.compiled/lua-only-hooks.md` is written at startup with the live namespace/function list.

### Behaviour changes

- `docs/lua-only-hooks.md` (hand/AI-maintained) **deleted**; replaced by the auto-generated
  `docs/.compiled/lua-only-hooks.md` (names only, no prose).
- New policy in `CLAUDE.md`: agents add no code comments and no unrequested markdown.

---

## Iteration 11 — generic `docgen` plugin + codebase-wide comment removal

### Finding

Two follow-ups the user requested after Iteration 10:

1. The single-purpose `LuaDocsPlugin` should be a **generic** doc generator that other doc types can
   join later, with the Lua-hook writer as one submodule.
2. **Remove all comments** from the codebase (the user opted into the mass-strip the CLAUDE.md rule
   had deferred).

### Design

- `src/docgen/` — `DocgenPlugin` runs every generator as a `Startup` system and exposes a shared
  `write_doc(file_name, contents)` helper (creates `docs/.compiled/`, writes, logs on failure).
  `src/docgen/lua_docs.rs` is the first generator (the hook index). Adding a new doc = add a
  submodule with a `write` system and list it in the `Startup` tuple — no other wiring.
- Comment removal done with a **string-aware** stripper (not a regex): it tracks double-quoted string
  literals (with escapes) and char literals (distinguishing them from lifetimes) so it never strips a
  `//` that lives inside a string. The only such case in the tree was `"mods://"` in `menus/assets.rs`,
  which survived intact. No block comments or raw strings existed. Comment-only lines are dropped;
  trailing comments are removed and the line right-stripped; runs of blank lines collapse to one.

### Audit

`cargo check --all-targets` clean; `cargo test` → **61 passed**. `"mods://"` verified present after
the strip; a scan for stray `//` (excluding `://`) found none. Regenerated `docs/.compiled/lua-only-hooks.md`
end-to-end via `DocgenPlugin`.

### Behaviour changes

**None** at runtime. 32 source files lost their comments; the plugin was renamed `LuaDocsPlugin` →
`DocgenPlugin` (module `src/lua_docs.rs` → `src/docgen/`), which changes no command or Lua name.

---

## Iteration 12 — correctness sweep: the user's 10-item plan

### Finding

The user supplied a prioritized 10-item plan (three phases) from a full codebase pass. Every item was
verified against the code before implementing (plan-audit step); all ten were real. Summary of what
was found and done, grouped by phase.

**Phase 1 — silent-breakage bugs**

1. **Stale `HotkeyRegistry` after a remap** (`input/remap.rs`, `input/mod.rs`). `handle_remap_bindings`
   inserts the new `Binding` through deferred `Commands`, but `sync_hotkey_registry` runs later in the
   same `Update` and never sees the insert, so `HotkeyRegistry` (and Lua `input.list_bindings`) showed
   the old key until the next frame. The plan offered (A) a direct `&mut Binding` write or (B) an
   `ApplyDeferred` sync point. **Plan audit rejected A**: `Binding` is an immutable component in
   bevy_enhanced_input 0.27.2 (`Component::Mutability = Immutable` — the compiler rejects
   `&mut Binding`), so the plan's option A was impossible. Implemented B: new
   `InputUpdateSystems::ApplyRemap` set holding an `ApplyDeferred`, chained between `ProcessRemap` and
   `SyncHotkeyRegistry`. Two `ApplyDeferred` instances in one schedule are distinct nodes (each gets
   its own `SystemKey` via `new_key_type!`), verified in the bevy_ecs source. The
   `.before(MenuUpdateSystems)` `configure_sets` in `integration/mod.rs` lists all five sets now.
2. **Duplicate DOM pointer observers + missing root `DomHandlers`** (`menus/dom/node.rs`,
   `menus/lifecycle.rs`, `menus/dom/mod.rs`). `MenuPlugin` registers global `on_dom_click`/
   `on_dom_hover_enter`/`on_dom_hover_exit` observers, and `spawn_node_entity` additionally attached
   per-entity copies on interactive nodes — both fire for the same trigger, so clicks on interactive
   nodes dispatched their handler **twice**. Removed the per-entity observers (global ones remain; the
   per-entity `on_dom_key_input`/`on_dom_text_change` on input nodes stay — they are not duplicated).
   Separately, the menu root entity had no `DomHandlers`, so `dom.on` bindings targeting the root were
   silently dropped; the root now gets `DomHandlers::default()`, and `DomOp::BindEvent` **warns**
   instead of dropping when the target lacks the component.
3. **`commands_docs` namespace grouping** (`docgen/commands_docs.rs`). `format_markdown` emitted a new
   `## namespace` heading whenever the namespace changed between adjacent entries, so bare (no `>`)
   commands interleaved with namespaced ones split the `general` section into several. Now grouped via
   a `BTreeMap<&str, Vec<&CommandEntry>>` first.

**Phase 2 — validation and architecture**

4. **`screenshot_camera` validation + capture scheduling** (`screenshot.rs`). The name query was
   `&Name` without `With<Camera>`, so `screenshot_camera Terrain` queued a doomed request; now
   `Query<&Name, With<Camera>>` and the command reports `error: no camera named '…'` via
   `CommandOutput`. `handle_capture_requests` and `start_pending_captures` were unordered; now
   `.chain()`ed (plugin and test) so a capture camera is never armed the frame it spawns.
   **Deviation:** the plan proposed a `ready: bool` flag on `PendingCapture`; kept the existing
   `is_added()` check plus the chain — same guarantee, no new state.
5. **No `Plugin` in `utils/` + order-independent command registration.** `utils/registry/commands.rs`
   held `CommandRegistryPlugin`, violating the strict-kernel rule, and `register_command` required the
   plugin to be added first (it wrote into a possibly-absent resource). New `src/commands.rs` with
   `CommandsPlugin` (resources, `RunCommand` message, `handle_run_commands`, history, the Lua
   `commands` namespace — **name and functions unchanged**, only the registration site moved).
   `CommandAppExt::register_command` now lazily inits `CommandRegistry`, so registration order no
   longer matters anywhere. `utils` keeps pure types + the `command!` macro.
6. **Unified `GameplayInput` context creation.** `setup_gameplay_input` and `character.rs` each built
   the `ContextActive` + `ContextActivity<GameplayInput>` tuple by hand. New
   `GameplayContextBundle` (Default = active) used in both places.

**Phase 3 — per-frame waste and consistency**

7. **Polling/direct field access in `integration/`.** `attach_camera_target` polled `Added<Character>`
   every frame → now an `On<Add<Character>>` observer. `sync_menu_pauses` reached into `PauseTracker`
   fields (`menu_pauses`, `last_reload_gen`) from outside `state.rs` → new `PauseTracker::sync_menus(
   any_open, reload_gen) -> bool` encapsulates the stale-count reset and resume decision; integration
   just acts on the return value.
8. **`dev.rs` per-frame scans + FPS coupling.** `update_debug_overlay_text` did a full
   `iter_entities` scan every frame to find its text node → the entity is now spawned first and stored
   in a `DebugOverlayTextEntity(Entity)` resource (root attaches it with `add_child`).
   `sync_debug_overlay_layout` hard-required `FpsOverlayConfig` → now `Option<Res<FpsOverlayConfig>>`.
   **Deviation:** the plan suggested dropping the FPS z-index height matching; kept it (the overlay
   still sits below the FPS graph) and made the config optional instead.
9. **Per-frame allocations in `dispatch_modifier_changes`** (`menus/dispatch.rs`). With no menu open
   the system early-returned without draining the mouse-wheel/motion readers and kept stale tracking
   state, so reopening a menu could fire a spurious or missed modifier event. Added a `resync` flag:
   on menu close the tracking state resets and readers drain; the first open frame after that always
   resyncs.
10. **`profiler` `CommandOutput` consistency + `find_command` ambiguity.** All four profiler commands
    only `info!`-logged, so command history showed nothing; they now also `output.set(...)` the same
    text (`profiler_report` folds summary + worst spike into one output). `CommandRegistry::find_command`
    silently picked the first suffix match when several commands shared a shorthand; it now returns
    `None` and warns, forcing the full name.

### Test-infrastructure discovery

The remap regression test initially panicked with "Encountered an error in observer … Resource does
not exist". Root cause: **`App::update()` never calls `App::finish()`** (only the default runner does),
and `EnhancedInputPlugin::finish` is what consumes `ContextRegistry` and inits
`ContextInstances<PreUpdate>` — the default context schedule is `PreUpdate`. Spawning `GameplayInput`
auto-inserts its required component `ContextPriority<GameplayInput>` (required-components mechanism),
whose `register` observer needs that resource. Fix: test apps that use `InputPlugin` must call
`app.finish()` before the first `app.update()`.

### Audit

`cargo check --all-targets` clean, no warnings. `cargo test` → **87 passed** (baseline was 78; nine
tests added: remap-same-frame, click-exactly-once, root right-click, docgen grouping, two
screenshot_camera validation tests, lazy registry init, ambiguous shorthand, sync_menus reset/resume).
Lua contract surface verified unchanged: no namespace, function, or callback renames anywhere; the
`commands` namespace moved files but kept its name and members.

### Behaviour changes

- Ambiguous command shorthand (e.g. two `…>toggle` commands) now **fails with a warning** instead of
  silently running the first match.
- Profiler commands now write their messages into command history (`CommandOutput`), visible via
  `commands.history()` in Lua.
- `screenshot_camera <name>` rejects names that belong to non-cameras (error output, no request).
- DOM clicks/hover on interactive nodes dispatch their handler **once** (was twice); root-level
  `dom.on` bindings now work; a `dom.on` onto an entity without `DomHandlers` logs a warning.
- The debug overlay text entity is spawned visible (no `Visibility::Hidden` dance).
- Command registration order no longer matters (lazy `CommandRegistry` init).
- Remapped bindings appear in `HotkeyRegistry`/`input.list_bindings` on the **same frame** as the
  remap (was next frame).

---

## Ticket 0001 — distance fields

Original instruction (verbatim, `tickets/incomplete/0001-distance_fields.md`):

> # Commands
> Do a thorough analaysis on how this can be done optimally. use websearch as well.
> - [ ] Add a command that enables/disables the custom distance fields for meshes.
>     - translucent materials do not generate a signed distance field.
>
> # Shaders
> Create utility wels files wherever possile. for example, the actual distance field could be a
> utility which could be used for ripples and such in the future and then a wels for the view.

### Analysis

**The engine provides nothing to toggle.** Bevy 0.20.1 has *no* mesh signed-distance-field system:
`mesh_sdf` (the per-mesh SDF atlas that `specular_transmission` materials used in 0.15/0.16) was
removed from `bevy_pbr` — a case-insensitive search for `mesh_sdf` across every bevy crate in the
local cargo registry returns zero hits. The ticket's sub-note "translucent materials do not
generate a signed distance field" is therefore exactly right for 0.20: nothing generates one, for
any material. "Custom distance fields" must mean a system this game owns end-to-end: bake → store →
bind → sample, with a console command gating it. (WebSearch is unavailable in this environment; the
analysis was done against the pinned bevy_pbr/bevy_render/bevy_shader/wesl 0.20.1/0.6.0 sources in
the cargo registry, which is the ground truth for this project anyway.)

**Which meshes.** The game's meshes are the terrain (a displaced `Plane3d` heightfield, 1000²
subdivisions ≈ 2 M triangles, analytic height via `Terrain::sample_ground_height`) and the water
plane (`specular_transmission: 1.0` — the translucent material the ticket calls out). Both are
heightfields: a single y per (x, z).

**Generation options considered:**

- *GPU per-mesh SDF atlas (the old Bevy approach):* render each mesh's SDF into a 3D atlas by
  raymarching triangles in a custom phase. General but heavy: atlas allocator, per-mesh pipeline,
  extract/prepare phases, ~600+ lines of render code. Rejected for now — our content is heightfields,
  where a cheaper exact method exists; revisit when arbitrary imported meshes need fields.
- *CPU brute-force triangle distance:* O(texels × triangles) = 10⁶ × 2×10⁶ — infeasible without a
  BVH/grid. Rejected.
- *Heightfield bake (chosen):* rasterize the mesh triangles into a 2D height grid (O(triangles),
  each ~1–2 grid cells), then fill a 3D texture with `sdf(x, y, z) = y − h(x, z)` (bilinear h).
  O(triangles + texels), exact for heightfields, ~100 ms for the terrain at startup. This is the
  standard vertical-distance SDF for heightfields: exact along vertical rays, slightly conservative
  on steep slopes — fine for water/shore effects and raymarch utilities.

**Representation:** a 3D `R32Float` texture + a bounds uniform. Matches the ticket's "distance
fields for meshes" framing, supports 3D sampling/raymarch utilities, and generalizes to non-heightfield
meshes later (the bake is the only part that changes). Anisotropic texel sizes (coarse horizontally,
fine vertically) keep the waterline band accurate without a 10⁸-texel grid. `R32Float` textures are
not filterable without the `TEXTURE_COMPONENT_…` float-filterable feature, so sampling is manual
trilinear via `textureLoad` — which belongs in the utility wesl anyway.

**Binding:** `ExtendedMaterial<StandardMaterial, E>` — the supported 0.20 path for attaching custom
shader data to PBR materials (bevy_render 0.20 exposes no public view-bind-group injection point;
`ViewBindGroup` no longer exists as a public type after the `combined_bind_group` overhaul). The SDF
is one global texture shared through the extension bind group, so the shader side is still written
as a view-wide module ("a wels for the view" per the ticket).

**Placement:** `src/terrain/distance_field.rs` (terrain submodule). The meshes that feed the field
and the material that consumes it (water) are both spawned by `TerrainPlugin`; a standalone feature
module would force a feature→feature import (terrain's water would need the extension type) or
integration indirection for zero current benefit. The machinery inside is mesh-generic (a
`DistanceFieldSource` component marks any entity whose mesh feeds the field), so extracting it into
its own module/service later is a file move, not a rewrite. Rejected: standalone `distance_field`
feature now (import-rule friction, speculative generality).

### Plan

1. **`src/terrain/distance_field.rs`** — the system:
   - `DistanceFieldConfig` resource: `enabled: bool`, texel-size constants, resolution helper.
   - `DistanceFieldSource` component: marks an entity whose `Mesh3d` feeds the field; attached to
     the terrain entity in `setup_terrain`.
   - Pure bake functions (unit-tested): `height_grid_from_mesh` (triangle → 2D max-height grid),
     `sdf_from_height_grid` (3D `y − h` fill), `bilinear_height`.
   - Bake system: when a source mesh is ready and the field is enabled and not yet baked, bake and
     upload into a `Handle<Image>` (3D R32Float) via `RenderWorld` `TextureData`; store bounds.
   - `DistanceFieldWater` material extension (`AsBindGroup`: uniform {bounds_min, bounds_max,
     enabled} + `texture_3d` + sampler) + fragment hook that modulates the water surface with
     shore-proximity ripples derived from the field (subtle; the field is the point, the ripples
     demonstrate consumption).
   - `command!(app, "Toggle custom mesh distance fields", toggle_distance_field)` →
     `terrain>toggle_distance_field`; flips `enabled`, writes `CommandOutput` ("Distance field:
     on/off"), updates the extension uniform.
   - `debug_info!` entry showing state + resolution.
2. **Shaders (`assets/shaders/`):**
   - `distance_field.wesl` — pure utility: manual trilinear `df_sample`, `df_gradient`,
     `df_raymarch`. No bindings. Reusable by any future shader (ripples, foam, refraction).
   - `distance_field_view.wesl` — the view module: declares the bound SDF texture/sampler/uniform
     and provides `distance_field_world(world_pos)` on top of the utility.
   - `water.wesl` — the extension fragment shader: imports both, applies shore ripples.
3. **Water entity** switches from `StandardMaterial` to
   `ExtendedMaterial<StandardMaterial, DistanceFieldWater>` (same base params).
4. **Tests:** height-grid rasterization on a synthetic mesh, SDF fill math, bilinear sampling,
   toggle round-trip. `cargo check --all-targets` + `cargo test` clean; game launches with no shader
   compile errors in the log (WGSL errors are loud at pipeline build).

### Plan audit

- *Is the water even visible?* The water plane (y = −0.1) currently sits 0.1 m **below** the terrain
  plateau (y = 0 across the inner rect), so it is occluded in today's build — the ripple demo will
  not be visible until the terrain/water levels are adjusted (terrain is under active editing; not
  this ticket's scope). Verification therefore leans on: clean shader compilation at runtime, the
  command toggling with output, and unit tests on the bake math. Noted, not fixed.
- *Does toggling off actually disable the field?* Yes at the sampling level (uniform flag gates the
  shader path) and at the bake level (no (re)bake while disabled). The texture stays bound — a
  1×1×1 placeholder before first bake keeps the bind group valid.
- *Command name:* `terrain>toggle_distance_field` (derived from module path; user types
  `toggle_distance_field`). No collision with existing commands.
- *WESL import mechanics between asset-path shaders:* confirmed against bevy_shader 0.20.1 —
  asset shaders get `ShaderImport::AssetPath("/<path>")` (absolute WESL module paths), so
  `assets/shaders/water.wesl` can `@import` its siblings by absolute path. Exact syntax pinned
  against the wesl 0.6 parser before writing shaders.
