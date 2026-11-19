- [ ] checkout runevision (https://github.com/runevision)

# Menus
- [x] setup html+lua menus
- [x] setup commands menu
  - [x] fix bug with flickering
  - [x] arrow keys (up/down) still not working
    - [x] The up/down arrow keys are still being used in the text input
    - [x] the highlighting is also not working anymore
  - [x] mouse should stay captured if it was already
  - [x] dont allow mouse here (not for select or text editing)
  - [x] highlight letters (handling proper order)
  - [x] keep text input box centered (no move) (should help with the flickering)
- [x] when i edit a component's htmls file, the ui goes blank, I have to go back to the index and press save then it comes back

# Command menu rework
- [x] command menu
  - [x] open a bottom sheet similar to skyrim
  - [x] when you start typing show suggestion to autocompelte as well as a list of items to choose from with the arrow keys 
  - [x] add commands to increase character base speed and sprint
    - [x] this command will take parameters space separated
- [ ] make the command list scrollable keeping the selected one at the bottom

# Hotkeys
- [x] use bevy_enhanced_input
  - [x] show bound keys on a vitual keyboard overlay (new command)
    - [x] bound keys light up and have a short name/desc (max two lines, anything longer just becomes ...)
    - [x] key board should show other hotkeys when different modifiers are pressed (shift, cntrl, shift + cntrl, ...)
    - [x] keybinds inactive during menus (can remove menu gaurd in grab_cursor)
- [x] also show mouse key binds
  - [x] handle mouse wheel scroll versus click
- [x] update keybinds by clicking on an already defined hotkey (should take into account modifiers) and then selecting another key with your mouse or keyboard (again should handle modifiers).
  - [x] should swap not just overwrite
  - [x] gameplay keybinds only
- [x] pressing anything on the mouse or keyboard should light up those places similar to the modifier keys but without changing view (only modifiers do that)
- [x] block any movement keys from ever being part of any hotkey chain
  - [x] also handle the case when the movement keys are remapped
    - [x] might also need to throw a warning if trying to remap movement keys to a key that is part of a hotkey chain
    - [x] BUG: when trying to move a key shift + f to f it doesnt work (probably doesnt work other way either)
      - [x] Also when i click a key to rebind it stays higlighted even when i move to differen un/modifier views
- [x] Move all the keybinding `rules` out of the game engine and into the lua script? Im okay with the rules being ephimeral.
- [x] when remapping a movement key, show lock icons on the illegal placements
- [ ] when remapping keys, right click should cancel

# Camera
- [x] zoom to switch between views + zoom out more (using shift + scroll wheel)

# Terrain
- [x] Setup atmosphere
- [x] I dont touch the floor, jumping also leaves me a little bit off the floor