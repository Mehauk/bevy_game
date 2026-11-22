# New docs
- [ ] similar to lua only hooks, a new doc should be generated called commands

# New commands

## screenshots
goal: Ability to take aerial shots from third person view. with robust implementation details for scalability
- [ ] take screenshot of whatever is in the camera's view
  - [ ] make this an abstraction that can work in any view with any camera
- [ ] angle camera
  - [ ] this allows commands to reposition camera instead of only with mouse

## debugging info
goal: Add debugging info under the fps view that can be toggled on or off via a command.needs to be scalable as new things are added.
- [ ] Create a register of info that will need to be added to if something is added for debugging purposes.
- [ ] register
  - [ ] movement state: standing, walking, running, falling, jumping
  - [ ] movement speed