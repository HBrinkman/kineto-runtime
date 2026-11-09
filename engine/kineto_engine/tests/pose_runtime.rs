//! Cross-runtime pose parity (issue #214): Swap Groups, a stack that changes
//! over time and a keyed joint, frame by frame.
//!
//! `pose_runtime.kvk` is a synthetic puppet the editor's exporter wrote: a
//! component instance whose turn swaps an arm's drawing, sends it behind the
//! torso and moves the shoulder joint (carrying an attached hand and a skinned
//! strap), plus a scene-level Swap Group and a scene-level restack. The table
//! beside it is the editor's own answer per frame, from
//! `test/tool/pose_runtime_fixture_test.dart`; that test also holds the
//! editor's video export renderer to the same answer pixel by pixel.

use std::collections::HashMap;

use kineto_engine::animation::{NodeOverrides, PropertyValue};
use kineto_engine::format;
use kineto_engine::hierarchy::{apply_point, multiply, Affine};
use kineto_engine::player::{EvaluatedFrame, KinetoPlayer};
use kineto_engine::render::{
    build_render_scene, sorted_siblings, RenderCommand, RenderShape, RenderTransform,
};

const FIXTURE: &[u8] = include_bytes!("fixtures/pose_runtime.kvk");
const EXPECTED: &str = include_str!("../../../test/fixtures/pose/pose_runtime_frames.txt");

/// The table rounds to four places.
const TOLERANCE: f64 = 2e-3;

#[derive(Default)]
struct Frame {
    time: f64,
    order: Vec<String>,
    nodes: Vec<(String, Affine)>,
    vertices: Vec<(String, usize, f64, f64)>,
}

fn frames() -> Vec<Frame> {
    let mut out: Vec<Frame> = Vec::new();
    for line in EXPECTED.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let number = |i: usize| -> f64 { parts[i].parse().expect("number") };
        match parts[0] {
            "frame" => out.push(Frame {
                time: number(1),
                ..Frame::default()
            }),
            "order" => {
                out.last_mut().expect("frame").order =
                    parts[1..].iter().map(|s| s.to_string()).collect()
            }
            "node" => out.last_mut().expect("frame").nodes.push((
                parts[1].to_owned(),
                [number(2), number(3), number(4), number(5), number(6), number(7)],
            )),
            "vertex" => out.last_mut().expect("frame").vertices.push((
                parts[1].to_owned(),
                parts[2].parse().expect("index"),
                number(3),
                number(4),
            )),
            other => panic!("unknown table line {other}"),
        }
    }
    out
}

fn transform_affine(t: RenderTransform) -> Affine {
    let (sin, cos) = t.rotation.sin_cos();
    let to_anchor = [1.0, 0.0, 0.0, 1.0, t.x + t.anchor_x, t.y + t.anchor_y];
    let rotate_scale = [
        cos * t.scale_x,
        sin * t.scale_x,
        -sin * t.scale_y,
        cos * t.scale_y,
        0.0,
        0.0,
    ];
    let from_anchor = [1.0, 0.0, 0.0, 1.0, -t.anchor_x, -t.anchor_y];
    multiply(multiply(to_anchor, rotate_scale), from_anchor)
}

fn world(command: &RenderCommand) -> Affine {
    multiply(command.parent, transform_affine(command.transform))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < TOLERANCE
}

fn render_at(player: &mut KinetoPlayer, time: f64) -> Vec<RenderCommand> {
    let frame = player.seek(time).expect("frame should evaluate");
    let document = format::load(FIXTURE).expect("document should parse");
    build_render_scene(document, &frame).commands
}

#[test]
fn the_runtime_draws_every_frame_the_way_the_editor_does() {
    let mut player = KinetoPlayer::load(FIXTURE.to_vec()).expect("player should load");
    player.play().expect("play");
    let frames = frames();
    assert_eq!(frames.len(), 11, "one entry per frame of the clip");

    for expected in &frames {
        let t = expected.time;
        let commands = render_at(&mut player, t);
        let order: Vec<&str> = commands.iter().map(|c| c.node_id.as_str()).collect();
        assert_eq!(
            order, expected.order,
            "frame {t}: what draws, back to front (visibility and draw order)"
        );

        let by_id: HashMap<&str, &RenderCommand> =
            commands.iter().map(|c| (c.node_id.as_str(), c)).collect();
        for (id, want) in &expected.nodes {
            let got = world(by_id[id.as_str()]);
            assert!(
                (0..6).all(|i| close(got[i], want[i])),
                "frame {t}: {id} at {got:?}, the editor has {want:?}"
            );
        }
        for (id, index, x, y) in &expected.vertices {
            let command = by_id[id.as_str()];
            let RenderShape::Path { contours, .. } = &command.shape else {
                panic!("{id} should draw as a path");
            };
            let point = contours[0].points[*index];
            let (gx, gy) = apply_point(world(command), point.anchor_x, point.anchor_y);
            assert!(
                close(gx, *x) && close(gy, *y),
                "frame {t}: {id} vertex {index} at ({gx}, {gy}), the editor has ({x}, {y})"
            );
        }
    }
}

#[test]
fn the_fixture_exercises_all_three_features() {
    // A table where nothing changes would be matched by a runtime that does
    // nothing at all.
    let frames = frames();
    let (first, last) = (&frames[0], &frames[frames.len() - 1]);
    assert!(first.order.contains(&"p/armFront".to_owned()));
    assert!(last.order.contains(&"p/armSide".to_owned()));
    assert!(!last.order.contains(&"p/armFront".to_owned()));
    let position = |f: &Frame, id: &str| f.order.iter().position(|o| o == id).unwrap();
    assert!(position(first, "p/torso") < position(first, "p/armFront"));
    assert!(position(last, "p/armSide") < position(last, "p/torso"));
    assert_eq!(first.order.last().map(String::as_str), Some("badgeA"));
    assert_eq!(last.order.first().map(String::as_str), Some("badgeB"));
    let hand = |f: &Frame| f.nodes.iter().find(|(id, _)| id == "p/hand").unwrap().1;
    assert!((hand(first)[4] - hand(last)[4]).abs() > 20.0, "the joint carries the hand");
}

#[test]
fn the_file_declares_the_pose_features_and_a_swap_group() {
    let document = format::load(FIXTURE).expect("document should parse");
    assert_eq!(document.version(), 2);
    let arm = document
        .nodes()
        .unwrap()
        .iter()
        .find(|n| n.id() == "p/arm")
        .and_then(|n| n.component_as_group())
        .expect("the arm group");
    assert_eq!(arm.solo_child_id(), Some("p/armFront"));
}

#[test]
fn a_tap_lands_on_what_is_drawn_on_top() {
    // A probe inside both the torso and the arm: the arm is on top at Front,
    // the torso once Side sends the arm behind it.
    let frames = frames();
    let torso = frames[0].nodes.iter().find(|(id, _)| id == "p/torso").unwrap().1;
    let (x, y) = apply_point(torso, 35.0, 18.0);
    let mut player = KinetoPlayer::load(FIXTURE.to_vec()).expect("player should load");
    player.play().expect("play");
    player.seek(0.0).expect("seek");
    let front = player.hit_test_document_point(x, y).unwrap().map(|t| t.node_id);
    player.seek(1.0).expect("seek");
    let side = player.hit_test_document_point(x, y).unwrap().map(|t| t.node_id);
    assert_eq!(front.as_deref(), Some("p/armFront"));
    assert_eq!(side.as_deref(), Some("p/torso"));
}

fn frame_with(values: &[(&str, &'static str, f64)]) -> EvaluatedFrame {
    let mut overrides: HashMap<String, NodeOverrides> = HashMap::new();
    for (id, key, v) in values {
        overrides
            .entry((*id).to_owned())
            .or_default()
            .values
            .insert(key, PropertyValue::Scalar(*v));
    }
    EvaluatedFrame {
        current_time: 0.0,
        active_animation_id: None,
        node_overrides: overrides,
        fk: HashMap::new(),
        ik_targets: Vec::new(),
    }
}

#[test]
fn siblings_sort_by_position_with_unkeyed_ones_holding_theirs() {
    // `withPosedStackOrder`: a keyed sibling sorts as its rounded value, an
    // unkeyed one as its index, ties keep list order.
    let ids = vec!["a", "b", "c", "d"];
    assert_eq!(sorted_siblings(ids.clone(), &frame_with(&[])), ids);
    assert_eq!(
        sorted_siblings(ids.clone(), &frame_with(&[("d", "drawOrder", 0.0)])),
        vec!["a", "d", "b", "c"]
    );
    assert_eq!(
        sorted_siblings(ids.clone(), &frame_with(&[("a", "drawOrder", 2.6)])),
        vec!["b", "c", "a", "d"]
    );
    assert_eq!(
        sorted_siblings(ids, &frame_with(&[("c", "drawOrder", -1.0)])),
        vec!["c", "a", "b", "d"]
    );
}
