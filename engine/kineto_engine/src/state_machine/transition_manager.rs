use crate::animation::EasingPreset;
use crate::format;

#[derive(Debug, Clone, PartialEq)]
pub struct SmEvalState {
    pub active_state_id: Option<String>,
    pub active_transition_id: Option<String>,
    pub transition_elapsed_seconds: f64,
    pub source_state_elapsed_seconds: f64,
    pub destination_state_elapsed_seconds: f64,
}

impl Default for SmEvalState {
    fn default() -> Self {
        Self {
            active_state_id: None,
            active_transition_id: None,
            transition_elapsed_seconds: 0.0,
            source_state_elapsed_seconds: 0.0,
            destination_state_elapsed_seconds: 0.0,
        }
    }
}

pub struct TransitionManager;

impl TransitionManager {
    pub fn progress(state: &SmEvalState, transition: format::SMTransition<'_>) -> f64 {
        let duration = transition.duration_seconds() as f64;
        if duration <= 0.0 {
            return 1.0;
        }
        (state.transition_elapsed_seconds / duration).clamp(0.0, 1.0)
    }

    pub fn eased_progress(state: &SmEvalState, transition: format::SMTransition<'_>) -> f64 {
        EasingPreset::from_kvk_ordinal(transition.easing()).apply(Self::progress(state, transition))
    }

    pub fn advance(
        state: &SmEvalState,
        transition: format::SMTransition<'_>,
        input_values: &super::evaluator::SmInputValues,
        dt: f64,
    ) -> (SmEvalState, super::evaluator::SmInputValues) {
        let elapsed = state.transition_elapsed_seconds + dt;
        let destination_elapsed = state.destination_state_elapsed_seconds + dt;
        let source_elapsed = state.source_state_elapsed_seconds + dt;

        if elapsed >= transition.duration_seconds() as f64 {
            return (
                SmEvalState {
                    active_state_id: Some(transition.to_state_id().to_owned()),
                    source_state_elapsed_seconds: destination_elapsed,
                    ..Default::default()
                },
                input_values.clear_triggers(),
            );
        }

        (
            SmEvalState {
                active_state_id: Some(transition.from_state_id().to_owned()),
                active_transition_id: Some(transition.id().to_owned()),
                transition_elapsed_seconds: elapsed,
                source_state_elapsed_seconds: source_elapsed,
                destination_state_elapsed_seconds: destination_elapsed,
            },
            input_values.clear_triggers(),
        )
    }
}
