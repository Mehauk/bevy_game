 Model 12:05 PM

You hit the exact mathematical conflict in coast.wesl / water.wesl:

    water.wesl decides culling (discard) against water.water_height (a constant plane), not the wave surface.

    Meanwhile, swell_height_shored oscillates below water_height in troughs, and coastal_surface_y pulls shallow/dry vertices toward terrain_y + wash (which collapses to terrain_y + 0.0 at rest_depth <= 0), leaving the water mesh coplanar with the y = 0.0 flat plain whenever sea_level is near 0.0 (-0.15..+0.5).

Here is how AAA games (Sea of Thieves, Unreal Water, Horizon, Assassin’s Creed) solve this cleanly.
How AAA Games Solve Wave-vs-Floor Clipping

AAA engines avoid trying to "cut" the shoreline with discard against water_height or lifting dry water vertices onto terrain_y. Instead, they combine four rules:
1. Never lift dry water vertices to terrain_y — keep the mesh at water_height + wave

Right now in coast.wesl, coastal_surface_y blends toward terrain_y + wash as rest_depth -> 0. That actively drags the water mesh up to y = 0.0 even when water_height = -0.1, causing it to Z-fight and clip across the entire flat plain.

Instead, always compute water height relative to water.water_height, never relative to terrain_y:

    Where land is dry (water_height < terrain_y), the water mesh naturally sits below the terrain (y = water_height), and the hardware Z-buffer hides it automatically at pixel precision with zero clipping.

    Only positive wave crests/swash (y = water_height + crest > terrain_y) ever emerge above the terrain.

2. Clamp / bias shallow troughs so a wave can never dip below the seabed

Open-ocean Gerstner/sine waves oscillate [-A, +A] around water_height. In shallow water (0 < rest_depth < shoal_depth), real waves cannot form troughs deeper than the water column — and if they do, the seabed pokes up through every trough.

AAA water shaders enforce this in the vertex shader in two ways:

    Depth-clamped troughs (simplest & exact):
    Wesl

    // Trough can never drop below the seabed (keep a tiny clearance above terrain_y)
    let min_swell = -max(shore.rest_depth - 0.03, 0.0);
    let clamped_swell = max(swell, min_swell);

    Once rest_depth <= 0.03, clamped_swell >= 0.0 — troughs flatten out instead of punching holes through the floor.

    Crest-biased shallow waves (Unreal Water / Sea of Thieves style):
    As rest_depth gets shallow, blend waves from symmetric sin(phase) ([-1, +1]) to positive-only 0.5 + 0.5 * sin(phase) ([0, +1]), or smoothly clamp negative offsets with shore_vert_damp. Real surf-zone bores sit on top of mean sea level rather than carving troughs below it.

3. Separate "Basin Culling" from "Shoreline Culling"

In water.wesl, spill_height_at is meant to prevent the ocean from flooding disconnected inland valleys behind mountains — not to trim the coastline.

    Basin cull (water.wesl): Keep if (water.water_height + max_crest < spill_height_at(ctx.p.xz)) { discard; } purely conservative (max_crest = max possible wave + swash height above water_height), so it only culls dry inland basins that even the highest crest cannot reach.

    Shoreline trim (Z-buffer + depth fade): Do not cull the shoreline using water_height vs terrain_y. Let the Z-buffer cut the mesh wherever water_y < terrain_y, and if you want to eliminate any 1-pixel Z-fighting seam right along water_y ≈ terrain_y, add a tiny vertex-or-fragment edge fade (or switch shallow edges to fade alpha over the first 2–5 cm of water depth ctx.p.y - terrain_y).

4. Put thin shoreline swash in the terrain shader, not the water mesh (Horizon / Ghost of Tsushima / RDR2 style)

Trying to make a 1.17 m water mesh triangle ride 5–15 cm cleanly above a flat or curved floor will almost always chord-clip when a wave crest/trough spans across vertices.

AAA games split the problem at the waterline:

    Water mesh: Handles deep/shallow waves (water_height + clamped_swell, always >= terrain_y where flooded and <= water_height where dry).

    Terrain shader (wet_sand.wesl): Renders the paper-thin swash film and foam lip directly on the terrain surface just above sea_level (world_pos.y ∈ [sea_level, sea_level + swash_runup]). Because it shades the terrain's own geometry, triangle chord clipping and Z-fighting are geometrically impossible, and retreating waves naturally reveal dark glossy wet sand behind them.

Concrete Fix for Your Current Shaders

If you want to fix clipping immediately inside your existing coast.wesl + water.wesl setup (even before moving swash into wet_sand.wesl):

    In coast.wesl (terrain_height_at): Bilinearly interpolate the .r channel (ground height) across the 4 nearest texels instead of nearest-neighbor textureLoad, so terrain_y doesn't stair-step by ~0.58 m on the crater slope.

    In coast.wesl (coastal_surface_y): Remove mix(offshore_y, terrain_y + wash, near_land) — never anchor vertices to terrain_y. Instead:

        Compute offshore = swell_height_shored(swells, rest_xz, time, shore) (which already attenuates to 0 at rest_depth <= 0).

        Clamp troughs above the seabed where flooded: offshore = max(offshore, -max(shore.rest_depth - 0.03, 0.0)).

        Optionally add a purely positive swash pulse only near the shoreline (shore_factor > 0):
        let swash = max(0.0, raw_wave_pulse) * shore_runup_mask;

        Return water.water_height + offshore + swash.

        Result: on dry flats (water_height = -0.1, terrain_y = 0.0), water without a swash crest stays at -0.1 (cleanly under the floor, culled by Z-buffer); when flooded (water_height = +0.5), troughs are clamped above 0.03 m off the floor; and wave crests can still surge smoothly up onto the shore without pulling neighboring troughs under terrain_y.

    In water.wesl: Use water.water_height + COAST_SWASH_RUNUP < spill_height_at(ctx.p.xz) strictly for disconnected basins, and let the Z-buffer (optionally with if (ctx.p.y < terrain_y - 0.01) { discard; } if AlphaMode::Opaque shows grazing artifacts) handle the exact waterline.