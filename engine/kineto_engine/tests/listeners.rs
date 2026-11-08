//! Issue #74: a tap drives an input, with no host code in between.

use kineto_engine::format::generated::kineto::{
    finish_document_buffer, Document, DocumentArgs, IkTarget, IkTargetArgs, InputKind,
    ListenerActionKind, ListenerEventKind, SMListener, SMListenerArgs, SMState, SMStateArgs,
    SMTransition, SMTransitionArgs, StateMachine, StateMachineArgs, StateMachineInput,
    StateMachineInputArgs, TransitionCondition, TransitionConditionArgs, TransitionConditionKind,
};
use kineto_engine::player::KinetoPlayer;

struct ListenerSpec {
    subject: &'static str,
    event: ListenerEventKind,
    action: ListenerActionKind,
    bool_value: bool,
    number_value: f32,
}

fn down(subject: &'static str, value: bool) -> ListenerSpec {
    ListenerSpec {
        subject,
        event: ListenerEventKind::PointerDown,
        action: ListenerActionKind::SetBoolean,
        bool_value: value,
        number_value: 0.0,
    }
}

/// One interactive target ("press-area", 200×100 at 200,200), a bool input
/// `pressed`, and Idle ⇄ Pressed driven by it.
fn build_doc(listeners: Vec<ListenerSpec>, interactive: bool) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(2048);

    let target_id = fbb.create_string("press-area");
    let chain_root = fbb.create_string("root");
    let target = IkTarget::create(
        &mut fbb,
        &IkTargetArgs {
            id: Some(target_id),
            chain_root_id: Some(chain_root),
            position_x: 200.0,
            position_y: 200.0,
            extent_w: 200.0,
            extent_h: 100.0,
            interactive,
            ..Default::default()
        },
    );
    let targets = fbb.create_vector(&[target]);

    let input_id = fbb.create_string("input-pressed");
    let input_name = fbb.create_string("pressed");
    let input = StateMachineInput::create(
        &mut fbb,
        &StateMachineInputArgs {
            id: Some(input_id),
            name: Some(input_name),
            kind: InputKind::Bool,
            ..Default::default()
        },
    );
    let inputs = fbb.create_vector(&[input]);

    let mut state_offsets = Vec::new();
    for id in ["idle", "pressed"] {
        let state_id = fbb.create_string(id);
        let state_name = fbb.create_string(id);
        let clip_id = fbb.create_string("");
        state_offsets.push(SMState::create(
            &mut fbb,
            &SMStateArgs {
                id: Some(state_id),
                name: Some(state_name),
                clip_id: Some(clip_id),
                ..Default::default()
            },
        ));
    }
    let states = fbb.create_vector(&state_offsets);

    let condition_input = fbb.create_string("input-pressed");
    let condition = TransitionCondition::create(
        &mut fbb,
        &TransitionConditionArgs {
            kind: TransitionConditionKind::BoolEquals,
            input_id: Some(condition_input),
            bool_expected: true,
            ..Default::default()
        },
    );
    let conditions = fbb.create_vector(&[condition]);
    let transition_id = fbb.create_string("t-down");
    let from = fbb.create_string("idle");
    let to = fbb.create_string("pressed");
    let transition = SMTransition::create(
        &mut fbb,
        &SMTransitionArgs {
            id: Some(transition_id),
            from_state_id: Some(from),
            to_state_id: Some(to),
            conditions: Some(conditions),
            enabled: true,
            // Explicit: the schema's default is 0.15s, and these tests are
            // about who writes the input, not about blend timing.
            duration_seconds: 0.0,
            ..Default::default()
        },
    );
    // The way back, so a test can observe an input returning to false rather
    // than only reaching true. Without it the machine is a one-way door and
    // "did the flip happen" has no visible answer.
    let back_input = fbb.create_string("input-pressed");
    let back_condition = TransitionCondition::create(
        &mut fbb,
        &TransitionConditionArgs {
            kind: TransitionConditionKind::BoolEquals,
            input_id: Some(back_input),
            bool_expected: false,
            ..Default::default()
        },
    );
    let back_conditions = fbb.create_vector(&[back_condition]);
    let back_id = fbb.create_string("t-up");
    let back_from = fbb.create_string("pressed");
    let back_to = fbb.create_string("idle");
    let back_transition = SMTransition::create(
        &mut fbb,
        &SMTransitionArgs {
            id: Some(back_id),
            from_state_id: Some(back_from),
            to_state_id: Some(back_to),
            conditions: Some(back_conditions),
            enabled: true,
            duration_seconds: 0.0,
            ..Default::default()
        },
    );
    let transitions = fbb.create_vector(&[transition, back_transition]);

    let listener_offsets: Vec<_> = listeners
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            let id = fbb.create_string(&format!("listener_{index}"));
            let subject = fbb.create_string(spec.subject);
            let input = fbb.create_string("input-pressed");
            SMListener::create(
                &mut fbb,
                &SMListenerArgs {
                    id: Some(id),
                    subject_id: Some(subject),
                    input_id: Some(input),
                    event: spec.event,
                    action: spec.action,
                    bool_value: spec.bool_value,
                    number_value: spec.number_value,
                    audio_event_id: None,
                },
            )
        })
        .collect();
    let listeners_vec = fbb.create_vector(&listener_offsets);

    let machine_id = fbb.create_string("machine");
    let machine_name = fbb.create_string("Button States");
    let initial = fbb.create_string("idle");
    let machine = StateMachine::create(
        &mut fbb,
        &StateMachineArgs {
            id: Some(machine_id),
            name: Some(machine_name),
            initial_state_id: Some(initial),
            states: Some(states),
            transitions: Some(transitions),
            inputs: Some(inputs),
            listeners: Some(listeners_vec),
            ..Default::default()
        },
    );
    let machines = fbb.create_vector(&[machine]);

    let name = fbb.create_string("Button");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            ik_targets: Some(targets),
            state_machines: Some(machines),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

#[test]
fn a_tap_inside_the_region_drives_the_machine() {
    let bytes = build_doc(vec![down("press-area", true)], true);
    let mut player = KinetoPlayer::load(bytes).expect("player should load");
    player.advance(0.0).expect("advance");

    let fired = player
        .pointer_event(ListenerEventKind::PointerDown, 200.0, 200.0)
        .expect("pointer event should run");
    assert_eq!(fired.len(), 1);

    // One tick to see the input, and the machine is there: a zero-duration
    // transition has nothing to blend.
    player.advance(0.016).expect("advance");
    assert_eq!(player.active_state("machine"), Some("pressed"));
}

#[test]
fn a_tap_outside_the_region_does_nothing() {
    let bytes = build_doc(vec![down("press-area", true)], true);
    let mut player = KinetoPlayer::load(bytes).expect("player should load");
    player.advance(0.0).expect("advance");

    // The region spans 100..300 × 150..250; this is just past its edge.
    let fired = player
        .pointer_event(ListenerEventKind::PointerDown, 301.0, 200.0)
        .expect("pointer event should run");

    assert!(fired.is_empty());
    player.advance(0.016).expect("advance");
    assert_eq!(player.active_state("machine"), Some("idle"));
}

#[test]
fn a_target_that_does_not_catch_touch_is_not_hit() {
    // The extent still means "how far a value travels"; only the flag makes it
    // a region a finger can land in.
    let bytes = build_doc(vec![down("press-area", true)], false);
    let player = KinetoPlayer::load(bytes).expect("player should load");

    assert_eq!(
        player.hit_test_targets(200.0, 200.0).expect("hit test"),
        None
    );
}

#[test]
fn the_release_writes_the_other_value() {
    let bytes = build_doc(
        vec![
            down("press-area", true),
            ListenerSpec {
                subject: "press-area",
                event: ListenerEventKind::PointerUp,
                action: ListenerActionKind::SetBoolean,
                bool_value: false,
                number_value: 0.0,
            },
        ],
        true,
    );
    let mut player = KinetoPlayer::load(bytes).expect("player should load");
    player.advance(0.0).expect("advance");

    player
        .pointer_event(ListenerEventKind::PointerDown, 200.0, 200.0)
        .expect("down");
    player.advance(0.016).expect("advance");
    assert_eq!(player.active_state("machine"), Some("pressed"));

    // Only the listener for this event fires: a press and a release are two
    // different facts about the same finger.
    let fired = player
        .pointer_event(ListenerEventKind::PointerUp, 200.0, 200.0)
        .expect("up");
    assert_eq!(fired.len(), 1);
}

#[test]
fn a_toggle_flips_the_input_on_every_press() {
    // The point of Toggle: one listener, on one event, both opens and closes.
    // Set would need two listeners on two events to say this.
    let bytes = build_doc(
        vec![ListenerSpec {
            subject: "press-area",
            event: ListenerEventKind::PointerDown,
            action: ListenerActionKind::ToggleBoolean,
            bool_value: false,
            number_value: 0.0,
        }],
        true,
    );
    let mut player = KinetoPlayer::load(bytes).expect("player should load");
    player.advance(0.0).expect("advance");

    player
        .pointer_event(ListenerEventKind::PointerDown, 200.0, 200.0)
        .expect("pointer event should run");
    player.advance(0.016).expect("advance");
    assert_eq!(player.active_state("machine"), Some("pressed"));

    // The second press is the half a Set listener cannot reach.
    player
        .pointer_event(ListenerEventKind::PointerDown, 200.0, 200.0)
        .expect("pointer event should run");
    player.advance(0.016).expect("advance");
    assert_eq!(player.active_state("machine"), Some("idle"));
}

#[test]
fn a_listener_on_a_missing_subject_never_fires() {
    let bytes = build_doc(vec![down("ghost", true)], true);
    let mut player = KinetoPlayer::load(bytes).expect("player should load");
    player.advance(0.0).expect("advance");

    let fired = player
        .pointer_event(ListenerEventKind::PointerDown, 200.0, 200.0)
        .expect("pointer event should run");
    assert!(fired.is_empty());
}
