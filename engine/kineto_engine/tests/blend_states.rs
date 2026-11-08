//! Blend trees (issue #111).
//!
//! A blend state exported as an animation state with an empty clip id and
//! posed nothing on device while the editor blended.

use kineto_engine::state_machine::blend_states::{evaluate_1d, evaluate_2d};

fn weights(result: &[kineto_engine::state_machine::blend_states::BlendWeight]) -> Vec<(&str, f64)> {
    result
        .iter()
        .map(|w| (w.clip_id.as_str(), w.weight))
        .collect()
}

#[test]
fn a_one_dimensional_blend_interpolates_between_neighbours() {
    let points = vec![
        ("walk".to_string(), 0.0),
        ("run".to_string(), 10.0),
    ];

    let got = evaluate_1d(&points, 2.5);
    assert_eq!(weights(&got), vec![("walk", 0.75), ("run", 0.25)]);
}

/// Past either end the nearest clip takes the whole pose, rather than fading
/// out into the rest pose.
#[test]
fn a_one_dimensional_blend_holds_its_ends() {
    let points = vec![
        ("walk".to_string(), 0.0),
        ("run".to_string(), 10.0),
    ];

    assert_eq!(weights(&evaluate_1d(&points, -5.0)), vec![("walk", 1.0)]);
    assert_eq!(weights(&evaluate_1d(&points, 99.0)), vec![("run", 1.0)]);
}

/// Points are sorted by threshold, so a tree authored out of order blends the
/// same as one authored in order.
#[test]
fn a_one_dimensional_blend_sorts_its_points() {
    let points = vec![
        ("run".to_string(), 10.0),
        ("walk".to_string(), 0.0),
    ];

    assert_eq!(
        weights(&evaluate_1d(&points, 2.5)),
        vec![("walk", 0.75), ("run", 0.25)]
    );
}

#[test]
fn a_two_dimensional_blend_is_barycentric_over_the_nearest_three() {
    let points = vec![
        ("a".to_string(), 0.0, 0.0),
        ("b".to_string(), 1.0, 0.0),
        ("c".to_string(), 0.0, 1.0),
    ];

    // The centroid weights all three equally.
    let got = evaluate_2d(&points, 1.0 / 3.0, 1.0 / 3.0);
    assert_eq!(got.len(), 3);
    for weight in &got {
        assert!(
            (weight.weight - 1.0 / 3.0).abs() < 1e-6,
            "expected an even split, got {weight:?}"
        );
    }
}

/// Outside the triangle the raw barycentric weights go negative; clamping and
/// renormalising pins the pose to the nearest edge instead of inverting it.
#[test]
fn a_two_dimensional_blend_clamps_outside_its_triangle() {
    let points = vec![
        ("a".to_string(), 0.0, 0.0),
        ("b".to_string(), 1.0, 0.0),
        ("c".to_string(), 0.0, 1.0),
    ];

    let got = evaluate_2d(&points, -5.0, -5.0);
    let total: f64 = got.iter().map(|w| w.weight).sum();
    assert!((total - 1.0).abs() < 1e-9, "weights must still partition 1");
    assert!(got.iter().all(|w| w.weight >= 0.0));
}

/// Two points make a line, not a triangle.
#[test]
fn a_two_point_plane_degrades_to_a_line() {
    let points = vec![
        ("a".to_string(), 0.0, 0.0),
        ("b".to_string(), 10.0, 0.0),
    ];

    assert_eq!(
        weights(&evaluate_2d(&points, 2.5, 0.0)),
        vec![("a", 0.75), ("b", 0.25)]
    );
}

/// Three collinear points have no interior to interpolate across.
#[test]
fn a_degenerate_triangle_falls_back_to_the_nearest_point() {
    let points = vec![
        ("a".to_string(), 0.0, 0.0),
        ("b".to_string(), 1.0, 0.0),
        ("c".to_string(), 2.0, 0.0),
    ];

    let got = evaluate_2d(&points, 0.1, 0.0);
    assert_eq!(weights(&got), vec![("a", 1.0)]);
}

#[test]
fn an_empty_tree_blends_nothing() {
    assert!(evaluate_1d(&[], 1.0).is_empty());
    assert!(evaluate_2d(&[], 1.0, 1.0).is_empty());
}

/// End to end: the player has to pose a blend state, not just compute weights.
#[test]
fn the_player_poses_an_active_blend_state() {
    use kineto_engine::player::KinetoPlayer;

    let mut player = KinetoPlayer::load(build_blend_doc()).expect("document should load");
    player.play().expect("frame");

    // The input sits at 2.5 of a 0..10 axis, so the pose is three quarters of
    // the first clip and one quarter of the second: 0.75*0 + 0.25*100 = 25.
    player.set_number("machine", "speed", 2.5).expect("number");
    let frame = player.advance(0.0).expect("frame");
    let x = frame
        .node_overrides
        .get("box")
        .and_then(|v| v.scalar("positionX"))
        .expect("the blend should pose positionX");
    assert!((x - 25.0).abs() < 1e-6, "expected 25, got {x}");

    // And the far end is the second clip alone.
    player.set_number("machine", "speed", 10.0).expect("number");
    let frame = player.advance(0.0).expect("frame");
    let x = frame
        .node_overrides
        .get("box")
        .and_then(|v| v.scalar("positionX"))
        .expect("posed");
    assert!((x - 100.0).abs() < 1e-6, "expected 100, got {x}");
}

fn build_blend_doc() -> Vec<u8> {
    build_blend_doc_with(
        10.0,
        kineto_engine::format::generated::kineto::InputSource::Manual,
    )
}

fn build_blend_doc_with(
    far_threshold: f32,
    source: kineto_engine::format::generated::kineto::InputSource,
) -> Vec<u8> {
    use kineto_engine::format::generated::kineto::*;

    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(8192);

    let mut clips = Vec::new();
    for (id, value) in [("slow", 0.0f32), ("fast", 100.0)] {
        let mut keyframes = Vec::new();
        for (index, time) in [0.0f32, 1.0].into_iter().enumerate() {
            let kf_id = fbb.create_string(if index == 0 { "k0" } else { "k1" });
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
        let track_id = fbb.create_string("track");
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
        let clip_id = fbb.create_string(id);
        let clip_name = fbb.create_string(id);
        clips.push(Animation::create(
            &mut fbb,
            &AnimationArgs {
                id: Some(clip_id),
                name: Some(clip_name),
                duration: 1.0,
                tracks: Some(tracks),
                ..Default::default()
            },
        ));
    }
    let animations = fbb.create_vector(&clips);

    let rect = Rectangle::create(
        &mut fbb,
        &RectangleArgs {
            width: 10.0,
            height: 10.0,
            ..Default::default()
        },
    );
    let shape = Shape::create(
        &mut fbb,
        &ShapeArgs {
            geometry_type: Geometry::Rectangle,
            geometry: Some(rect.as_union_value()),
            ..Default::default()
        },
    );
    let transform = Transform::create(&mut fbb, &TransformArgs::default());
    let node_id = fbb.create_string("box");
    let node_name = fbb.create_string("Box");
    let node = Node::create(
        &mut fbb,
        &NodeArgs {
            id: Some(node_id),
            name: Some(node_name),
            component_type: Component::Shape,
            component: Some(shape.as_union_value()),
            transform: Some(transform),
            visible: true,
            opacity: 1.0,
            ..Default::default()
        },
    );
    let nodes = fbb.create_vector(&[node]);

    let mut points = Vec::new();
    for (clip, x) in [("slow", 0.0f32), ("fast", far_threshold)] {
        let clip_id = fbb.create_string(clip);
        points.push(BlendPoint::create(
            &mut fbb,
            &BlendPointArgs {
                clip_id: Some(clip_id),
                x,
                y: 0.0,
            },
        ));
    }
    let points = fbb.create_vector(&points);

    let state_id = fbb.create_string("locomotion");
    let state_name = fbb.create_string("Locomotion");
    let empty_clip = fbb.create_string("");
    let input_x = fbb.create_string("speed");
    let state = SMState::create(
        &mut fbb,
        &SMStateArgs {
            id: Some(state_id),
            name: Some(state_name),
            clip_id: Some(empty_clip),
            kind: SMStateKind::Blend1D,
            input_id_x: Some(input_x),
            blend_points: Some(points),
            ..Default::default()
        },
    );
    let states = fbb.create_vector(&[state]);

    let input_id = fbb.create_string("speed");
    let input_name = fbb.create_string("speed");
    let input = StateMachineInput::create(
        &mut fbb,
        &StateMachineInputArgs {
            id: Some(input_id),
            name: Some(input_name),
            kind: InputKind::Number,
            source,
            ..Default::default()
        },
    );
    let inputs = fbb.create_vector(&[input]);

    let machine_id = fbb.create_string("machine");
    let machine_name = fbb.create_string("Machine");
    let initial = fbb.create_string("locomotion");
    let machine = StateMachine::create(
        &mut fbb,
        &StateMachineArgs {
            id: Some(machine_id),
            name: Some(machine_name),
            initial_state_id: Some(initial),
            states: Some(states),
            inputs: Some(inputs),
            ..Default::default()
        },
    );
    let machines = fbb.create_vector(&[machine]);

    let name = fbb.create_string("Blend Fixture");
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

/// A pointer-driven input takes the cursor's normalised position. The runtime
/// had no source field at all, so a document with one sat still unless the
/// host app knew to feed it (issue #111).
#[test]
fn a_pointer_driven_input_follows_the_cursor() {
    use kineto_engine::format::ListenerEventKind;
    use kineto_engine::player::KinetoPlayer;

    let mut player = KinetoPlayer::load(build_pointer_driven_doc()).expect("load");
    player.play().expect("frame");

    // Half way across a 200-wide canvas normalises to 0.5, and the blend axis
    // runs 0..1 here, so the pose is the midpoint of the two clips.
    player
        .pointer_event(ListenerEventKind::PointerDown, 100.0, 0.0)
        .expect("pointer");
    let frame = player.advance(0.0).expect("frame");
    let x = frame
        .node_overrides
        .get("box")
        .and_then(|v| v.scalar("positionX"))
        .expect("posed");
    assert!((x - 50.0).abs() < 1e-6, "expected the midpoint, got {x}");

    // Past the right edge it clamps rather than running away.
    player
        .pointer_event(ListenerEventKind::PointerDown, 9999.0, 0.0)
        .expect("pointer");
    let frame = player.advance(0.0).expect("frame");
    let x = frame
        .node_overrides
        .get("box")
        .and_then(|v| v.scalar("positionX"))
        .expect("posed");
    assert!((x - 100.0).abs() < 1e-6, "expected the far end, got {x}");
}

/// The same document as the blend fixture, with the axis normalised to 0..1
/// and its input driven by the pointer.
fn build_pointer_driven_doc() -> Vec<u8> {
    build_blend_doc_with(
        1.0,
        kineto_engine::format::generated::kineto::InputSource::PointerX,
    )
}
