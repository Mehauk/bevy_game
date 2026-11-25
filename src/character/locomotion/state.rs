use super::super::types::MovementState;

const MOVEMENT_SPEED_EPSILON: f32 = 0.05;
const SWIM_ENTER_DEPTH: f32 = 1.15;
const SWIM_EXIT_DEPTH: f32 = 1.05;
const WADING_SLOWDOWN: f32 = 0.45;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Medium {
    Land,
    Wading,
    Water,
}

pub(crate) fn classify_movement_state(
    is_grounded: bool,
    horiz_speed: f32,
    velocity_y: f32,
    sprinting: bool,
) -> MovementState {
    if !is_grounded {
        if velocity_y > 0.0 {
            MovementState::Jumping
        } else {
            MovementState::Falling
        }
    } else if horiz_speed <= MOVEMENT_SPEED_EPSILON {
        MovementState::Standing
    } else if sprinting {
        MovementState::Running
    } else {
        MovementState::Walking
    }
}

fn swim_latch(depth: f32, was_swimming: bool) -> bool {
    if was_swimming {
        depth > SWIM_EXIT_DEPTH
    } else {
        depth >= SWIM_ENTER_DEPTH
    }
}

pub(crate) fn classify_medium(depth: f32, was_swimming: bool) -> Medium {
    if swim_latch(depth, was_swimming) {
        Medium::Water
    } else if depth > 0.0 {
        Medium::Wading
    } else {
        Medium::Land
    }
}

pub(crate) fn wading_scale(depth: f32) -> f32 {
    let sunk = (depth / SWIM_ENTER_DEPTH).clamp(0.0, 1.0);
    1.0 - WADING_SLOWDOWN * sunk
}

pub(crate) fn classify_wading_state(state: MovementState) -> MovementState {
    match state {
        MovementState::Standing | MovementState::Walking | MovementState::Running => {
            MovementState::Wading
        }
        other => other,
    }
}

pub(crate) fn classify_water_state(diving: bool, ascending: bool) -> MovementState {
    if diving {
        MovementState::Diving
    } else if ascending {
        MovementState::Floating
    } else {
        MovementState::Swimming
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airborne_rises_fall_by_vertical_velocity() {
        assert_eq!(
            classify_movement_state(false, 0.0, 1.0, false),
            MovementState::Jumping
        );
        assert_eq!(
            classify_movement_state(false, 8.0, 0.1, true),
            MovementState::Jumping
        );
        assert_eq!(
            classify_movement_state(false, 8.0, -0.1, false),
            MovementState::Falling
        );
        assert_eq!(
            classify_movement_state(false, 0.0, 0.0, false),
            MovementState::Falling
        );
    }

    #[test]
    fn grounded_walk_down_a_slope_is_not_falling() {
        assert_eq!(
            classify_movement_state(true, 6.0, -2.0, false),
            MovementState::Walking
        );
    }

    #[test]
    fn grounded_speed_and_sprint_split_the_states() {
        assert_eq!(
            classify_movement_state(true, 0.0, 0.0, false),
            MovementState::Standing
        );
        assert_eq!(
            classify_movement_state(true, MOVEMENT_SPEED_EPSILON, 0.0, true),
            MovementState::Standing,
            "at the epsilon it still counts as standing"
        );
        assert_eq!(
            classify_movement_state(true, 6.0, 0.0, false),
            MovementState::Walking
        );
        assert_eq!(
            classify_movement_state(true, 12.0, 0.0, true),
            MovementState::Running
        );
    }

    #[test]
    fn movement_state_displays_lowercase() {
        assert_eq!(MovementState::Jumping.to_string(), "jumping");
        assert_eq!(MovementState::default(), MovementState::Standing);
    }

    #[test]
    fn swim_latch_needs_more_depth_to_leave_than_to_enter() {
        assert!(!swim_latch(SWIM_ENTER_DEPTH - 0.01, false));
        assert!(swim_latch(SWIM_ENTER_DEPTH, false));
        assert!(
            swim_latch(SWIM_EXIT_DEPTH + 0.01, true),
            "draining through the band keeps you swimming"
        );
        assert!(
            !swim_latch(SWIM_EXIT_DEPTH, true),
            "once shallow enough you stand again"
        );
    }

    #[test]
    fn medium_splits_land_wading_and_water() {
        assert_eq!(classify_medium(-0.5, false), Medium::Land);
        assert_eq!(classify_medium(0.0, false), Medium::Land);
        assert_eq!(classify_medium(0.5, false), Medium::Wading);
        assert_eq!(classify_medium(SWIM_ENTER_DEPTH, false), Medium::Water);
        assert_eq!(classify_medium(1.1, false), Medium::Wading);
        assert_eq!(classify_medium(1.1, true), Medium::Water);
    }

    #[test]
    fn wading_slows_the_deeper_you_are() {
        assert_eq!(wading_scale(0.0), 1.0, "dry shoes are unbothered");
        let knee = wading_scale(0.5);
        let chest = wading_scale(1.0);
        assert!(knee > chest, "deeper is slower");
        assert!(chest > 1.0 - WADING_SLOWDOWN);
        assert!(
            (wading_scale(99.0) - (1.0 - WADING_SLOWDOWN)).abs() < 1e-6,
            "the ramp bottoms out instead of going negative"
        );
    }

    #[test]
    fn water_states_split_on_input_and_depth() {
        assert_eq!(classify_water_state(true, true), MovementState::Diving);
        assert_eq!(classify_water_state(false, true), MovementState::Floating);
        assert_eq!(classify_water_state(false, false), MovementState::Swimming);
    }

    #[test]
    fn wading_relabels_only_the_grounded_states() {
        assert_eq!(
            classify_wading_state(MovementState::Standing),
            MovementState::Wading
        );
        assert_eq!(
            classify_wading_state(MovementState::Walking),
            MovementState::Wading
        );
        assert_eq!(
            classify_wading_state(MovementState::Running),
            MovementState::Wading
        );
        assert_eq!(
            classify_wading_state(MovementState::Jumping),
            MovementState::Jumping
        );
        assert_eq!(
            classify_wading_state(MovementState::Falling),
            MovementState::Falling
        );
    }

    #[test]
    fn water_movement_states_display_lowercase() {
        assert_eq!(MovementState::Wading.to_string(), "wading");
        assert_eq!(MovementState::Floating.to_string(), "floating");
        assert_eq!(MovementState::Swimming.to_string(), "swimming");
        assert_eq!(MovementState::Diving.to_string(), "diving");
    }

    #[test]
    fn only_the_fluid_states_count_as_swimming() {
        assert!(MovementState::Floating.is_swimming());
        assert!(MovementState::Swimming.is_swimming());
        assert!(MovementState::Diving.is_swimming());
        assert!(
            !MovementState::Wading.is_swimming(),
            "wading keeps its footing"
        );
        assert!(!MovementState::Walking.is_swimming());
        assert!(!MovementState::Jumping.is_swimming());
    }
}
