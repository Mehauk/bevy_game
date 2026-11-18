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
- [x] When i edit a component's htmls file, the ui goes blank, I have to go back to the index and press save then it comes back

# Menu reworks
- [ ] command menu
  - [ ] open a bottom sheet similar to skyrim
  - [ ] when you start typing show suggestion to autocompelte as well as a list of items to choose from with the arrow keys 

# Hotkeys
- [x] use bevy_enhanced_input
  - [x] show bound keys on a vitual keyboard overlay (new command)
    - [x] bound keys light up and have a short name/desc (max two lines, anything longer just becomes ...)
    - [x] key board should show other hotkeys when different modifiers are pressed (shift, cntrl, shift + cntrl, ...)
    - [x] keybinds inactive during menus (can remove menu gaurd in grab_cursor)

# Camera
- [ ] zoom to switch between views + zoom out more

# Terrain
- [ ] Setup atmosphere