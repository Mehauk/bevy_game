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
  (`docs/lua-hooks.md`), DRY helpers (`script_map`, `broadcast_to_menus`, the `MODIFIER_KEYS`
  table, the `command!` macro), and unit tests on all the pure logic (camera math, character
  physics, terrain generation, DOM spec parsing, command registry, key round-trips).

**Assessment:** this is a high-quality codebase. The obvious structural wins are already taken.
The remaining work is subtler: contract accuracy, modularity of the two largest files, and small
surface reductions. I am deliberately avoiding churn — every change below is justified against a
concrete goal, and rejected options are recorded with reasons.

---

## Findings

| # | Severity | Finding |
|---|----------|---------|
| F1 | **Med (contract)** | `docs/lua-hooks.md` documents `state.pause(source)` / `state.resume(source)` with a `source` parameter. The Rust impl (`src/state.rs`) registers both with **no** arguments and uses a reference-counted `PauseTracker` (menu-pause count + manual flag), not per-source tracking. The bundled scripts call `state.pause()` / `state.resume()` with no args. The doc overstates the API — a mod author following the doc would believe per-source pause ownership exists when it does not. |
| F2 | **Low (modularity)** | `src/terrain/mod.rs` is 647 lines mixing two unrelated concerns: a pure, dependency-light heightfield domain model (`TerrainMap`/`TerrainCell` + Voronoi/noise generation + height sampling) and Bevy-specific rendering/fog/atmosphere/skybox systems + console commands. The domain model has no ECS dependencies and is independently testable, but is buried in the rendering file. |
| F3 | **Low (surface)** | `input::set_context_active` and `input::keys::parse_key_code` are `pub` but used only within their own module. |

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
   `docs/lua-hooks.md` to describe the actual no-arg, reference-counted behaviour.
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

- **F1 (doc contract).** `docs/lua-hooks.md`: rewrote the `state` namespace table so
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

| # | Severity | Finding |
|---|----------|---------|
| F4 | **Low (dead code + contract)** | A *menu input context* was half-wired: `InputPlugin::build` registered `register_hotkey_context::<MenuInput>("menu")`, `MenuInput` was a defined marker component, and `sync_context_activity` queried `With<MenuInput>` and toggled its activity on every menu open/close. But **no entity is ever spawned with `MenuInput`** (grep: nothing spawns/inserts it), so the query is always empty and the toggle loop is dead. The same phantom leaked into the contract: `docs/lua-hooks.md` told mod authors `input.list_bindings()` could report `context == "menu"`, a value that can never appear. |
| F5 | **Low (contract)** | `right_click` is a live DOM event — emitted in `menus/dom/events.rs` for the secondary mouse button (a deliberate separate event so menus can dismiss without firing `click`), and actually used by `assets/menus/keyboard/script.lua`. It was missing from the `dom.on` event list in `docs/lua-hooks.md`. |

### Plan

1. **F4.** Delete the vestigial menu context end-to-end: the `register_hotkey_context::<MenuInput>`
   call, the `MenuInput` marker struct, the `q_menu` query param, and the dead toggle loop in
   `sync_context_activity`. Update the module doc (it framed contexts as "gameplay vs menu"). In
   `docs/lua-hooks.md`, drop the unreachable `"menu"` value from `input.list_bindings()`.
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
- `docs/lua-hooks.md`: `input.list_bindings()` `context` is now `"gameplay"` or `"unknown"`;
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
