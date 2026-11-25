# CLAUDE.md

Guidance for agents (and humans) working in this repository.

## Documentation and comments

**No comments in code.** Do not add `//`, `///`, `//!`, or block comments to source files. Prefer
self-explanatory code: precise names, small functions, obvious structure. If a piece of logic
needs prose to be understood, restructure it rather than comment it.

- This governs new and modified code. Existing comments are removed opportunistically when a file
  is already being edited; do not run dedicated mass-comment-strips unless explicitly asked.

**Markdown is written by humans, not agents.** Do not create or edit `.md` files unless the user
explicitly asks for it in the current request — this covers `docs/`, READMEs, guides, and
changelogs.

## Layout

- Feature modules (`input`, `menus`, `camera`, `terrain`, `state`, `character`) never import one
  another. Cross-feature wiring lives in `src/integration/`, wired by a single `IntegrationPlugin`.
- `src/utils/` is a strict kernel: no feature imports and no plugins live there.
