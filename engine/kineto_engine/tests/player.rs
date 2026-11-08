use kineto_engine::format::generated::kineto::{
    finish_document_buffer, AnimatedPropertyKind, Animation, AnimationArgs, Component, Document,
    DocumentArgs, Geometry, InputKind, KFKeyframe, KFKeyframeArgs, KFTrack, KFTrackArgs,
    KeyframeInterpolationKind, LoopMode, Node, NodeArgs, Rectangle, RectangleArgs, SMState,
    SMStateArgs, SMTransition, SMTransitionArgs, Shape, ShapeArgs, StateMachine,
    StateMachineArgs, StateMachineInput, StateMachineInputArgs, Transform, TransformArgs,
    TransitionCondition, TransitionConditionArgs, TransitionConditionKind,
};
use kineto_engine::player::{KinetoPlayer, PlayState};

fn assert_close(actual: f64, expected: f64, epsilon: f64) {
    assert!(
        (actual - expected).abs() <= epsilon,
        "expected {actual} to be within {epsilon} of {expected}"
    );
}

#[test]
fn advances_animation_and_returns_node_overrides() {
    let bytes = build_player_doc();
    let mut player = KinetoPlayer::load(bytes).expect("player should load");

    player.play().expect("play should evaluate");
    let frame = player.advance(0.5).expect("advance should evaluate");

    assert_eq!(player.play_state(), PlayState::Playing);
    assert_close(frame.current_time, 0.5, 1e-9);
    assert_eq!(frame.active_animation_id.as_deref(), Some("idle"));
    assert_close(
        frame
            .node_overrides
            .get("button-node")
            .and_then(|overrides| overrides.scalar("opacity"))
            .expect("opacity override missing"),
        0.5,
        0.001,
    );
}

#[test]
fn state_machine_trigger_selects_destination_clip() {
    let bytes = build_player_doc();
    let mut player = KinetoPlayer::load(bytes).expect("player should load");

    player
        .set_trigger("sm1", "tap")
        .expect("trigger should queue");
    let frame = player.advance(0.016).expect("advance should evaluate");

    assert_eq!(player.active_state("sm1"), Some("pressed"));
    assert_eq!(frame.active_animation_id.as_deref(), Some("pressed"));
}

#[test]
fn hit_test_returns_topmost_visible_shape() {
    let bytes = build_player_doc();
    let player = KinetoPlayer::load(bytes).expect("player should load");

    let target = player
        .hit_test_document_point(50.0, 30.0)
        .expect("hit test should parse")
        .expect("target should hit");

    assert_eq!(target.node_id, "button-node");
    assert_eq!(target.node_name, "Button");
}

#[test]
fn hit_test_uses_ellipse_geometry() {
    let bytes = build_player_doc();
    let player = KinetoPlayer::load(bytes).expect("player should load");

    let outside = player
        .hit_test_document_point(130.0, 30.0)
        .expect("hit test should parse")
        .expect("background should hit outside circle");
    let inside = player
        .hit_test_document_point(150.0, 50.0)
        .expect("hit test should parse")
        .expect("circle should hit");

    assert_eq!(outside.node_id, "background");
    assert_eq!(inside.node_id, "circle-node");
}

#[test]
fn reports_cross_fade_transition_progress() {
    let bytes = build_player_doc_with_transition_duration(0.5);
    let mut player = KinetoPlayer::load(bytes).expect("player should load");

    player
        .set_trigger("sm1", "tap")
        .expect("trigger should queue");
    player.advance(0.016).expect("transition should start");
    assert_eq!(player.active_transition("sm1"), Some("tap-transition"));

    player.advance(0.25).expect("transition should progress");
    let progress = player
        .transition_progress("sm1")
        .expect("transition progress should exist");

    assert!(progress > 0.0 && progress < 1.0, "progress={progress}");
}

#[test]
fn ping_pong_ascends_then_strictly_descends_under_frame_sized_ticks() {
    // Real 60Hz per-frame ticking (not two coarse ticks) — the shape that
    // actually runs on device and is what froze at the clip end before the
    // direction-state fix (issue #43).
    let bytes = build_ping_pong_doc();
    let mut player = KinetoPlayer::load(bytes).expect("player should load");
    player.play().expect("play should evaluate");

    let mut samples = Vec::new();
    for _ in 0..300 {
        let frame = player.advance(1.0 / 60.0).expect("advance should evaluate");
        samples.push(frame.current_time);
    }

    let (peak_index, peak) = samples
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, v)| (i, *v))
        .expect("samples should be non-empty");
    assert_close(peak, 1.0, 0.02);
    assert!(peak_index > 0 && peak_index < samples.len() - 1);

    for i in (peak_index + 1)..(peak_index + 10).min(samples.len()) {
        assert!(
            samples[i] <= samples[i - 1] + 1e-9,
            "expected strictly descending time after the bounce at index {i}: {:?}",
            &samples[peak_index..(peak_index + 10).min(samples.len())]
        );
    }

    for t in &samples {
        assert!(*t >= -1e-9 && *t <= 1.0 + 1e-9, "time out of range: {t}");
    }
}

#[test]
fn ping_pong_handles_a_single_dt_larger_than_duration() {
    let bytes = build_ping_pong_doc();
    let mut player = KinetoPlayer::load(bytes).expect("player should load");
    player.play().expect("play should evaluate");

    // duration=1.0; raw t=2.5 bounces 0→1→0→0.5 (two full reflections, then
    // ascending) → current_time should land at 0.5, not overshoot or NaN.
    let frame = player.advance(2.5).expect("advance should evaluate");
    assert_close(frame.current_time, 0.5, 0.01);
    assert!(frame.current_time.is_finite());
    assert_eq!(player.play_state(), PlayState::Playing);
}

#[test]
fn ping_pong_seek_resets_direction_to_forward() {
    let bytes = build_ping_pong_doc();
    let mut player = KinetoPlayer::load(bytes).expect("player should load");
    player.play().expect("play should evaluate");
    player.advance(1.6).expect("advance should evaluate"); // bounces once → backward

    player.seek(0.2).expect("seek should evaluate");
    let frame = player.advance(0.1).expect("advance should evaluate");
    assert!(
        frame.current_time > 0.2,
        "expected forward motion after seek, got {}",
        frame.current_time
    );
}

fn build_ping_pong_doc() -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(4096);

    let nodes = build_nodes(&mut fbb);
    let animation = ping_pong_animation(&mut fbb);
    let animations = fbb.create_vector(&[animation]);

    let name = fbb.create_string("PingPong Fixture");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            canvas_width: 200.0,
            canvas_height: 100.0,
            nodes: Some(nodes),
            animations: Some(animations),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

fn ping_pong_animation<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
) -> flatbuffers::WIPOffset<Animation<'a>> {
    let kf0_id = fbb.create_string("kf0");
    let kf0 = KFKeyframe::create(
        fbb,
        &KFKeyframeArgs {
            id: Some(kf0_id),
            time: 0.0,
            scalar_value: 0.0,
            interpolation: KeyframeInterpolationKind::Linear,
            ..Default::default()
        },
    );
    let kf1_id = fbb.create_string("kf1");
    let kf1 = KFKeyframe::create(
        fbb,
        &KFKeyframeArgs {
            id: Some(kf1_id),
            time: 1.0,
            scalar_value: 1.0,
            interpolation: KeyframeInterpolationKind::Linear,
            ..Default::default()
        },
    );
    let keyframes = fbb.create_vector(&[kf0, kf1]);
    let track_id = fbb.create_string("track-opacity");
    let target_id = fbb.create_string("button-node");
    let track = KFTrack::create(
        fbb,
        &KFTrackArgs {
            id: Some(track_id),
            target_node_id: Some(target_id),
            property: AnimatedPropertyKind::Opacity,
            keyframes: Some(keyframes),
        },
    );
    let tracks = fbb.create_vector(&[track]);
    let id = fbb.create_string("bounce");
    let name = fbb.create_string("Bounce");
    Animation::create(
        fbb,
        &AnimationArgs {
            id: Some(id),
            name: Some(name),
            duration: 1.0,
            tracks: Some(tracks),
            loop_mode: LoopMode::PingPong,
            ..Default::default()
        },
    )
}

fn build_player_doc() -> Vec<u8> {
    build_player_doc_with_transition_duration(0.0)
}

fn build_player_doc_with_transition_duration(duration: f32) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(4096);

    let nodes = build_nodes(&mut fbb);
    let animations = build_animations(&mut fbb);
    let state_machines = build_state_machine(&mut fbb, duration);

    let name = fbb.create_string("Player Fixture");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            canvas_width: 200.0,
            canvas_height: 100.0,
            nodes: Some(nodes),
            animations: Some(animations),
            state_machines: Some(state_machines),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

fn build_nodes<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
) -> flatbuffers::WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<Node<'a>>>> {
    let background = rect_node(
        fbb,
        "background",
        "Background",
        0.0,
        0.0,
        200.0,
        100.0,
        true,
    );
    let button = rect_node(fbb, "button-node", "Button", 40.0, 20.0, 60.0, 40.0, true);
    let hidden = rect_node(
        fbb,
        "hidden-cover",
        "Hidden Cover",
        35.0,
        15.0,
        80.0,
        50.0,
        false,
    );
    let circle = ellipse_node(fbb, "circle-node", "Circle", 130.0, 30.0, 40.0, 40.0);
    fbb.create_vector(&[background, button, hidden, circle])
}

fn rect_node<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    name: &'static str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    visible: bool,
) -> flatbuffers::WIPOffset<Node<'a>> {
    let rect = Rectangle::create(
        fbb,
        &RectangleArgs {
            width,
            height,
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
    node_with_shape(fbb, id, name, x, y, shape, visible)
}

fn ellipse_node<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    name: &'static str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> flatbuffers::WIPOffset<Node<'a>> {
    let ellipse = kineto_engine::format::generated::kineto::Ellipse::create(
        fbb,
        &kineto_engine::format::generated::kineto::EllipseArgs { width, height },
    );
    let shape = Shape::create(
        fbb,
        &ShapeArgs {
            geometry_type: Geometry::Ellipse,
            geometry: Some(ellipse.as_union_value()),
            ..Default::default()
        },
    );
    node_with_shape(fbb, id, name, x, y, shape, true)
}

fn node_with_shape<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    name: &'static str,
    x: f32,
    y: f32,
    shape: flatbuffers::WIPOffset<Shape<'a>>,
    visible: bool,
) -> flatbuffers::WIPOffset<Node<'a>> {
    let transform = Transform::create(
        fbb,
        &TransformArgs {
            x,
            y,
            ..Default::default()
        },
    );
    let id = fbb.create_string(id);
    let name = fbb.create_string(name);
    Node::create(
        fbb,
        &NodeArgs {
            id: Some(id),
            name: Some(name),
            component_type: Component::Shape,
            component: Some(shape.as_union_value()),
            transform: Some(transform),
            visible,
            ..Default::default()
        },
    )
}

fn build_animations<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
) -> flatbuffers::WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<Animation<'a>>>> {
    let idle = animation(fbb, "idle", "Idle", 0.0, 1.0);
    let pressed = animation(fbb, "pressed", "Pressed", 1.0, 0.25);
    fbb.create_vector(&[idle, pressed])
}

fn animation<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    name: &'static str,
    start: f32,
    end: f32,
) -> flatbuffers::WIPOffset<Animation<'a>> {
    let kf0_id = fbb.create_string("kf0");
    let kf0 = KFKeyframe::create(
        fbb,
        &KFKeyframeArgs {
            id: Some(kf0_id),
            time: 0.0,
            scalar_value: start,
            interpolation: KeyframeInterpolationKind::Linear,
            ..Default::default()
        },
    );
    let kf1_id = fbb.create_string("kf1");
    let kf1 = KFKeyframe::create(
        fbb,
        &KFKeyframeArgs {
            id: Some(kf1_id),
            time: 1.0,
            scalar_value: end,
            interpolation: KeyframeInterpolationKind::Linear,
            ..Default::default()
        },
    );
    let keyframes = fbb.create_vector(&[kf0, kf1]);
    let track_id = fbb.create_string("track-opacity");
    let target_id = fbb.create_string("button-node");
    let track = KFTrack::create(
        fbb,
        &KFTrackArgs {
            id: Some(track_id),
            target_node_id: Some(target_id),
            property: AnimatedPropertyKind::Opacity,
            keyframes: Some(keyframes),
        },
    );
    let tracks = fbb.create_vector(&[track]);
    let id = fbb.create_string(id);
    let name = fbb.create_string(name);
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

fn build_state_machine<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    duration: f32,
) -> flatbuffers::WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<StateMachine<'a>>>>
{
    let idle = sm_state(fbb, "idle", "idle");
    let pressed = sm_state(fbb, "pressed", "pressed");
    let states = fbb.create_vector(&[idle, pressed]);

    let input_id = fbb.create_string("tap-input");
    let input_name = fbb.create_string("tap");
    let input = StateMachineInput::create(
        fbb,
        &StateMachineInputArgs {
            id: Some(input_id),
            name: Some(input_name),
            kind: InputKind::Trigger,
            ..Default::default()
        },
    );
    let inputs = fbb.create_vector(&[input]);

    let condition_input = fbb.create_string("tap-input");
    let condition = TransitionCondition::create(
        fbb,
        &TransitionConditionArgs {
            kind: TransitionConditionKind::TriggerFired,
            input_id: Some(condition_input),
            ..Default::default()
        },
    );
    let conditions = fbb.create_vector(&[condition]);
    let transition_id = fbb.create_string("tap-transition");
    let from = fbb.create_string("idle");
    let to = fbb.create_string("pressed");
    let transition = SMTransition::create(
        fbb,
        &SMTransitionArgs {
            id: Some(transition_id),
            from_state_id: Some(from),
            to_state_id: Some(to),
            conditions: Some(conditions),
            duration_seconds: duration,
            easing: 0,
            enabled: true,
            audio_event_ids: None,
        },
    );
    let transitions = fbb.create_vector(&[transition]);

    let sm_id = fbb.create_string("sm1");
    let sm_name = fbb.create_string("Character");
    let initial = fbb.create_string("idle");
    let machine = StateMachine::create(
        fbb,
        &StateMachineArgs {
            id: Some(sm_id),
            name: Some(sm_name),
            initial_state_id: Some(initial),
            states: Some(states),
            transitions: Some(transitions),
            inputs: Some(inputs),
            ..Default::default()
        },
    );
    fbb.create_vector(&[machine])
}

fn sm_state<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    clip_id: &'static str,
) -> flatbuffers::WIPOffset<SMState<'a>> {
    let id_offset = fbb.create_string(id);
    let name = fbb.create_string(id);
    let clip_id = fbb.create_string(clip_id);
    SMState::create(
        fbb,
        &SMStateArgs {
            id: Some(id_offset),
            name: Some(name),
            clip_id: Some(clip_id),
            ..Default::default()
        },
    )
}
