//! Issue #74: a number input driving a property through the runtime.

use std::collections::HashMap;

use kineto_engine::animation::NodeOverrides;
use kineto_engine::bindings::apply_bindings;
use kineto_engine::format::generated::kineto::{
    finish_document_buffer, root_as_document, AnimatedPropertyKind, Document, DocumentArgs,
    IkTarget, IkTargetArgs, InputBinding, InputBindingArgs, InputKind, SMState, SMStateArgs,
    StateMachine, StateMachineArgs, StateMachineInput, StateMachineInputArgs,
};
use kineto_engine::player::KinetoPlayer;

struct TargetSpec {
    id: &'static str,
    position: (f32, f32),
    extent: (f32, f32),
}

struct BindingSpec {
    input_id: &'static str,
    subject_id: &'static str,
    property: AnimatedPropertyKind,
    mapping: Option<(f32, f32)>,
    invert: bool,
}

fn spatial(input_id: &'static str, subject_id: &'static str, property: AnimatedPropertyKind) -> BindingSpec {
    BindingSpec {
        input_id,
        subject_id,
        property,
        mapping: None,
        invert: false,
    }
}

fn build_doc(targets: Vec<TargetSpec>, bindings: Vec<BindingSpec>, number_default: f32) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(2048);

    let target_offsets: Vec<_> = targets
        .iter()
        .map(|spec| {
            let id = fbb.create_string(spec.id);
            let chain_root_id = fbb.create_string("root");
            IkTarget::create(
                &mut fbb,
                &IkTargetArgs {
                    id: Some(id),
                    chain_root_id: Some(chain_root_id),
                    position_x: spec.position.0,
                    position_y: spec.position.1,
                    extent_w: spec.extent.0,
                    extent_h: spec.extent.1,
                    // Every fixture target is draggable: the interactive flag
                    // is what lets a pointer drive its bound axes at all.
                    interactive: true,
                    ..Default::default()
                },
            )
        })
        .collect();
    let targets_vec = fbb.create_vector(&target_offsets);

    let binding_offsets: Vec<_> = bindings
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            let id = fbb.create_string(&format!("binding_{index}"));
            let input_id = fbb.create_string(spec.input_id);
            let subject_id = fbb.create_string(spec.subject_id);
            InputBinding::create(
                &mut fbb,
                &InputBindingArgs {
                    id: Some(id),
                    input_id: Some(input_id),
                    subject_id: Some(subject_id),
                    property: spec.property,
                    has_mapping: spec.mapping.is_some(),
                    mapping_min: spec.mapping.map(|range| range.0).unwrap_or(0.0),
                    mapping_max: spec.mapping.map(|range| range.1).unwrap_or(0.0),
                    invert: spec.invert,
                },
            )
        })
        .collect();
    let bindings_vec = fbb.create_vector(&binding_offsets);

    // One input per distinct id the bindings name, so a fixture binding two
    // axes actually has two inputs to write. It used to make exactly one, and
    // a second binding then resolved to nothing — which reads as "the runtime
    // ignored that axis" rather than "the fixture never gave it an input".
    let mut input_ids: Vec<&'static str> = bindings.iter().map(|b| b.input_id).collect();
    input_ids.dedup();
    let input_offsets: Vec<_> = input_ids
        .iter()
        .map(|id| {
            let input_id = fbb.create_string(id);
            // "pointer_x" is addressed as "Pointer X" by the set-number test,
            // which drives inputs by their display name.
            let input_name = fbb.create_string(match *id {
                "pointer_x" => "Pointer X",
                "pointer_y" => "Pointer Y",
                other => other,
            });
            StateMachineInput::create(
                &mut fbb,
                &StateMachineInputArgs {
                    id: Some(input_id),
                    name: Some(input_name),
                    kind: InputKind::Number,
                    default_number_value: number_default,
                    ..Default::default()
                },
            )
        })
        .collect();
    let inputs_vec = fbb.create_vector(&input_offsets);

    let state_id = fbb.create_string("idle");
    let state_name = fbb.create_string("Idle");
    let clip_id = fbb.create_string("");
    let state = SMState::create(
        &mut fbb,
        &SMStateArgs {
            id: Some(state_id),
            name: Some(state_name),
            clip_id: Some(clip_id),
            ..Default::default()
        },
    );
    let states_vec = fbb.create_vector(&[state]);

    let machine_id = fbb.create_string("machine");
    let machine_name = fbb.create_string("Machine");
    let initial_state_id = fbb.create_string("idle");
    let machine = StateMachine::create(
        &mut fbb,
        &StateMachineArgs {
            id: Some(machine_id),
            name: Some(machine_name),
            initial_state_id: Some(initial_state_id),
            states: Some(states_vec),
            inputs: Some(inputs_vec),
            ..Default::default()
        },
    );
    let machines_vec = fbb.create_vector(&[machine]);

    let doc_name = fbb.create_string("bindings");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(doc_name),
            ik_targets: Some(targets_vec),
            state_machines: Some(machines_vec),
            input_bindings: Some(bindings_vec),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

fn overrides_for(bytes: &[u8], values: &[(&str, f64)]) -> HashMap<String, NodeOverrides> {
    let document = root_as_document(bytes).expect("document should parse");
    let number_values: HashMap<String, f64> = values
        .iter()
        .map(|(id, value)| ((*id).to_owned(), *value))
        .collect();
    let mut base = HashMap::new();
    apply_bindings(&document, &number_values, &mut base);
    base
}

fn hand(extent: (f32, f32)) -> TargetSpec {
    TargetSpec {
        id: "hand",
        position: (100.0, 100.0),
        extent,
    }
}

#[test]
fn spatial_binding_walks_the_targets_extent() {
    let bytes = build_doc(
        vec![hand((200.0, 0.0))],
        vec![spatial(
            "pointer_x",
            "hand",
            AnimatedPropertyKind::IkTargetPositionX,
        )],
        0.0,
    );

    // Extent 200 centred on 100 → the input's 0..1 spans 0..200.
    for (input, expected) in [(0.0, 0.0), (0.5, 100.0), (1.0, 200.0)] {
        let overrides = overrides_for(&bytes, &[("pointer_x", input)]);
        assert_eq!(
            overrides["hand"].scalar("ikTargetPositionX"),
            Some(expected),
            "input {input} should place the target at {expected}"
        );
    }
}

#[test]
fn the_vertical_axis_walks_its_own_side_of_the_extent() {
    // Nothing in the pipeline treats the axes differently; this is here so a
    // change that quietly makes one of them special fails out loud.
    let bytes = build_doc(
        vec![hand((0.0, 80.0))],
        vec![spatial(
            "pointer_x",
            "hand",
            AnimatedPropertyKind::IkTargetPositionY,
        )],
        0.0,
    );

    // Extent height 80 centred on 100 → 60..140, and X is left alone.
    let overrides = overrides_for(&bytes, &[("pointer_x", 0.25)]);
    assert_eq!(
        overrides["hand"].scalar("ikTargetPositionY"),
        Some(80.0)
    );
    assert_eq!(overrides["hand"].scalar("ikTargetPositionX"), None);
}

#[test]
fn out_of_range_input_clamps_to_the_extent() {
    let bytes = build_doc(
        vec![hand((200.0, 0.0))],
        vec![spatial(
            "pointer_x",
            "hand",
            AnimatedPropertyKind::IkTargetPositionX,
        )],
        0.0,
    );

    // A host feeding raw pointer coordinates pins the rig at the edge rather
    // than flinging it off-canvas.
    let overrides = overrides_for(&bytes, &[("pointer_x", 12.5)]);
    assert_eq!(overrides["hand"].scalar("ikTargetPositionX"), Some(200.0));
}

#[test]
fn invert_flips_which_end_zero_lands_on() {
    let bytes = build_doc(
        vec![hand((200.0, 0.0))],
        vec![BindingSpec {
            invert: true,
            ..spatial("pointer_x", "hand", AnimatedPropertyKind::IkTargetPositionX)
        }],
        0.0,
    );

    let overrides = overrides_for(&bytes, &[("pointer_x", 0.0)]);
    assert_eq!(overrides["hand"].scalar("ikTargetPositionX"), Some(200.0));
}

#[test]
fn a_point_target_has_nowhere_to_travel() {
    // Collapsing an extent parks the binding instead of destroying it.
    let bytes = build_doc(
        vec![hand((0.0, 0.0))],
        vec![spatial(
            "pointer_x",
            "hand",
            AnimatedPropertyKind::IkTargetPositionX,
        )],
        0.0,
    );

    let overrides = overrides_for(&bytes, &[("pointer_x", 1.0)]);
    assert!(!overrides.contains_key("hand"));
}

#[test]
fn an_undriven_input_moves_nothing() {
    // "Not driven yet" is not "driven to the bottom of its range".
    let bytes = build_doc(
        vec![hand((200.0, 0.0))],
        vec![spatial(
            "pointer_x",
            "hand",
            AnimatedPropertyKind::IkTargetPositionX,
        )],
        0.0,
    );

    let overrides = overrides_for(&bytes, &[]);
    assert!(overrides.is_empty());
}

#[test]
fn a_typed_mapping_ignores_the_subjects_extent() {
    let bytes = build_doc(
        vec![hand((200.0, 0.0))],
        vec![BindingSpec {
            mapping: Some((0.0, 90.0)),
            ..spatial("pointer_x", "hand", AnimatedPropertyKind::Rotation)
        }],
        0.0,
    );

    let overrides = overrides_for(&bytes, &[("pointer_x", 0.5)]);
    assert_eq!(overrides["hand"].scalar("rotation"), Some(45.0));
}

#[test]
fn a_binding_beats_a_keyframe_on_the_same_property() {
    use kineto_engine::animation::PropertyValue;

    let bytes = build_doc(
        vec![hand((200.0, 0.0))],
        vec![spatial(
            "pointer_x",
            "hand",
            AnimatedPropertyKind::IkTargetPositionX,
        )],
        0.0,
    );

    let document = root_as_document(&bytes).expect("document should parse");
    let mut base: HashMap<String, NodeOverrides> = HashMap::new();
    let clip = base.entry("hand".to_owned()).or_default();
    clip.values
        .insert("ikTargetPositionX", PropertyValue::Scalar(-999.0));
    clip.values
        .insert("ikTargetPositionY", PropertyValue::Scalar(42.0));

    let number_values = HashMap::from([("pointer_x".to_owned(), 1.0)]);
    apply_bindings(&document, &number_values, &mut base);

    assert_eq!(base["hand"].scalar("ikTargetPositionX"), Some(200.0));
    // The clip keeps every property the binding does not claim.
    assert_eq!(base["hand"].scalar("ikTargetPositionY"), Some(42.0));
}

#[test]
fn player_set_number_drives_the_frame() {
    let bytes = build_doc(
        vec![hand((200.0, 0.0))],
        vec![spatial(
            "pointer_x",
            "hand",
            AnimatedPropertyKind::IkTargetPositionX,
        )],
        0.0,
    );
    let mut player = KinetoPlayer::load(bytes).expect("player should load");

    player
        .set_number("machine", "Pointer X", 0.25)
        .expect("number input should be settable by name");

    let frame = player.current_frame().expect("frame should evaluate");
    assert_eq!(
        frame.node_overrides["hand"].scalar("ikTargetPositionX"),
        Some(50.0)
    );
}

#[test]
fn a_declared_default_drives_the_first_frame() {
    let bytes = build_doc(
        vec![hand((200.0, 0.0))],
        vec![spatial(
            "pointer_x",
            "hand",
            AnimatedPropertyKind::IkTargetPositionX,
        )],
        1.0,
    );
    let player = KinetoPlayer::load(bytes).expect("player should load");

    let frame = player.current_frame().expect("frame should evaluate");
    assert_eq!(
        frame.node_overrides["hand"].scalar("ikTargetPositionX"),
        Some(200.0)
    );
}

#[test]
fn setting_a_number_by_the_wrong_kind_is_rejected() {
    let bytes = build_doc(vec![hand((200.0, 0.0))], vec![], 0.0);
    let mut player = KinetoPlayer::load(bytes).expect("player should load");

    assert!(player.set_bool("machine", "Pointer X", true).is_err());
}

/// The reading half of this machinery always worked — `apply_bindings` maps a
/// 0..1 input across a target's extent — but nothing ever *wrote* that number,
/// so an interactive target did nothing at runtime while the editor's preview
/// moved it. A tap logged "0 listeners", which was true and useless: the
/// mechanism was never listeners (issue #111).
#[test]
fn a_tap_inside_an_interactive_targets_extent_drives_its_bound_axes() {
    let bytes = build_doc(
        vec![hand((200.0, 100.0))],
        vec![
            spatial("pointer_x", "hand", AnimatedPropertyKind::IkTargetPositionX),
            spatial("pointer_y", "hand", AnimatedPropertyKind::IkTargetPositionY),
        ],
        0.0,
    );
    let mut player = KinetoPlayer::load(bytes).expect("document should load");
    player.play().expect("frame");

    // The target sits at (100,100) with a 200x100 extent, so x spans 0..200
    // and y spans 50..150. A tap at (150, 125) is three quarters along both.
    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerDown, 150.0, 125.0)
        .expect("pointer");
    let frame = player.advance(0.0).expect("frame");

    assert_eq!(
        frame.node_overrides["hand"].scalar("ikTargetPositionX"),
        Some(150.0),
        "the tap should have moved the target under the finger"
    );
    assert_eq!(
        frame.node_overrides["hand"].scalar("ikTargetPositionY"),
        Some(125.0),
    );
}

/// Outside the box the target is not being dragged, so it must hold still
/// rather than snap to the edge.
#[test]
fn a_tap_outside_the_extent_leaves_the_target_alone() {
    let bytes = build_doc(
        vec![hand((200.0, 100.0))],
        vec![spatial("pointer_x", "hand", AnimatedPropertyKind::IkTargetPositionX)],
        0.25,
    );
    let mut player = KinetoPlayer::load(bytes).expect("document should load");
    player.play().expect("frame");

    let before = player.advance(0.0).expect("frame").node_overrides["hand"]
        .scalar("ikTargetPositionX");

    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerDown, 900.0, 900.0)
        .expect("pointer");
    let after = player.advance(0.0).expect("frame").node_overrides["hand"]
        .scalar("ikTargetPositionX");

    assert_eq!(before, after, "a tap elsewhere must not move the target");
}


// ── Follows pointer (the "exposes" interaction mode) ────────────────────────
//
// The third mode, and the one that never travelled at all: the target's
// position *becomes* the pointer's, so a constraint reading it aims whatever
// it drives at the finger. A document set up this way did nothing at runtime
// while the editor's preview followed along (issue #111).

use kineto_engine::format::generated::kineto::{TargetFollowFeel, TargetFollowScope};

fn follow_doc(scope: TargetFollowScope, feel: TargetFollowFeel) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(2048);
    let id = fbb.create_string("gaze");
    let chain_root_id = fbb.create_string("root");
    let target = IkTarget::create(
        &mut fbb,
        &IkTargetArgs {
            id: Some(id),
            chain_root_id: Some(chain_root_id),
            position_x: 100.0,
            position_y: 100.0,
            extent_w: 40.0,
            extent_h: 40.0,
            follows_pointer: true,
            follow_scope: scope,
            follow_feel: feel,
            ..Default::default()
        },
    );
    let targets = fbb.create_vector(&[target]);
    let name = fbb.create_string("Follow");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            canvas_width: 400.0,
            canvas_height: 400.0,
            ik_targets: Some(targets),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

fn gaze_position(player: &mut KinetoPlayer) -> (f64, f64) {
    let frame = player.advance(0.016).expect("frame");
    let target = frame
        .ik_targets
        .iter()
        .find(|t| t.id == "gaze")
        .expect("the target should be in the frame");
    (target.position.x, target.position.y)
}

/// Scope `Anywhere` is the "look at whatever I touch" case: a finger nowhere
/// near the target still drives it.
#[test]
fn a_snap_target_lands_on_the_pointer_anywhere_on_the_canvas() {
    let mut player = KinetoPlayer::load(follow_doc(
        TargetFollowScope::Anywhere,
        TargetFollowFeel::Snap,
    ))
    .expect("load");
    player.play().expect("frame");

    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerDown, 320.0, 40.0)
        .expect("pointer");

    assert_eq!(gaze_position(&mut player), (320.0, 40.0));
}

/// Scope `WithinExtent` only qualifies while the finger is inside the box.
#[test]
fn a_within_extent_target_ignores_a_finger_outside_its_box() {
    let mut player = KinetoPlayer::load(follow_doc(
        TargetFollowScope::WithinExtent,
        TargetFollowFeel::Snap,
    ))
    .expect("load");
    player.play().expect("frame");

    // Well outside the 40x40 box centred on (100,100).
    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerDown, 320.0, 40.0)
        .expect("pointer");
    assert_eq!(gaze_position(&mut player), (100.0, 100.0));

    // Inside it.
    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerDown, 110.0, 95.0)
        .expect("pointer");
    assert_eq!(gaze_position(&mut player), (110.0, 95.0));
}

/// Smooth eases toward the pointer instead of arriving: after one frame it has
/// closed part of the gap and no more.
#[test]
fn a_smooth_target_eases_toward_the_pointer() {
    let mut player = KinetoPlayer::load(follow_doc(
        TargetFollowScope::Anywhere,
        TargetFollowFeel::Smooth,
    ))
    .expect("load");
    player.play().expect("frame");

    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerDown, 300.0, 100.0)
        .expect("pointer");

    let (x, _) = gaze_position(&mut player);
    assert!(
        x > 100.0 && x < 300.0,
        "one frame should close part of the gap, got {x}"
    );

    // And it keeps closing.
    let (next, _) = gaze_position(&mut player);
    assert!(next > x, "the chase should continue, {next} vs {x}");
}

/// Spring overshoots before it settles, which is the whole reason to pick it.
#[test]
fn a_spring_target_overshoots_before_settling() {
    let mut player = KinetoPlayer::load(follow_doc(
        TargetFollowScope::Anywhere,
        TargetFollowFeel::Spring,
    ))
    .expect("load");
    player.play().expect("frame");

    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerDown, 200.0, 100.0)
        .expect("pointer");

    let mut furthest: f64 = 0.0;
    for _ in 0..120 {
        let (x, _) = gaze_position(&mut player);
        furthest = furthest.max(x);
    }
    assert!(
        furthest > 200.0,
        "a spring should pass its target before settling, reached {furthest}"
    );
}

/// A document with no following target must not pay for the machinery.
#[test]
fn a_target_that_does_not_follow_stays_where_it_was_authored() {
    let mut player = KinetoPlayer::load(follow_doc(
        TargetFollowScope::Anywhere,
        TargetFollowFeel::Snap,
    ))
    .expect("load");
    player.play().expect("frame");

    // No pointer event at all.
    assert_eq!(gaze_position(&mut player), (100.0, 100.0));
}

/// A drag has to keep updating a following target, not only the tap that
/// started it. There is no Move listener in the editor, so a move is delivered
/// as continuous input and fires nothing (issue #111).
#[test]
fn a_pointer_move_keeps_aiming_a_following_target() {
    let mut player = KinetoPlayer::load(follow_doc(
        TargetFollowScope::Anywhere,
        TargetFollowFeel::Snap,
    ))
    .expect("load");
    player.play().expect("frame");

    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerDown, 200.0, 200.0)
        .expect("pointer");
    assert_eq!(gaze_position(&mut player), (200.0, 200.0));

    // The finger travels without lifting.
    player.pointer_moved(340.0, 90.0).expect("move");
    assert_eq!(gaze_position(&mut player), (340.0, 90.0));
}

/// The renderer fits the document into the drawable, so a drawable pixel is
/// not a document unit. A host that hit tests has to come back through that
/// fit — and when it did not, every touch landed where the artwork was not.
#[test]
fn a_drawable_point_maps_back_through_the_viewport_fit() {
    let player = KinetoPlayer::load(follow_doc(
        TargetFollowScope::Anywhere,
        TargetFollowFeel::Snap,
    ))
    .expect("load");

    // The fixture's canvas is 400x400. In an 800x1000 drawable, contain-fit
    // scales by 2 (the tighter axis) and centres the leftover 200px of height.
    let centre = player.document_point(800.0, 1000.0, 400.0, 500.0).expect("point");
    assert!((centre.0 - 200.0).abs() < 1e-6, "got {centre:?}");
    assert!((centre.1 - 200.0).abs() < 1e-6, "got {centre:?}");

    // The document's own origin sits below the letterbox, not at the corner.
    let origin = player.document_point(800.0, 1000.0, 0.0, 100.0).expect("point");
    assert!(origin.0.abs() < 1e-6 && origin.1.abs() < 1e-6, "got {origin:?}");
}

/// A surface that has not been laid out yet must not divide by zero; the 1:1
/// mapping is what was correct before there was a fit at all.
#[test]
fn a_degenerate_drawable_maps_one_to_one() {
    let player = KinetoPlayer::load(follow_doc(
        TargetFollowScope::Anywhere,
        TargetFollowFeel::Snap,
    ))
    .expect("load");

    assert_eq!(player.document_point(0.0, 0.0, 42.0, 7.0).expect("point"), (42.0, 7.0));
}

/// A lifted finger releases every following target at once, back to where the
/// document put it — `TargetFollowScope` has no reading for "keep chasing the
/// last spot" (`preview_pointer_drive.dart:124`).
///
/// Holding the last position instead is what made the animation look frozen
/// after a tap: a following target wins over its keyframes, so one left parked
/// there never moved again (issue #111).
#[test]
fn lifting_the_finger_releases_a_following_target() {
    let mut player = KinetoPlayer::load(follow_doc(
        TargetFollowScope::Anywhere,
        TargetFollowFeel::Snap,
    ))
    .expect("load");
    player.play().expect("frame");

    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerDown, 300.0, 40.0)
        .expect("pointer");
    assert_eq!(gaze_position(&mut player), (300.0, 40.0));

    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerUp, 300.0, 40.0)
        .expect("pointer");

    assert_eq!(
        gaze_position(&mut player),
        (100.0, 100.0),
        "the target should be back where the document put it"
    );
}

/// And a release must not leave the spring carrying velocity into the next
/// gesture, which would fling the target off the finger.
#[test]
fn a_release_clears_the_springs_velocity() {
    let mut player = KinetoPlayer::load(follow_doc(
        TargetFollowScope::Anywhere,
        TargetFollowFeel::Spring,
    ))
    .expect("load");
    player.play().expect("frame");

    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerDown, 380.0, 100.0)
        .expect("pointer");
    for _ in 0..10 {
        gaze_position(&mut player);
    }
    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerUp, 380.0, 100.0)
        .expect("pointer");
    assert_eq!(gaze_position(&mut player), (100.0, 100.0));

    // A fresh press starts from the authored position, not mid-flight.
    player
        .pointer_event(kineto_engine::format::ListenerEventKind::PointerDown, 110.0, 100.0)
        .expect("pointer");
    let (x, _) = gaze_position(&mut player);
    assert!(
        x > 99.0 && x < 115.0,
        "the new chase should start from rest, got {x}"
    );
}
