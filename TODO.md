- [ ] checkout runevision (https://github.com/runevision)

# Menus
- [x] setup html+lua menus
- [-] setup commands menu
  - [-] fix bug with flickering
  - [ ] arrow keys (up/down) still not working
    - [ ] they sorta work, I can briefly see one highlighted but then the highlight disapears and teh menu flickers back to no selection state. The up/down arrow keys are still being used in the text input
  - [x] mouse should stay captured if it was already
  - [x] dont allow mouse here (not for select or text editing)
  - [ ] highlight letters (handling proper order)
  - [x] keep text input box centered (no move) (should help with the flickering)
- [ ] When i edit a component, the ui goes blank, I hve to go back to the index and press save then it comes back

# Hotkeys
- [ ] use bevy_enhanced_input
  - [ ] show bound keys on a vitual keyboard overlay (new command)
    - [ ] bound keys light up and have a short name/desc (max two lines, anything longeer just becomes ... (make sure not to resize the keys))
    - [ ] key board should show other hotkeys when different modifiers are pressed (shift, cntrl, shift + cntrl, ...)
    - [ ] keybinds inactive during menus (can remove menu gaurd in grab_cursor)

# Camera
- [ ] zoom to switch between views + zoom out more

# Terrain
- [ ] Setup atmosphere