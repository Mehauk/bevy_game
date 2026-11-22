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

- [ ] bug 1: deep-ocean top-down / grazing captures read as a flat grey sky-mirror with no depth gradient.
  - [proof](target/shots/compare_main.png): uniform grey water, hard bank edge.
  - diagnosis: NOT a defect. `water_material.wesl` already does depth-gradient body colour, shoreline
    foam and fresnel reflection, and the sea mesh bakes per-vertex depth (`water.rs:458`). Deep water at a
    grazing angle correctly mirrors the sky. The reference lake (`target/video_ref/frames/water_12.jpg`)
    is shallow, which is why it shows the gradient. A shallow-lake capture is the correct comparison.
  - [ ] fix: none required (corrected misdiagnosis).

- [ ] bug 2: a bright near-vertical light line appears along the sea in the NE top-down capture.
  - [proof](target/shots/sweep_NE.png): light line near the coast at focus (90,90).
  - diagnosis: not yet root-caused — likely the shoreline foam strip or a sea-chunk mesh tile edge; needs
    a `surfaces.rs`/`water.rs` mesh-boundary trace before any change (no bandaid).
  - [ ] fix: pending diagnosis.

- [ ] bug 3: terrain surface blend shows a regular checkerboard block pattern at moderate zoom.
  - [proof](target/shots/sweep_SW.png): checkerboard across the green field at focus (-90,-90) zoom 3.
  - diagnosis: the splat control map is baked at `SPLAT_RESOLUTION = 128` per chunk; the weights texture
    is already linear-sampled, so the blocking is genuine control-resolution limit, not a filtering bug.
    A proper fix raises control resolution, which collides with the `splat_bake` perf budget (the bench
    test) — a resolution/budget design decision, not a safe one-line change.
  - [ ] fix: pending a resolution-vs-budget decision.

- [ ] bug 4: Dune, Snow and Volcanic never dominate any texel in the default world (no snow caps on the
  rock summits the isolation pass reveals).
  - [proof](docs/.compiled/terrain-previews/layer_snow_seed-1337_finite.png): fully black, vs
    `layer_rock_seed-1337_finite.png` showing summits.
  - diagnosis: on steep summits `cliff_share` is ~1 so `(1-cliff)*snow` ~0 and snow cannot dominate steep
    rock — plausibly correct. Needs a region/temperature distribution check to confirm whether any cell
    should produce a snow-dominant flat summit before treating it as a bug.
  - [ ] fix: pending diagnosis.
