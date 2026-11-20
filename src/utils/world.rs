//! Shared world facts that more than one feature needs but no single feature owns.
//!
//! These are deliberately tiny, dependency-free resources: a producer feature writes them and
//! a consumer feature reads them, so neither has to import the other (see the integration rule
//! in `refactor.md`).

use bevy::prelude::*;

/// Surface height at the world origin, kept current by `terrain`.
///
/// The player spawns and respawns at the origin, so `character` needs this one number but must
/// not depend on the whole `terrain` heightfield. Defaults to `0.0` so `character` still works
/// if `terrain` is not present.
#[derive(Resource)]
pub struct SpawnGroundHeight(pub f32);

impl Default for SpawnGroundHeight {
    fn default() -> Self {
        Self(0.0)
    }
}
