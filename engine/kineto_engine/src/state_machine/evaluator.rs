use std::collections::{HashMap, HashSet};

use crate::format;

use super::transition_manager::{SmEvalState, TransitionManager};

/// `f64` that carries `SmInputValues`'s `Eq`. NaN never reaches here — the
/// setters are the only way in and callers pass real values — so bitwise
/// equality is the honest comparison for "did the inputs change".
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OrderedF64(pub f64);

impl Eq for OrderedF64 {}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SmInputValues {
    pub bool_values: HashMap<String, bool>,
    pub pending_trigger_ids: HashSet<String>,
    /// Normalised `0..1` values per number input (issue #74). Absent means
    /// "not driven yet", which is not the same as driven to zero: a binding on
    /// an input nobody has set must leave its property alone.
    pub number_values: HashMap<String, OrderedF64>,
}

impl SmInputValues {
    pub fn bool_value(&self, input_id: &str, default_value: bool) -> bool {
        self.bool_values
            .get(input_id)
            .copied()
            .unwrap_or(default_value)
    }

    pub fn has_trigger(&self, input_id: &str) -> bool {
        self.pending_trigger_ids.contains(input_id)
    }

    pub fn number_value(&self, input_id: &str) -> Option<f64> {
        self.number_values.get(input_id).map(|value| value.0)
    }

    pub fn set_number(mut self, input_id: impl Into<String>, value: f64) -> Self {
        self.number_values
            .insert(input_id.into(), OrderedF64(value));
        self
    }

    pub fn set_bool(mut self, input_id: impl Into<String>, value: bool) -> Self {
        self.bool_values.insert(input_id.into(), value);
        self
    }

    pub fn queue_trigger(mut self, input_id: impl Into<String>) -> Self {
        self.pending_trigger_ids.insert(input_id.into());
        self
    }

    pub fn clear_triggers(&self) -> Self {
        if self.pending_trigger_ids.is_empty() {
            return self.clone();
        }
        Self {
            bool_values: self.bool_values.clone(),
            pending_trigger_ids: HashSet::new(),
            number_values: self.number_values.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmAdvanceResult {
    pub state: SmEvalState,
    pub input_values: SmInputValues,
    pub consumed_trigger_ids: HashSet<String>,
    /// Audio events this tick started, in transition order (issue #111).
    ///
    /// Reported, never played: the engine has no audio device and a `.kvk`
    /// carries no sound files. The host is told what fired and plays whatever
    /// it ships for that event.
    ///
    /// Non-empty only on the tick a transition *starts*, so a blend that spans
    /// twenty frames reports its sound once.
    pub fired_audio_event_ids: Vec<String>,
}

/// One layer's graph, addressed uniformly whether it is the base layer or one
/// stacked above it (issue #98).
///
/// The base layer lives in the machine's own `states`/`transitions`; every
/// other layer has an `SMLayer` of its own. Both sides keep reading the
/// machine for inputs, which stay machine-wide so a host sets a value once and
/// every layer sees it.
#[derive(Clone, Copy)]
pub struct LayerGraph<'a> {
    machine: format::StateMachine<'a>,
    layer: Option<format::SMLayer<'a>>,
}

impl<'a> LayerGraph<'a> {
    pub fn base(machine: format::StateMachine<'a>) -> Self {
        Self {
            machine,
            layer: None,
        }
    }

    pub fn of(machine: format::StateMachine<'a>, layer: format::SMLayer<'a>) -> Self {
        Self {
            machine,
            layer: Some(layer),
        }
    }

    /// Stable per-layer key. The base layer has no stored id, so it derives
    /// one from the machine the same way the editor does (`<machine>::base`),
    /// which is what lets both sides talk about the same layer.
    pub fn id(&self) -> String {
        match self.layer {
            Some(layer) => layer.id().to_owned(),
            None => format!("{}::base", self.machine.id()),
        }
    }

    pub fn name(&self) -> &'a str {
        match self.layer {
            Some(layer) => layer.name(),
            None => self.machine.base_layer_name().unwrap_or("Base"),
        }
    }

    /// A disabled layer is skipped entirely: it neither advances nor
    /// contributes a pose.
    pub fn enabled(&self) -> bool {
        match self.layer {
            Some(layer) => layer.enabled(),
            None => self.machine.base_layer_enabled(),
        }
    }

    pub fn is_base(&self) -> bool {
        self.layer.is_none()
    }

    fn states(
        &self,
    ) -> ::flatbuffers::Vector<'a, ::flatbuffers::ForwardsUOffset<format::SMState<'a>>> {
        match self.layer {
            Some(layer) => layer.states().unwrap_or_default(),
            None => self.machine.states().unwrap_or_default(),
        }
    }

    fn transitions(
        &self,
    ) -> ::flatbuffers::Vector<'a, ::flatbuffers::ForwardsUOffset<format::SMTransition<'a>>> {
        match self.layer {
            Some(layer) => layer.transitions().unwrap_or_default(),
            None => self.machine.transitions().unwrap_or_default(),
        }
    }

    /// The state this layer starts in, resolving a dangling or absent id to
    /// the first state — the same rule the editor's `initialState` uses.
    pub fn initial_state_id_resolved(&self) -> Option<String> {
        let states = self.states();
        if states.is_empty() {
            return None;
        }
        if let Some(id) = self.initial_state_id() {
            if states.iter().any(|state| state.id() == id) {
                return Some(id.to_owned());
            }
        }
        Some(states.get(0).id().to_owned())
    }

    fn initial_state_id(&self) -> Option<&'a str> {
        match self.layer {
            Some(layer) => layer.initial_state_id(),
            None => self.machine.initial_state_id(),
        }
    }

    /// The clip the layer is showing right now, and how far into that state it
    /// is — the per-layer clock. `None` when the layer has no active state or
    /// that state names no clip.
    pub fn active_clip(&self, state: &SmEvalState) -> Option<(&'a str, f64)> {
        let active_id = state.active_state_id.as_deref()?;
        let sm_state = self.states().iter().find(|s| s.id() == active_id)?;
        Some((sm_state.clip_id(), state.source_state_elapsed_seconds))
    }
}

/// Every layer of `machine`, bottom-first: the base layer, then the layers
/// stacked on it in order. Order is priority — a later layer wins on the
/// properties it animates.
pub fn layers_of<'a>(machine: format::StateMachine<'a>) -> Vec<LayerGraph<'a>> {
    let mut layers = vec![LayerGraph::base(machine)];
    for layer in machine.layers().unwrap_or_default() {
        layers.push(LayerGraph::of(machine, layer));
    }
    layers
}

pub struct SmEvaluator;

impl SmEvaluator {
    /// Advances the machine's base layer. Kept as the shape every pre-layers
    /// caller uses; [`SmEvaluator::advance_layer`] is the general form.
    pub fn advance(
        machine: format::StateMachine<'_>,
        state: &SmEvalState,
        input_values: &SmInputValues,
        delta_seconds: f64,
    ) -> SmAdvanceResult {
        Self::advance_layer(
            LayerGraph::base(machine),
            state,
            input_values,
            delta_seconds,
        )
    }

    pub fn advance_layer(
        graph: LayerGraph<'_>,
        state: &SmEvalState,
        input_values: &SmInputValues,
        delta_seconds: f64,
    ) -> SmAdvanceResult {
        let machine = graph;
        let states = graph.states();
        if states.is_empty() {
            return SmAdvanceResult {
                state: SmEvalState::default(),
                input_values: input_values.clear_triggers(),
                consumed_trigger_ids: HashSet::new(),
                fired_audio_event_ids: Vec::new(),
            };
        }

        if let Some(active_transition_id) = state.active_transition_id.as_deref() {
            if let Some(transition) = find_transition(machine, active_transition_id) {
                let (next_state, next_inputs) =
                    TransitionManager::advance(state, transition, input_values, delta_seconds);
                return SmAdvanceResult {
                    state: next_state,
                    input_values: next_inputs,
                    consumed_trigger_ids: HashSet::new(),
                    fired_audio_event_ids: Vec::new(),
                };
            }
        }

        let active_id = resolve_active_id(machine, state.active_state_id.as_deref());
        let elapsed = state.source_state_elapsed_seconds + delta_seconds;

        let matched = first_match(machine, &active_id, input_values, elapsed);
        let Some(transition) = matched else {
            return SmAdvanceResult {
                state: SmEvalState {
                    active_state_id: Some(active_id),
                    source_state_elapsed_seconds: elapsed,
                    ..Default::default()
                },
                input_values: input_values.clear_triggers(),
                consumed_trigger_ids: HashSet::new(),
                fired_audio_event_ids: Vec::new(),
            };
        };

        let consumed = collect_triggers(transition);
        let cleared = input_values.clear_triggers();
        let fired_audio = collect_audio_events(transition);

        if transition.duration_seconds() <= 0.0 {
            return SmAdvanceResult {
                state: SmEvalState {
                    active_state_id: Some(transition.to_state_id().to_owned()),
                    ..Default::default()
                },
                input_values: cleared,
                consumed_trigger_ids: consumed,
                fired_audio_event_ids: fired_audio,
            };
        }

        SmAdvanceResult {
            state: SmEvalState {
                active_state_id: Some(active_id),
                active_transition_id: Some(transition.id().to_owned()),
                transition_elapsed_seconds: 0.0,
                source_state_elapsed_seconds: elapsed,
                destination_state_elapsed_seconds: 0.0,
            },
            input_values: cleared,
            consumed_trigger_ids: consumed,
            fired_audio_event_ids: fired_audio,
        }
    }
}

/// The sounds a transition fires when it starts (issue #111).
fn collect_audio_events(transition: format::SMTransition<'_>) -> Vec<String> {
    transition
        .audio_event_ids()
        .map(|ids| ids.iter().map(|id| id.to_owned()).collect())
        .unwrap_or_default()
}

fn resolve_active_id(graph: LayerGraph<'_>, id: Option<&str>) -> String {
    let states = graph.states();
    if let Some(id) = id {
        if states.iter().any(|state| state.id() == id) {
            return id.to_owned();
        }
    }
    if let Some(initial_id) = graph.initial_state_id() {
        if states.iter().any(|state| state.id() == initial_id) {
            return initial_id.to_owned();
        }
    }
    states.get(0).id().to_owned()
}

fn find_transition<'a>(graph: LayerGraph<'a>, id: &str) -> Option<format::SMTransition<'a>> {
    graph
        .transitions()
        .iter()
        .find(|transition| transition.id() == id)
}

fn first_match<'a>(
    graph: LayerGraph<'a>,
    from_state_id: &str,
    input_values: &SmInputValues,
    source_elapsed: f64,
) -> Option<format::SMTransition<'a>> {
    graph.transitions().iter().find(|transition| {
        transition.from_state_id() == from_state_id
            && matches_transition(graph, *transition, input_values, source_elapsed)
    })
}

fn matches_transition(
    graph: LayerGraph<'_>,
    transition: format::SMTransition<'_>,
    input_values: &SmInputValues,
    source_elapsed: f64,
) -> bool {
    // `enabled` is deliberately NOT consulted, matching
    // `state_machine_evaluator.dart:219-225`. The Enabled toggle was removed
    // from the UI in issue #42, so nothing renders the flag — but documents
    // saved while it existed still carry `enabled: false`. Honouring it here
    // while the editor ignores it made such a transition work in the editor
    // and stay invisibly dead on device, with every surface looking correct
    // (the iPad "State 2 stays active" repro).
    let conditions = transition.conditions().unwrap_or_default();
    if conditions.is_empty() {
        return false;
    }

    conditions
        .iter()
        .all(|condition| condition_passes(graph, condition, input_values, source_elapsed))
}

fn condition_passes(
    graph: LayerGraph<'_>,
    condition: format::TransitionCondition<'_>,
    input_values: &SmInputValues,
    source_elapsed: f64,
) -> bool {
    match condition.kind() {
        format::TransitionConditionKind::BoolEquals => {
            let Some(input_id) = condition.input_id() else {
                return false;
            };
            let Some(input) = find_input(graph.machine, input_id) else {
                return false;
            };
            if input.kind() != format::InputKind::Bool {
                return false;
            }
            let actual = input_values.bool_value(input_id, input.default_bool_value());
            actual == condition.bool_expected()
        }
        format::TransitionConditionKind::TriggerFired => {
            let Some(input_id) = condition.input_id() else {
                return false;
            };
            input_values.has_trigger(input_id)
        }
        format::TransitionConditionKind::HasExitTime => {
            source_elapsed >= condition.exit_time_seconds() as f64
        }
        _ => false,
    }
}

fn find_input<'a>(
    machine: format::StateMachine<'a>,
    id: &str,
) -> Option<format::StateMachineInput<'a>> {
    machine
        .inputs()
        .unwrap_or_default()
        .iter()
        .find(|input| input.id() == id)
}

fn collect_triggers(transition: format::SMTransition<'_>) -> HashSet<String> {
    transition
        .conditions()
        .unwrap_or_default()
        .iter()
        .filter_map(|condition| {
            if condition.kind() == format::TransitionConditionKind::TriggerFired {
                condition.input_id().map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}
