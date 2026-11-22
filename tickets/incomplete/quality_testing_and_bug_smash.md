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
    feature, not terrain; [proof](target/shots/bug2_full.png): whole-world shot places it on the coast.
  - diagnosis: NOT a defect. The band is shoreline foam over a locally straight coastal segment. Depth was
    read directly: `inspect_point 30 90` → height 1.20 vs sea level 1.60 → depth 0.40 m < `FOAM_DEPTH`
    (0.8), so the shader's `depth_foam = 1 - smoothstep(0, foam_depth, depth)` correctly returns full foam.
    A heightfield scan of the sub-`FOAM_DEPTH` water along z=60..120 is only 1–4 cells wide per row and its
    x-range shifts with z (41, 44, 15–18, 12–30, 6–34, 0–37, 40) — a thin irregular shoreline strip, not a
    straight seam. The chunk grid tiles exactly at x=0 (chunk_bounds share the edge at 0.0, no overlap/gap)
    and the water uniform is global (no per-chunk centre), so there is no chunk-seam discontinuity. The
    apparent straightness is the coastline geometry there, and the brightness is `FOAM_DEPTH` tuning — a
    cosmetic tuning choice, not a correctness defect. No bandaid applied.
  - [x] fix: none required (corrected misdiagnosis; cosmetic foam-width tuning is a design decision).

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

Every issue recorded this session was traced to root cause with runtime reads (BRP `inspect_point`,
`world_shot`, `set_season`) and a dominant-layer histogram over the live config. None is a clean
correctness defect: bugs 1, 2 and 4 are correct behaviour (two were prior misdiagnoses), and bug 3 is a
control-resolution-vs-perf-budget design tradeoff. Per the no-bandaid rule no cosmetic/aesthetic change
was applied to "fix" non-defects. The ticket stays open because its premise (find defects to fix) did not
yield a safely-fixable defect this session; the next session should probe gameplay logic (state machine
transitions, console command edge cases, hydrology edge cases) rather than graphics, which has now been
swept.
