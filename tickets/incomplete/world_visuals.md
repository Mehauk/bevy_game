**do after shaders**

This ticket is to check the correctness of world. You will zoom in to various parts and examine them. look for irregularites such as seams.
- [x] create an ability to take a screenshot of the world from the topdown:
  - [x] full rendered world with water, rivers, lakes, everything even stuff that will be added later.
  - [x] the full rendered world with the river splines overlayed
  - [x] only the rivers renderd
  - [x] only the rivers rendered with splines overlayed
  - [x] rivers and lakes rendered
  - [x] a series of images that only shows one terrain type (texture) (wil be used to check how the terrain blends into each other)
  - [x] two images showing the world without water and the world with water to be able to inspect if underwater textures are correctly set
  - [x] images of the character inside various water fixtures to check the shader interactions
  - [x] two images that compares where the water is and where the meshes are (should overlap exactly)

- [x] also compare with the images from the videos to check for similarity

---

find and fix 100 inconsistencies with the following format

number. [ ] issue: My Issue
    - proof: [inline-image]
    - optional comparison: copraison explanation [inline-image-2]
    - plan:
    - fix:

---

---start here---

## Top-down capture ability

Implemented in `src/integration/world_shot.rs`, registered by `IntegrationPlugin`. Console command
`world_shot <mode>` spawns a top-down orthographic camera (world-fitted, fog disabled per-frame after
`sync_terrain_fog`) and toggles mesh visibility by entity `Name`. Modes: `off`, `full`,
`full_splines`, `no_water`, `rivers`, `rivers_splines`, `rivers_lakes`. `off` despawns the shot camera
and restores the main camera. Proof images live in `target/shots/`.

Drive it from BRP by setting `AgentConsole.request` to e.g. `world_shot full`, then screenshot with
`camera=<shot-cam-entity>` (the shot camera is the ortho camera at y=600).

## Findings

1. [x] issue: `no_water` mode rendered sea, lakes and rivers (identical to `full`), so the
   "world without water" inspection image was impossible.
    - proof: `target/shots/no_water_1.png` (water still present) vs `target/shots/m_no_water.png` (after fix, dry)
    - plan: `shows_sea`/`shows_lakes`/`shows_rivers` wrongly matched `NoWater`.
    - fix: predicates corrected so `NoWater` shows terrain only.

2. [ ] issue: from directly above the ocean is indistinguishable from the sky — calm water reflects the
   sky gradient and sun disc, so "where is the water" cannot be read off a top-down shot.
    - proof: `target/shots/m_full.png`, `target/shots/chunk00_detail.png`
    - plan: a top-down water read needs a depth/coverage tint, not the live reflection.
    - fix: deferred — add a `world_shot` depth-shading mode or a bathymetry preview.

3. [x] issue (resolved by the finding-10 generation fix): the three `Sea Surface` meshes only covered the
   eastern half of the world (world x>=0 plus a sliver); the western half had no sea-surface mesh.
    - proof: pre-fix AABB rects via `world_query` — Sea Surface 4294966409 = x[0,200]z[0,200],
      4294966411 = x[0,200]z[-200,0], 4294966410 = a ~23x158 sliver. Post-fix the sea meshes tile the whole
      world (see finding 10).
    - plan: (was) confirm with `inspect_point` over west sub-sea-level cells, then check
      `surfaces::rebuild_water_surfaces` region partitioning.
    - fix: not a partitioning bug. The meshes were built from the compact startup placeholder map and never
      rebuilt because the delivered real map reused `generation == 1`. The finding-10 `GENERATION_COUNTER`
      fix makes the real map's delivery bump the generation, so `rebuild_water_surfaces` rebuilds all four
      sea chunks over the full world.

4. [x] issue (resolved by the finding-10 generation fix): the single `River Network` mesh AABB was only
   ~63x44 m near the origin although the world reports 74 rivers across a ~330 m continent.
    - proof: pre-fix `world_query` AABB rect (532,297,114,80) for entity 4294966407. Post-fix the river
      mesh spans the world (7895 verts, full extent) — see finding 10.
    - plan: (was) verify whether river ribbons are merged into one mesh or most rivers lack geometry.
    - fix: same root cause as finding 10 — the river mesh was placeholder-built and stale; the generation fix
      rebuilds it from the real hydrology.

5. [x] issue (resolved by the finding-10 generation fix): only one `Lake Surface` entity existed (~12x18 m)
   although the world reports 21 lakes.
    - proof: pre-fix `world_find_entities_by_name` "Lake Surface" returned a single entity (4294966408).
      Post-fix 21 `Lake Surface` entities exist, matching `world_summary` — see finding 10.
    - plan: (was) check whether lakes are merged into one mesh or only one lake got a surface.
    - fix: same root cause as finding 10 — placeholder-built water; the generation fix rebuilds every lake.

6. [ ] issue: the submerged view is a flat featureless grey-blue murk — no caustics, light shafts or
   see-through-to-bed, because `WATER_MURKINESS` is 1.0.
    - proof: `target/shots/char_waterfall.png` (character fully underwater).
    - plan: lower murkiness / enable underwater caustics for inspection.
    - fix: deferred — gameplay tuning, not a safe blind change.

7. [ ] issue: at a grazing angle the lake surface meets the shore with a hard opaque edge and no foam or
   wet-sand blend.
    - proof: `target/shots/char_lake.png`.
    - plan: shoreline foam term keyed to water/land intersection depth.
    - fix: deferred.

8. [x] issue (corrected — misdiagnosis): my read that Dune, Snow and Volcanic never dominate a texel was
   wrong. A dominant-layer histogram over the live config (seed 1337, 25600 samples) disproves it:
    - Dune DOES dominate (166 texels, 0.65%); desert-interior texels read `Dune=1.00`. The "fully black"
      isolation image was a sparse layer misread as empty.
    - Snow is SEASONAL. The isolation preview was a Summer snapshot (`SeasonClock::default()` = Summer,
      seasonal=0); the summer snowline is 68 m while the world max is 54.9 m and no cell is cold enough, so
      summer snow is correctly absent. In Winter the runtime shader drops the snowline to ~54 m and caps the
      highest ridges (`target/shots/winter_full.png`).
    - Volcanic is absent because this seed has no volcanic region at all (region histogram has no
      "volcanic caldera" entry) — a correct absence.
    - proof: `target/shots/winter_full.png` (winter snow caps present); dominant-layer histogram over the
      live config.
    - plan: none — not a world-gen defect. The isolation preview's binary dominant-layer view is misleading
      for sparse/seasonal layers (a tooling note for the preview, not a generation bug).
    - fix: corrected the finding; no code change. Cross-referenced as bug 4 in
      `tickets/incomplete/quality_testing_and_bug_smash.md`.

9. [x] issue (corrected): my initial read that "our water has no depth gradient / reflection / foam" was
   wrong. On inspection `assets/shaders/water_material.wesl` already implements all three: depth-driven
   body colour (`transmit = exp(-depth*absorption)`, `body = mix(deep, shallow, transmit)`, lines 208-209),
   shoreline foam (`depth_foam = 1 - smoothstep(0, foam_depth, depth)`, line 221), and fresnel sky
   reflection (lines 236-238). The sea mesh bakes per-vertex water depth into `COLOR_0.r`
   (`src/terrain/water.rs:458`), which the shader reads as `vdata.r`. The flat grey look in the deep-ocean
   grazing captures is physically correct (deep water at a grazing angle reflects the sky via Fresnel);
   the reference `water_12.jpg` is a *shallow* lake, which is why it shows the gradient and bed.
    - proof: `assets/shaders/water_material.wesl:208-246`, `src/terrain/water.rs:450-458`.
    - plan: none — not a defect. A shallow-lake capture (not deep ocean) is the correct comparison.
    - fix: corrected the finding; no code change needed. Findings 2/6/7 (murk, hard shoreline) remain the
      real, separate water issues.

10. [x] issue: a straight vertical white seam ran through open deep water in the NE quadrant — a real
    defect, not natural foam (foam follows shorelines, not straight lines across the open sea).
    - proof: `target/shots/sweep_NE.png` (vertical white line through open water at focus 90,90);
      `target/shots/x0_deepseam.png` (the band isolated); `target/shots/x0_despawn5.png` (the band vanishes
      when the stale sea mesh is despawned); `target/shots/x0_wide.png` (the band runs the full z to the
      world edge); `target/shots/x0_fixed.png` (band gone after the fix).
    - root cause: the startup placeholder `TerrainMap` (compact config) and the delivered real map both
      carried `generation == 1`, so the generation-keyed `rebuild_water_surfaces` (`surfaces.rs:62`) and
      `rebuild_terrain` (`chunk.rs:403`) never rebuilt after the real map was delivered. The placeholder's
      coarse 32×32 heightfield sampled across 400 m chunk bounds clamped to ~sea level, baking
      `depth≈0 → foam≈1` (white) along a chunk edge — a permanent white band at x∈[10.4,23.2]. The same
      staleness produced findings 3/4/5 and the blocky terrain scale.
    - fix: `TerrainMap::new` now draws a process-wide monotonic `GENERATION_COUNTER`
      (`src/terrain/map/mod.rs`), so every constructed map (placeholder, delivered, regenerate/reseed) has a
      unique generation and the real map's delivery invalidates the placeholder-built meshes. Verified at the
      data level (the previously-broken vertex now reads depth 10.58 / foam 0.00) and visually
      (`x0_fixed.png`). Regression test `every_map_gets_a_fresh_generation_number`.

11. [x] issue: the terrain surface blend showed a regular checkerboard blocking pattern — the top-4
    layer-index grid snapped per control texel, so layer transitions stepped on a grid.
    - proof: `target/shots/sweep_SW.png` (stark checkerboard on the placeholder field);
      `target/shots/sweep_SW_grid.png` (no-dither, real terrain) vs `target/shots/sweep_SW_dithered.png`
      (dithered) — the grid is gone on the real terrain.
    - diagnosis: two compounding causes. (a) The stark grid in the original capture was a placeholder
      artifact (same root cause as findings 3/4/5/10): the flat placeholder field made the index grid pop
      as clean squares; the real terrain's varied splat weights already smooth it. (b) The residual grid is
      the top-4 index grid: `splat_indices` is read with `textureLoad` at an integer texel
      (`terrain_material.wesl`), so the *identity* of the four blended layers snaps per 128×128 texel even
      though the weights are linearly sampled (so it was never a filtering bug). Indices can't be
      interpolated.
    - fix: dithered two-texel index blend (`dithered_layer_indices` in `terrain_material.wesl`): along the
      dominant texel axis, pick the current texel's or the neighbour's layer set per-pixel via a world-space
      hash thresholded by the sub-texel fraction, so the layer-set boundary cross-fades over one texel
      instead of snapping. `SPLAT_RESOLUTION = 128` and the `splat_bake` budget are unchanged.

## Per-layer isolation pass (new)

`export_preview layers` (and `export_preview layer_<name>`) now renders one image per `SurfaceLayer`,
painting each texel with that layer's albedo only where it is the dominant layer (`plan_texel` mix
dominant) and black elsewhere. Implemented in `src/terrain/preview.rs::layer_isolation` over new public
helpers `splat::dominant_surface_at` and `splat::albedo_at`. Output:
`docs/.compiled/terrain-previews/layer_{sand,dune,silt,gravel,rock,moss,soil,volcanic,snow}_seed-*.png`.
This is what surfaced finding 8 (dune/snow/volcanic never dominate).

## Video comparison (new)

Reference frames extracted with `yt-dlp` + `ffmpeg` into `target/video_ref/frames/` (river: 105 frames,
water: 36 frames, one per 20 s). The videos are UE5 editor screen-recordings (material graph + plane
preview), not rendered worlds; the usable rendered-water frames are the lake/pond viewport shots
(e.g. `water_12.jpg`). Comparison against our captures is recorded as finding 9.

## Sweep coverage

Systematic top-down sweep performed: full world (`sweep_full.png`) plus zoomed quadrant captures at
focus (90,90) `sweep_NE.png` and (-90,-90) `sweep_SW.png`, zoom 3 (~130 m/frame). New findings 8–11
recorded from these.

Status of the 11 evidenced findings: 7 are now closed — findings 3, 4, 5, 10 and 11 trace to the stale
placeholder-built meshes from an unchanged `generation` (fixed by the `GENERATION_COUNTER` change in
`src/terrain/map/mod.rs`; finding 11's residual grid additionally smoothed by the dithered index blend in
`terrain_material.wesl`), finding 8 was a misdiagnosis (corrected), and finding 1 was fixed earlier. That
leaves 4 open: findings 2, 6, 7 (top-down water read, underwater murk, grazing-angle shoreline blend —
shading/design tuning).

The "100 inconsistencies" target is not met: 11 issues evidenced, 7 closed. The exact next regions to
sweep on resume are the NW and SE quadrants at zoom 3, the full chunk-boundary grid at zoom ≥4 (now that
the meshes are rebuilt from the real map and worth re-inspecting), and the river/lake mesh edges at high
zoom. Findings 2/6/7 need a shading-look design decision before a non-bandaid fix is possible.