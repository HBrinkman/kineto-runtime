use kineto_engine::format::generated::kineto::{
    finish_document_buffer, root_as_document, Document, DocumentArgs, InputKind, SMState,
    SMStateArgs, SMTransition, SMTransitionArgs, StateMachine, StateMachineArgs, StateMachineInput,
    StateMachineInputArgs, TransitionCondition, TransitionConditionArgs, TransitionConditionKind,
};
use kineto_engine::state_machine::{SmEvalState, SmEvaluator, SmInputValues, TransitionManager};

fn assert_close(actual: f64, expected: f64, epsilon: f64) {
    assert!(
        (actual - expected).abs() <= epsilon,
        "expected {actual} to be within {epsilon} of {expected}"
    );
}

#[test]
fn transition_manager_tracks_progress_and_easing() {
    let bytes = build_doc(
        vec![state("a", "clip_a"), state("b", "clip_b")],
        vec![transition("t", "a", "b", vec![], 1.0, 4, true)],
        vec![],
        Some("a"),
    );
    let machine = root_as_document(&bytes)
        .expect("document should parse")
        .state_machines()
        .unwrap()
        .get(0);
    let transition = machine.transitions().unwrap().get(0);
    let eval_state = SmEvalState {
        transition_elapsed_seconds: 0.5,
        ..Default::default()
    };

    assert_close(
        TransitionManager::progress(&eval_state, transition),
        0.5,
        0.001,
    );
    assert_close(
        TransitionManager::eased_progress(&eval_state, transition),
        0.5,
        0.05,
    );
}

#[test]
fn uses_initial_state_when_no_active_state() {
    let bytes = build_default_doc(vec![], vec![]);
    let machine = first_machine(&bytes);

    let result = SmEvaluator::advance(
        machine,
        &SmEvalState::default(),
        &SmInputValues::default(),
        0.1,
    );

    assert_eq!(result.state.active_state_id.as_deref(), Some("idle"));
}

#[test]
fn transition_with_no_conditions_does_not_fire() {
    let bytes = build_default_doc(
        vec![transition("t", "idle", "walk", vec![], 0.0, 0, true)],
        vec![],
    );
    let machine = first_machine(&bytes);

    let result = SmEvaluator::advance(
        machine,
        &SmEvalState {
            active_state_id: Some("idle".to_owned()),
            ..Default::default()
        },
        &SmInputValues::default(),
        0.1,
    );

    assert_eq!(result.state.active_state_id.as_deref(), Some("idle"));
}

#[test]
fn bool_transition_fires_when_condition_matches() {
    let bytes = build_default_doc(
        vec![transition(
            "t",
            "idle",
            "walk",
            vec![bool_condition("input1", true)],
            0.0,
            0,
            true,
        )],
        vec![input("input1", "isWalking", InputKind::Bool, false)],
    );
    let machine = first_machine(&bytes);
    let inputs = SmInputValues::default().set_bool("input1", true);

    let result = SmEvaluator::advance(
        machine,
        &SmEvalState {
            active_state_id: Some("idle".to_owned()),
            ..Default::default()
        },
        &inputs,
        0.1,
    );

    assert_eq!(result.state.active_state_id.as_deref(), Some("walk"));
}

#[test]
fn trigger_fires_once_and_clears() {
    let bytes = build_default_doc(
        vec![transition(
            "t",
            "idle",
            "walk",
            vec![trigger_condition("trig1")],
            0.0,
            0,
            true,
        )],
        vec![input("trig1", "jump", InputKind::Trigger, false)],
    );
    let machine = first_machine(&bytes);
    let inputs = SmInputValues::default().queue_trigger("trig1");

    let result = SmEvaluator::advance(
        machine,
        &SmEvalState {
            active_state_id: Some("idle".to_owned()),
            ..Default::default()
        },
        &inputs,
        0.1,
    );

    assert_eq!(result.state.active_state_id.as_deref(), Some("walk"));
    assert!(result.input_values.pending_trigger_ids.is_empty());
    assert!(result.consumed_trigger_ids.contains("trig1"));
}

#[test]
fn unconsumed_trigger_clears_after_one_frame() {
    let bytes = build_default_doc(vec![], vec![]);
    let machine = first_machine(&bytes);
    let inputs = SmInputValues::default().queue_trigger("noop");

    let result = SmEvaluator::advance(
        machine,
        &SmEvalState {
            active_state_id: Some("idle".to_owned()),
            ..Default::default()
        },
        &inputs,
        0.1,
    );

    assert!(result.input_values.pending_trigger_ids.is_empty());
}

#[test]
fn has_exit_time_gates_until_threshold() {
    let bytes = build_default_doc(
        vec![transition(
            "t",
            "idle",
            "walk",
            vec![exit_time_condition(0.5)],
            0.0,
            0,
            true,
        )],
        vec![],
    );
    let machine = first_machine(&bytes);

    let before = SmEvaluator::advance(
        machine,
        &SmEvalState {
            active_state_id: Some("idle".to_owned()),
            ..Default::default()
        },
        &SmInputValues::default(),
        0.49,
    );
    let after = SmEvaluator::advance(machine, &before.state, &before.input_values, 0.01);

    assert_eq!(before.state.active_state_id.as_deref(), Some("idle"));
    assert_eq!(after.state.active_state_id.as_deref(), Some("walk"));
}

#[test]
fn cross_fade_transition_completes_after_duration() {
    let bytes = build_default_doc(
        vec![transition(
            "t",
            "idle",
            "walk",
            vec![bool_condition("input1", true)],
            0.5,
            0,
            true,
        )],
        vec![input("input1", "isWalking", InputKind::Bool, false)],
    );
    let machine = first_machine(&bytes);
    let inputs = SmInputValues::default().set_bool("input1", true);

    let started = SmEvaluator::advance(
        machine,
        &SmEvalState {
            active_state_id: Some("idle".to_owned()),
            ..Default::default()
        },
        &inputs,
        0.1,
    );
    assert_eq!(started.state.active_transition_id.as_deref(), Some("t"));

    let half = SmEvaluator::advance(machine, &started.state, &started.input_values, 0.25);
    assert_eq!(half.state.active_transition_id.as_deref(), Some("t"));
    assert_close(half.state.transition_elapsed_seconds, 0.25, 1e-9);

    let completed = SmEvaluator::advance(machine, &half.state, &half.input_values, 0.25);
    assert_eq!(completed.state.active_state_id.as_deref(), Some("walk"));
    assert!(completed.state.active_transition_id.is_none());
}

#[test]
fn no_states_returns_inactive_result() {
    let bytes = build_doc(vec![], vec![], vec![], None);
    let machine = first_machine(&bytes);

    let result = SmEvaluator::advance(
        machine,
        &SmEvalState::default(),
        &SmInputValues::default(),
        0.1,
    );

    assert!(result.state.active_state_id.is_none());
}

fn first_machine(bytes: &[u8]) -> kineto_engine::format::StateMachine<'_> {
    root_as_document(bytes)
        .expect("document should parse")
        .state_machines()
        .expect("state machines missing")
        .get(0)
}

fn build_default_doc(transitions: Vec<TransitionSpec>, inputs: Vec<InputSpec>) -> Vec<u8> {
    build_doc(
        vec![state("idle", "clip_idle"), state("walk", "clip_walk")],
        transitions,
        inputs,
        Some("idle"),
    )
}

#[derive(Clone)]
struct StateSpec {
    id: &'static str,
    clip_id: &'static str,
}

fn state(id: &'static str, clip_id: &'static str) -> StateSpec {
    StateSpec { id, clip_id }
}

#[derive(Clone)]
struct InputSpec {
    id: &'static str,
    name: &'static str,
    kind: InputKind,
    default_bool_value: bool,
}

fn input(
    id: &'static str,
    name: &'static str,
    kind: InputKind,
    default_bool_value: bool,
) -> InputSpec {
    InputSpec {
        id,
        name,
        kind,
        default_bool_value,
    }
}

#[derive(Clone)]
struct TransitionSpec {
    id: &'static str,
    from: &'static str,
    to: &'static str,
    conditions: Vec<ConditionSpec>,
    duration: f32,
    easing: u8,
    enabled: bool,
}

fn transition(
    id: &'static str,
    from: &'static str,
    to: &'static str,
    conditions: Vec<ConditionSpec>,
    duration: f32,
    easing: u8,
    enabled: bool,
) -> TransitionSpec {
    TransitionSpec {
        id,
        from,
        to,
        conditions,
        duration,
        easing,
        enabled,
    }
}

#[derive(Clone)]
enum ConditionSpec {
    Bool {
        input_id: &'static str,
        expected: bool,
    },
    Trigger {
        input_id: &'static str,
    },
    ExitTime {
        seconds: f32,
    },
}

fn bool_condition(input_id: &'static str, expected: bool) -> ConditionSpec {
    ConditionSpec::Bool { input_id, expected }
}

fn trigger_condition(input_id: &'static str) -> ConditionSpec {
    ConditionSpec::Trigger { input_id }
}

fn exit_time_condition(seconds: f32) -> ConditionSpec {
    ConditionSpec::ExitTime { seconds }
}

fn build_doc(
    states: Vec<StateSpec>,
    transitions: Vec<TransitionSpec>,
    inputs: Vec<InputSpec>,
    initial_state_id: Option<&'static str>,
) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(2048);

    let state_offsets: Vec<_> = states
        .iter()
        .map(|spec| {
            let id = fbb.create_string(spec.id);
            let name = fbb.create_string(spec.id);
            let clip_id = fbb.create_string(spec.clip_id);
            SMState::create(
                &mut fbb,
                &SMStateArgs {
                    id: Some(id),
                    name: Some(name),
                    clip_id: Some(clip_id),
                    ..Default::default()
                },
            )
        })
        .collect();
    let states_vec = fbb.create_vector(&state_offsets);

    let input_offsets: Vec<_> = inputs
        .iter()
        .map(|spec| {
            let id = fbb.create_string(spec.id);
            let name = fbb.create_string(spec.name);
            StateMachineInput::create(
                &mut fbb,
                &StateMachineInputArgs {
                    id: Some(id),
                    name: Some(name),
                    kind: spec.kind,
                    default_bool_value: spec.default_bool_value,
                    ..Default::default()
                },
            )
        })
        .collect();
    let inputs_vec = fbb.create_vector(&input_offsets);

    let transition_offsets: Vec<_> = transitions
        .iter()
        .map(|spec| {
            let condition_offsets: Vec<_> = spec
                .conditions
                .iter()
                .map(|condition| match condition {
                    ConditionSpec::Bool { input_id, expected } => {
                        let input_id = fbb.create_string(input_id);
                        TransitionCondition::create(
                            &mut fbb,
                            &TransitionConditionArgs {
                                kind: TransitionConditionKind::BoolEquals,
                                input_id: Some(input_id),
                                bool_expected: *expected,
                                ..Default::default()
                            },
                        )
                    }
                    ConditionSpec::Trigger { input_id } => {
                        let input_id = fbb.create_string(input_id);
                        TransitionCondition::create(
                            &mut fbb,
                            &TransitionConditionArgs {
                                kind: TransitionConditionKind::TriggerFired,
                                input_id: Some(input_id),
                                ..Default::default()
                            },
                        )
                    }
                    ConditionSpec::ExitTime { seconds } => TransitionCondition::create(
                        &mut fbb,
                        &TransitionConditionArgs {
                            kind: TransitionConditionKind::HasExitTime,
                            exit_time_seconds: *seconds,
                            ..Default::default()
                        },
                    ),
                })
                .collect();
            let conditions_vec = fbb.create_vector(&condition_offsets);
            let id = fbb.create_string(spec.id);
            let from = fbb.create_string(spec.from);
            let to = fbb.create_string(spec.to);
            SMTransition::create(
                &mut fbb,
                &SMTransitionArgs {
                    id: Some(id),
                    from_state_id: Some(from),
                    to_state_id: Some(to),
                    conditions: Some(conditions_vec),
                    duration_seconds: spec.duration,
                    easing: spec.easing,
                    enabled: spec.enabled,
                    audio_event_ids: None,
                },
            )
        })
        .collect();
    let transitions_vec = fbb.create_vector(&transition_offsets);

    let sm_id = fbb.create_string("sm1");
    let sm_name = fbb.create_string("Character");
    let initial = initial_state_id.map(|id| fbb.create_string(id));
    let machine = StateMachine::create(
        &mut fbb,
        &StateMachineArgs {
            id: Some(sm_id),
            name: Some(sm_name),
            initial_state_id: initial,
            states: Some(states_vec),
            transitions: Some(transitions_vec),
            inputs: Some(inputs_vec),
            ..Default::default()
        },
    );
    let machines = fbb.create_vector(&[machine]);

    let doc_name = fbb.create_string("State Machine Fixture");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(doc_name),
            state_machines: Some(machines),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}
