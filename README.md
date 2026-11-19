# Game Engine

## Table of Contents <!-- omit from toc -->
- [Menus](#menus)
- [Performance](#performance)

## Menus

Game menus are defined with HTML and CSS and controlled via `lua54` scripts with hooks into the engine for reading and writing values. 

The menus can be as dynamic as you want, you can make the world map into the main menu or update the `HUD` with a permanent questlog.
*Note: although you can recreate the main menu in the middle of the game, some commands will not work (do nothing) or error out depending on the game state*

Currently there is a set number of menus; later a hook to read a list of menus, add new menus, and bind menus will be added

Go [here](docs/lua-hooks.md) for a full list of lua<->engine hooks 

## Performance
- Quadtree based terrain/object chunking and visible occlusion
- Quadtree based LOD for static items, animations, characters