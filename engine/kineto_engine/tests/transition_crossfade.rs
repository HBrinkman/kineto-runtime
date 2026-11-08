//! Transitions crossfade rather than cutting (issue #111 parity audit §5.1).
//!
//! `eased_progress` existed from the day the transition manager was written
//! and its only caller was a test: the runtime kept the *source* state active
//! for the whole duration and switched at the end, so every transition was a
//! hard cut on device while the editor's preview faded.

use kineto_engine::format::generated::kineto::{
    finish_document_buffer, AnimatedPropertyKind, Animation, AnimationArgs, Document, DocumentArgs,
    InputKind, KFKeyframe, KFKeyframeArgs, KFTrack, KFTrackArgs, Node, NodeArgs, Rectangle,
    Component, Geometry, LoopMode, RectangleArgs, SMState, SMStateArgs, SMTransition, SMTransitionArgs,
    Shape, ShapeArgs, StateMachine, StateMachineArgs, StateMachineInput, StateMachineInputArgs,
    Transform, TransformArgs, TransitionCondition, TransitionConditionArgs,
    TransitionConditionKind,
};
use kineto_engine::player::KinetoPlayer;

fn position_x(frame: &kineto_engine::player::EvaluatedFrame) -> f64 {
    frame
        .node_overrides
        .get("box")
        .and_then(|values| values.scalar("positionX"))
        .expect("the clip animates positionX")
}

#[test]
fn a_running_transition_blends_the_two_clips() {
    let mut player = KinetoPlayer::load(build_doc()).expect("document should load");
    player.play().expect("frame");
    player.set_trigger("machine", "go").expect("trigger");

    // The advance that fires the transition starts it at zero elapsed, so the
    // clock only runs from the next one.
    player.advance(0.0).expect("frame");
    assert_eq!(player.active_transition("machine"), Some("t"));

    // Half a second into a one-second linear transition: the source clip holds
    // 0 and the destination holds 100, so the pose is the midpoint. A hard cut
    // would still read 0 here and jump to 100 at the end.
    let frame = player.advance(0.5).expect("frame");
    let mid = position_x(&frame);
    assert!(
        (mid - 50.0).abs() < 1e-6,
        "expected the midpoint of the crossfade, got {mid}"
    );

    // And it lands cleanly on the destination.
    let frame = player.advance(0.6).expect("frame");
    let end = position_x(&frame);
    assert!((end - 100.0).abs() < 1e-6, "expected 100, got {end}");
}

/// `enabled` is deliberately ignored, matching
/// `state_machine_evaluator.dart:219-225`. The toggle was removed from the UI
/// in issue #42, so a document saved while it existed carries `enabled: false`
/// on a transition every editor surface still shows as live — and the runtime
/// used to refuse to fire it.
#[test]
fn a_legacy_disabled_transition_still_fires() {
    let mut player = KinetoPlayer::load(build_doc_with(false, 0.0)).expect("document should load");
    player.play().expect("frame");
    player.set_trigger("machine", "go").expect("trigger");
    player.advance(0.1).expect("frame");

    assert_eq!(
        player.active_state("machine"),
        Some("to"),
        "a transition the editor would fire must fire here too"
    );
}

fn build_doc() -> Vec<u8> {
    build_doc_with(true, 1.0)
}

/// The same machine driven by a boolean rather than a trigger.
fn build_bool_doc() -> Vec<u8> {
    build_doc_with_condition(true, 0.0, TransitionConditionKind::BoolEquals)
}

fn build_doc_with(enabled: bool, duration: f32) -> Vec<u8> {
    build_doc_with_condition(enabled, duration, TransitionConditionKind::TriggerFired)
}

fn build_doc_with_condition(
    enabled: bool,
    duration: f32,
    condition_kind: TransitionConditionKind,
) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(8192);

    let clip_from = constant_clip(&mut fbb, "clip-from", 0.0);
    let clip_to = constant_clip(&mut fbb, "clip-to", 100.0);
    let animations = fbb.create_vector(&[clip_from, clip_to]);

    let node = box_node(&mut fbb);
    let nodes = fbb.create_vector(&[node]);

    let from = sm_state(&mut fbb, "from", "clip-from");
    let to = sm_state(&mut fbb, "to", "clip-to");
    let states = fbb.create_vector(&[from, to]);

    let input_id = fbb.create_string("go");
    let input_name = fbb.create_string("go");
    let input = StateMachineInput::create(
        &mut fbb,
        &StateMachineInputArgs {
            id: Some(input_id),
            name: Some(input_name),
            kind: if condition_kind == TransitionConditionKind::BoolEquals {
                InputKind::Bool
            } else {
                InputKind::Trigger
            },
            ..Default::default()
        },
    );
    let inputs = fbb.create_vector(&[input]);

    let condition_input = fbb.create_string("go");
    let condition = TransitionCondition::create(
        &mut fbb,
        &TransitionConditionArgs {
            kind: condition_kind,
            input_id: Some(condition_input),
            // The value a BoolEquals condition waits for; ignored otherwise.
            bool_expected: true,
            ..Default::default()
        },
    );
    let conditions = fbb.create_vector(&[condition]);
    let transition_id = fbb.create_string("t");
    let transition_from = fbb.create_string("from");
    let transition_to = fbb.create_string("to");
    let transition = SMTransition::create(
        &mut fbb,
        &SMTransitionArgs {
            id: Some(transition_id),
            from_state_id: Some(transition_from),
            to_state_id: Some(transition_to),
            conditions: Some(conditions),
            duration_seconds: duration,
            easing: 0,
            enabled,
            ..Default::default()
        },
    );
    let transitions = fbb.create_vector(&[transition]);

    let machine_id = fbb.create_string("machine");
    let machine_name = fbb.create_string("Machine");
    let initial = fbb.create_string("from");
    let machine = StateMachine::create(
        &mut fbb,
        &StateMachineArgs {
            id: Some(machine_id),
            name: Some(machine_name),
            initial_state_id: Some(initial),
            states: Some(states),
            transitions: Some(transitions),
            inputs: Some(inputs),
            ..Default::default()
        },
    );
    let machines = fbb.create_vector(&[machine]);

    let name = fbb.create_string("Crossfade");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            canvas_width: 200.0,
            canvas_height: 200.0,
            nodes: Some(nodes),
            animations: Some(animations),
            state_machines: Some(machines),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

/// A clip that holds `value` for its whole duration, so any difference in the
/// pose is the blend rather than the playhead.
fn constant_clip<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    value: f32,
) -> flatbuffers::WIPOffset<Animation<'a>> {
    let mut keyframes = Vec::new();
    for (index, time) in [0.0f32, 1.0].into_iter().enumerate() {
        let kf_id = fbb.create_string(if index == 0 { "k0" } else { "k1" });
        keyframes.push(KFKeyframe::create(
            fbb,
            &KFKeyframeArgs {
                id: Some(kf_id),
                time,
                scalar_value: value,
                ..Default::default()
            },
        ));
    }
    let keyframes = fbb.create_vector(&keyframes);
    let track_id = fbb.create_string("track");
    let target = fbb.create_string("box");
    let track = KFTrack::create(
        fbb,
        &KFTrackArgs {
            id: Some(track_id),
            target_node_id: Some(target),
            property: AnimatedPropertyKind::PositionX,
            keyframes: Some(keyframes),
            ..Default::default()
        },
    );
    let tracks = fbb.create_vector(&[track]);
    let id = fbb.create_string(id);
    let name = fbb.create_string("Clip");
    Animation::create(
        fbb,
        &AnimationArgs {
            id: Some(id),
            name: Some(name),
            duration: 1.0,
            tracks: Some(tracks),
            ..Default::default()
        },
    )
}

fn sm_state<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    clip_id: &'static str,
) -> flatbuffers::WIPOffset<SMState<'a>> {
    let id = fbb.create_string(id);
    let name = fbb.create_string("State");
    let clip_id = fbb.create_string(clip_id);
    SMState::create(
        fbb,
        &SMStateArgs {
            id: Some(id),
            name: Some(name),
            clip_id: Some(clip_id),
            ..Default::default()
        },
    )
}

fn box_node<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
) -> flatbuffers::WIPOffset<Node<'a>> {
    let rect = Rectangle::create(
        fbb,
        &RectangleArgs {
            width: 10.0,
            height: 10.0,
            ..Default::default()
        },
    );
    let shape = Shape::create(
        fbb,
        &ShapeArgs {
            geometry_type: Geometry::Rectangle,
            geometry: Some(rect.as_union_value()),
            ..Default::default()
        },
    );
    let transform = Transform::create(fbb, &TransformArgs::default());
    let id = fbb.create_string("box");
    let name = fbb.create_string("Box");
    Node::create(
        fbb,
        &NodeArgs {
            id: Some(id),
            name: Some(name),
            component_type: Component::Shape,
            component: Some(shape.as_union_value()),
            transform: Some(transform),
            visible: true,
            opacity: 1.0,
            ..Default::default()
        },
    )
}


/// A boolean flipping has to change the *pose*, not just the active state.
///
/// Every existing test stopped at the state id, so a machine that transitioned
/// correctly while the frame kept posing the old clip would have passed all of
/// them — which is exactly the shape of "the state changes in Kineto's test
/// mode but nothing moves in the app" (issue #111).
#[test]
fn flipping_a_boolean_poses_the_destination_clip() {
    let mut player = KinetoPlayer::load(build_bool_doc()).expect("document should load");
    let frame = player.play().expect("frame");
    assert_eq!(position_x(&frame), 0.0, "the source clip holds 0");

    player.set_bool("machine", "go", true).expect("bool");
    // Duration 0: the transition completes on the advance that fires it.
    let frame = player.advance(0.016).expect("frame");

    assert_eq!(
        player.active_state("machine"),
        Some("to"),
        "the machine should have moved on"
    );
    assert_eq!(
        position_x(&frame),
        100.0,
        "and the frame should be posing the destination clip"
    );
}

/// Going to a one-shot state and back must leave the looping clip running.
///
/// A machine's clip runs on that state's own clock, which restarts when the
/// state is entered — that is what the editor samples. The runtime sampled the
/// player's single global playhead, and a one-shot finishing pins that
/// playhead to its duration *and stops the player*, so coming back to a
/// looping clip rendered its last frame forever (issue #111).
#[test]
fn returning_to_a_looping_clip_starts_it_again() {
    let mut player = KinetoPlayer::load(build_round_trip_doc()).expect("load");
    player.play().expect("frame");

    // Into the one-shot, and let it run past its end.
    player.set_bool("machine", "go", true).expect("bool");
    for _ in 0..200 {
        player.advance(0.016).expect("frame");
    }
    assert_eq!(player.active_state("machine"), Some("to"));

    // Back to the looping clip.
    player.set_bool("machine", "go", false).expect("bool");
    player.advance(0.016).expect("frame");
    assert_eq!(player.active_state("machine"), Some("from"));

    // It has to move again, not hold one frame.
    let first = position_x(&player.advance(0.016).expect("frame"));
    let mut moved = false;
    for _ in 0..40 {
        if (position_x(&player.advance(0.016).expect("frame")) - first).abs() > 1e-6 {
            moved = true;
            break;
        }
    }
    assert!(moved, "the looping clip should be playing again, it held {first}");
}

/// Two clips: a *moving* loop and a one-shot, so the test above can tell
/// "playing" from "frozen".
fn build_round_trip_doc() -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(8192);

    // The looping clip ramps positionX across its two seconds.
    let mut keyframes = Vec::new();
    for (index, (time, value)) in [(0.0f32, 0.0f32), (2.0, 200.0)].into_iter().enumerate() {
        let kf_id = fbb.create_string(if index == 0 { "a0" } else { "a1" });
        keyframes.push(KFKeyframe::create(
            &mut fbb,
            &KFKeyframeArgs {
                id: Some(kf_id),
                time,
                scalar_value: value,
                ..Default::default()
            },
        ));
    }
    let keyframes = fbb.create_vector(&keyframes);
    let track_id = fbb.create_string("t");
    let target = fbb.create_string("box");
    let track = KFTrack::create(
        &mut fbb,
        &KFTrackArgs {
            id: Some(track_id),
            target_node_id: Some(target),
            property: AnimatedPropertyKind::PositionX,
            keyframes: Some(keyframes),
            ..Default::default()
        },
    );
    let tracks = fbb.create_vector(&[track]);
    let loop_id = fbb.create_string("clip-from");
    let loop_name = fbb.create_string("Loop");
    let looping = Animation::create(
        &mut fbb,
        &AnimationArgs {
            id: Some(loop_id),
            name: Some(loop_name),
            duration: 2.0,
            loop_: true,
            loop_mode: LoopMode::Loop,
            tracks: Some(tracks),
            ..Default::default()
        },
    );

    let once = constant_clip(&mut fbb, "clip-to", 500.0);
    let animations = fbb.create_vector(&[looping, once]);

    let node = box_node(&mut fbb);
    let nodes = fbb.create_vector(&[node]);

    let from = sm_state(&mut fbb, "from", "clip-from");
    let to = sm_state(&mut fbb, "to", "clip-to");
    let states = fbb.create_vector(&[from, to]);

    let input_id = fbb.create_string("go");
    let input_name = fbb.create_string("go");
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

    let mut transitions = Vec::new();
    for (id, from_id, to_id, expected) in [
        ("t-go", "from", "to", true),
        ("t-back", "to", "from", false),
    ] {
        let condition_input = fbb.create_string("go");
        let condition = TransitionCondition::create(
            &mut fbb,
            &TransitionConditionArgs {
                kind: TransitionConditionKind::BoolEquals,
                input_id: Some(condition_input),
                bool_expected: expected,
                ..Default::default()
            },
        );
        let conditions = fbb.create_vector(&[condition]);
        let tid = fbb.create_string(id);
        let f = fbb.create_string(from_id);
        let t = fbb.create_string(to_id);
        transitions.push(SMTransition::create(
            &mut fbb,
            &SMTransitionArgs {
                id: Some(tid),
                from_state_id: Some(f),
                to_state_id: Some(t),
                conditions: Some(conditions),
                duration_seconds: 0.0,
                enabled: true,
                ..Default::default()
            },
        ));
    }
    let transitions = fbb.create_vector(&transitions);

    let machine_id = fbb.create_string("machine");
    let machine_name = fbb.create_string("Machine");
    let initial = fbb.create_string("from");
    let machine = StateMachine::create(
        &mut fbb,
        &StateMachineArgs {
            id: Some(machine_id),
            name: Some(machine_name),
            initial_state_id: Some(initial),
            states: Some(states),
            transitions: Some(transitions),
            inputs: Some(inputs),
            ..Default::default()
        },
    );
    let machines = fbb.create_vector(&[machine]);

    let name = fbb.create_string("Round trip");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            canvas_width: 200.0,
            canvas_height: 200.0,
            nodes: Some(nodes),
            animations: Some(animations),
            state_machines: Some(machines),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}
