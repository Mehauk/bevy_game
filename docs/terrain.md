# Terrain
- [1. Static world asset loading](#1-static-world-asset-loading)
- [2. World generation](#2-world-generation)
  - [Core](#core)
    - [Parameters](#parameters)
    - [Base Features](#base-features)
    - [Noise Features](#noise-features)
    - [Evolving Features](#evolving-features)
  - [Water](#water)
    - [Base Water (Sea Level \& Ocean)](#base-water-sea-level--ocean)
    - [Hydrological Water (Rivers, Waterfalls \& Lakes)](#hydrological-water-rivers-waterfalls--lakes)
    - [Shared Behaviour (Shading, Freezing \& Immersion)](#shared-behaviour-shading-freezing--immersion)
  - [Seasonal Features](#seasonal-features)
  - [World Generation Types](#world-generation-types)


## 1. Static world asset loading
future

## 2. World generation

### Core
The world generation is broken down into the following parts

#### Parameters
The following parameters are fed into the world generation algorithm:
- temperature enum {scorching, hot, warm, temperate, chilly, cold, freezing}
- humidity enum {saturated, humid, moderate humidity, dry, arid}
- wind enum {gale, windy, moderate windy, not windy}
- sun exposure enum {harsh sun, exposed sun, moderate sun, low sun, no sun}
- precipitation (handles all precipitation including rain and snow) enum {heavy precipitation, moderate precipitation, low precipitation, no precipitation}
- base height enum {abyssal, deep ocean, coastal shelf, sea level, lowland, highland, alpine, peak}
- base gradient; a vector describing the tilt of the section plane (slope magnitude {flat, gentle, moderate, steep, sheer} and sun/wind aspect)
- hydrology enum {dry, well drained, poorly drained, fluvial / channel, flooded / basin}
- substrate enum {sand, silt / clay, limestone / karst, rock / crystalline, volcanic}
- geothermal enum {none, geothermal, active volcanic}
- salinity (submerged, coastal, and closed basins) enum {freshwater, brackish, marine, hypersaline}
- substrate enum {sand, silt / clay, limestone / karst, rock / crystalline, volcanic} (defines ground hardness / erodibility and angle of repose)
- region (gated by all the above parameters)
	- Submerged (Abyssal / Deep Ocean / Coastal Shelf):
		- abyssal trench
		- deep ocean basin (abyssal plain / seamount / cold-water reef)
		- hydrothermal vent field (vents / cold seep / submarine lava)
		- kelp forest
		- coral reef
		- seagrass meadow
		- barren seabed / rocky shoal
		- sea ice / pack ice / ice shelf
	- Coastal & Sea Level:
		- mangrove swamp
		- tidal flat / salt marsh (mudflat / coastal salt pan)
		- sandy beach / dunes (barrier spit / fog coast)
		- rocky coast / sea cliffs (shingle beach / sea caves / headland)
		- river delta / estuary
		- frozen shore / glacial coast
	- Lowland:
		- tropical rainforest
		- tropical monsoon forest
		- tropical deciduous forest
		- savanna (tropical grassland / thornbush)
		- desert / barren (sand dunes / stony reg / fog desert / salt flat / cold desert)
		- wetland / marsh / bog (tropical swamp / floodplain / peat muskeg / lake)
		- temperate grassland (prairie / pampas / oak savanna / steppe)
		- temperate forest (temperate rainforest / deciduous / coniferous)
		- mediterranean scrubland (chaparral / maquis)
		- taiga (boreal forest / lichen woodland)
		- arctic tundra
	- Highland:
		- plateau / mesa (arid mesa / salar / snow plateau)
		- badlands / canyonlands (gorge / rocky escarpment)
		- highland steppe / desert (highland savanna / temperate steppe / cold desert)
		- montane forest (cloud forest / montane rainforest / coniferous / dry woodland)
		- highland moorland / heath (blanket bog)
		- karst / limestone hills
	- Alpine & Peak:
		- subalpine forest (coniferous / wind-stunted krummholz)
		- alpine meadow (meadow / tropical paramo / dry puna / alpine tundra / tarn)
		- scree / talus slope (sheer rock cliff / desert mountain)
		- glacier / icefield (perennial snowfield / valley glacier)
		- polar ice cap
		- volcanic caldera / lava field (fumarole / geothermal peak)

#### Base Features
This part of the generation is responsible for creating the structural guidline of the world. It will handle creating islands, continents, archipelegos and so on. As well as establish baseline parameters for the further steps.

- Start with a flat world at sea level. everything below is submerged in water
- A weighted voronoi algorithm breaks down the world into sections. These sections are given a full set of arbitrary parameters but similar to neighbors so no jarring changes between sections.
- Each section to section boundry can roll its own cell_blend_width. 
- Each section is given a base color/texture and cliff color/texture based on its rolled region. At lower angles a simple merging between colors/textures is allowable. At steep angles cliff colors/textures are used. Accross section boundaries, the higher section's cliff color/texture is preferred.
- each section should also impact the atmosphere, blending between the two nearing boundaries.

#### Noise Features
This step involves introducing various noise algorithms to purturb the heightma and make it look more natural. The noise should be chosen per the parameters. 

<!-- AI markdown exception: Fill this out as you create more noise values-->
noise types:
- hilly noise is good for hills, plains, grasslands and any rolling default terrain (fBm perlin, domain-warped); it is the fallback whenever nothing more specific applies
- ridged multifractal noise makes sharp alpine ridges, volcanic peaks and mountain ranges, because inverting the noise before squaring it turns perlin ridges into knife-edge crests
- dune noise is anisotropic billow stretched along the local wind vector, so sand seas, cold deserts and wind-blown snowfields get long parallel crests and slip faces
- terraced noise quantises the field into steps, giving plateaus, mesas, badlands and limestone karst their flat-topped benches
- billow noise produces rounded mounds for moorlands, wetlands, grassland hummocks and seabed kelp or coral formations
- cellular (worley) noise gives Voronoi cracks and blocky cells for volcanic lava fields, salt pans, dried mudflats and scree

Selection is per section from its rolled parameters (geothermal and substrate first, then elevation band, aridity, slope and hydrology), and the noise sampled near a boundary is blended between the two sections' algorithms over that boundary's blend width, so a cell never snaps from one algorithm to another.

#### Evolving Features
This step requires approximating or simuluting errosion, crater impacts, wind errosion and other evolutionary events.

- Use the base precipitation values per section to create a full precipitation map, which will then be used to simulate or approximate errosion. This step will create rivers, waterfalls, water basins, lakes
- Ground hardness derived from `substrate` (and subtle vertical strata variation) controls how strongly hydraulic, fluvial, and wind erosion carve the terrain—soft sand and silt erode into wide valleys, dunes, and floodplains, while hard crystalline and volcanic rock resist carving to form narrow gorges and waterfall ledges.

### Water
Water is handled in two distinct ways—elevation-driven base water and runoff-driven hydrological water—which then converge on a shared shader, freezing, and immersion interface.

#### Base Water (Sea Level & Ocean)
- Sea water is driven by the global seasonal `sea_level`.
- Only basins connected to the open ocean (or world perimeter) are flooded by base water; landlocked depressions that dip below sea level stay dry or form isolated basins rather than filling with seawater.
- Meshed per terrain chunk (only for chunks containing terrain below max sea level) rather than a single global quad, allowing `FiniteTorus` wrapping, `Infinite` chunk streaming, vertex depth/wave attributes, and localized sea ice.

#### Hydrological Water (Rivers, Waterfalls & Lakes)
 - Created during `Evolving Features` from the precipitation map, priority-flood depressions, and flow accumulation.
 - **Inland lakes / basins** sit at their resolved spill elevation, triangulated directly to the flooded basin footprint (plus a thin bank margin) rather than bounding boxes. Salinity is determined by drainage connectivity and aridity (freshwater when draining, brackish near coasts, hypersaline in closed arid basins).
-- **Rivers** follow smoothed paths along high flow accumulation from headwaters down to lakes or sea level. Ribbon width scales with local accumulation, cross-section edges tuck into the carved banks to avoid floating or z-fighting, and paths split/wrap cleanly across `FiniteTorus` borders.
+- **Rivers** trace interpolating splines along high flow accumulation from headwaters down to lakes or sea level, stitched at confluences into a single continuous mesh. Water surface elevation and left/right bank widths are solved from approximate water volume (discharge divided by slope-driven velocity) against approximate channel volume (integrated across the carved terrain cross-section), keeping narrow gorges deeper and floodplains wider while tucking bank skirts into the terrain and wrapping cleanly across `FiniteTorus` borders.
 - **Waterfalls** form at steep drops along river paths, carving downstream plunge pools and generating cascade sheets and plunge-pool foam.

#### Shared Behaviour (Shading, Freezing & Immersion)
- Both water systems share a water material supporting animated surface ripples (wind-driven on oceans/lakes, flow-driven on rivers), Fresnel reflection, depth-based colour absorption, shoreline foam, and double-sided rendering for underwater views.
- Freezing is evaluated locally from seasonal temperature, salinity, and geothermal activity (freshwater freezes before marine water; active volcanic and hydrothermal zones remain liquid), spawning walkable ice geometry and colliders only where water actually freezes.
- A unified water query exposes surface height, depth, flow direction, salinity, and frozen state across oceans, lakes, and rivers for underwater camera fog and character swimming/buoyancy.

### Seasonal Features
This step handles features that develop due to cyclic changes in paramters 

- Required adding seasons {spring, summer, fall, winter}
- Each season updates the parameters or each base region by some delta
- Each season has its own sea level
- The vertices within the seasonal flood zone (submerged for part of the year) undergo coastal erosion—interpolating their height into a smooth gradient toward the water and overriding their biome material/texture.
- The vertices within the seasonal flood zone (submerged for part of the year) undergo coastal erosion—interpolating their height into a smooth beach/tidal-flat gradient and overriding their surface material scaled by ground hardness (soft sand/silt wears easily into wet sand and mudflats, while hard stony/volcanic rock resists wave flattening to preserve sea cliffs and rocky/shingle shores).

### World Generation Types
- Create an infinite world (future)
- Create a finite world
- Create a finite torus world