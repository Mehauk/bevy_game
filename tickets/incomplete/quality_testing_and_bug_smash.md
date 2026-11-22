**do second last**

For this ticket you will play through the game can record any issues you find. look for visual irregularities, gameplay bugs, and bad graphics. After recording a few bugs per session you will fix those bugs (no bandaid fixes).

---

## Playtest session log

Driven live over BRP (pid 21732). Verified working: WASD walk, Shift sprint, Space jump, swimming/buoyancy
(capsule floats at the waterline), the `~` console (history, autocomplete bar, copy buttons, single
`RunCommand` path), and the top-down `world_shot` capture modes. No crashes, no asset-load errors; the
runtime log contains only profiler frame-spike warnings (render/present and fixed-step catch-up), no
correctness errors.

The graphics issues surfaced during the playtest are recorded and cross-referenced in
`tickets/complete/0008-world_visuals.md`. On proper diagnosis most of them are correct behaviour, not
defects (see below), so per the no-bandaid rule they are not "fixed" here.

- [x] bug 1: deep-ocean top-down / grazing captures read as a flat grey sky-mirror with no depth gradient.
  - [proof](target/shots/compare_main.png): uniform grey water, hard bank edge.
  - diagnosis: NOT a defect. `water_material.wesl` already does depth-gradient body colour, shoreline
    foam and fresnel reflection, and the sea mesh bakes per-vertex depth (`water.rs:458`). Deep water at a
    grazing angle correctly mirrors the sky. The reference lake (`target/video_ref/frames/water_12.jpg`)
    is shallow, which is why it shows the gradient. A shallow-lake capture is the correct comparison.
  - [x] fix: none required (corrected misdiagnosis).

- [x] bug 2: a bright near-vertical light line appears along the sea in the NE top-down capture.
  - [proof](target/shots/bug2_ne.png): bright vertical foam band just offshore at focus (90,90) zoom 2;
    [proof](target/shots/bug2_nowater.png): the band is gone with water hidden, so it is a water-surface
    feature, not terrain; [proof](target/shots/x0_fixed.png): band gone after the fix.
  - diagnosis: THIS IS A DEFECT (the earlier "not a defect / shoreline foam" read was wrong). The band is a
    straight seam through open deep water, not a shoreline. Root cause: the startup placeholder `TerrainMap`
    (compact config) and the delivered real map both carried `generation == 1`, so the generation-keyed
    `rebuild_water_surfaces`/`rebuild_terrain` never rebuilt after the real map arrived. The placeholder's
    coarse heightfield sampled across 400 m chunk bounds clamped to ~sea level, baking `depth≈0 → foam≈1`
    (white) along a chunk edge. This is the same defect as finding 10 in
    `tickets/incomplete/world_visuals.md`.
  - [x] fix: `TerrainMap::new` now assigns a process-wide monotonic `GENERATION_COUNTER`
    (`src/terrain/map/mod.rs`), so the delivered map's generation differs from the placeholder's and the
    water/terrain meshes rebuild from the real map. Verified visually (`x0_fixed.png`) and at the data level;
    regression test `every_map_gets_a_fresh_generation_number`.

- [ ] bug 3: terrain surface blend shows a regular checkerboard block pattern at moderate zoom.
  - [proof](target/shots/sweep_SW.png): checkerboard across the green field at focus (-90,-90) zoom 3.
  - diagnosis: the splat control map is baked at `SPLAT_RESOLUTION = 128` per chunk; the weights texture
    is already linear-sampled, so the blocking is genuine control-resolution limit, not a filtering bug.
    A proper fix raises control resolution, which collides with the `splat_bake` perf budget (the bench
    test) — a resolution/budget design decision, not a safe one-line change.
  - [ ] fix: pending a resolution-vs-budget decision.

- [x] bug 4: Dune, Snow and Volcanic never dominate any texel in the default world (no snow caps on the
  rock summits the isolation pass reveals).
  - [proof](target/shots/winter_full.png): winter whole-world shot shows snow caps on the high ridges.
  - diagnosis: MISDIAGNOSIS, disproved by a dominant-layer histogram over the live config
    (`TerrainConfig::default()`, seed 1337, 25600 samples):
      - Dune DOES dominate — 166 texels (0.65%); desert-interior texels read `Dune=1.00` (e.g. (-200,60)).
        The earlier "fully black" isolation image was a sparse layer misread as empty.
      - Snow is SEASONAL. The isolation preview was a Summer snapshot: `SeasonClock::default()` is Summer
        (`seasonal=0`), the summer snowline is 68 m while the world's max height is 54.9 m (altitude term
        = 0), and no cell is cold enough for the temperature term (`snow_positive=0` in summer). In Winter
        the runtime shader snow overlay drops the snowline to ~54 m and caps the highest ridges (see proof).
      - Volcanic is absent because this world has no volcanic region at all (region histogram has no
        "volcanic caldera" entry) — correct absence, not a bug.
  - [x] fix: none required (corrected misdiagnosis; the isolation preview's binary dominant-layer view is
    misleading for sparse/seasonal layers, which is a tooling note for the preview, not a world-gen defect).

---

## Session outcome

Every issue recorded was traced to root cause with runtime reads (BRP `inspect_point`, `world_shot`,
`set_season`, a temporary water-mesh vertex dump) and a dominant-layer histogram over the live config.
Bug 2 was a genuine correctness defect — stale placeholder-built water/terrain meshes from an unchanged
`generation` — and is now fixed at the root (`GENERATION_COUNTER` in `src/terrain/map/mod.rs`, regression
test `every_map_gets_a_fresh_generation_number`); it is the same defect as finding 10 in
`tickets/incomplete/world_visuals.md`. Bugs 1 and 4 are correct behaviour (prior misdiagnoses, corrected),
and bug 3 is a control-resolution-vs-perf-budget design tradeoff. The ticket stays open for bug 3 (needs a
resolution/budget decision) and for the next session to probe gameplay logic (state machine transitions,
console command edge cases, hydrology edge cases) rather than graphics, which has now been swept.
