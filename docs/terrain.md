# Terrain
- [1. Static world asset loading](#1-static-world-asset-loading)
- [2. World generation](#2-world-generation)
  - [Core](#core)
    - [Parameters](#parameters)
    - [Base Features](#base-features)
    - [Noise Features](#noise-features)
    - [Evolving Features](#evolving-features)
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
- A voronoi algorithm breaks down the world into sections. These sections are given a full set of arbitrary parameters but similar to neighbors so no jarring changes between sections.
- Each section to section boundry can roll its own cell_blend_width. 
- Each section is given a base color/texture and cliff color/texture based on its rolled region. At lower angles a simple merging between colors/textures is allowable. At steep angles cliff colors/textures are used. Accross section boundaries, the higher section's cliff color/texture is preferred.
- each section should also impact the atmosphere, blending between the two nearing boundaries.

#### Noise Features
This step involves introducing various noise algorithms to purturb the heightma and make it look more natural. The noise should be chosen per the parameters. 

<!-- AI markdown exception: Fill this out as you create more noise values-->
noise types:
- hilly noise is good for hills, dunes, ... (our current implementation is hilly)

#### Evolving Features
This step requires approximating or simuluting errosion, crater impacts, wind errosion and other evolutionary events.

- Use the base precipitation values per section to create a full precipitation map, which will then be used to simulate or approximate errosion. This step will create rivers, waterfalls, water basins, lakes

### Seasonal Features
This step handles features that develop due to cyclic changes in paramters 

- Required adding seasons {spring, summer, fall, winter}
- Each season updates the parameters or each base region by some delta
- Each season has its own sea level
- The vertices within the seasonal flood zone (submerged for part of the year) undergo coastal erosion—interpolating their height into a smooth gradient toward the water and overriding their biome material/texture.

### World Generation Types
- Create an infinite world (future)
- Create a finite world
- Create a finite torus world