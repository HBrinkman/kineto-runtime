//! Cross-runtime geometry parity (issue #111).
//!
//! The editor writes down the cubic segments it believes a path has, in
//! `test/fixtures/geometry/path_segments.txt`; this reads the same `.kvk`,
//! asks the runtime for its own answer, and requires the two to agree.
//!
//! Four things read a Kineto path — the editor's Dart and Metal renderers, the
//! web runtime and this one — and for months one of them treated Bézier
//! handles as absolute points while the other three added them to their
//! anchor. Nothing compared them, so the disagreement only surfaced as spikes
//! on a physical iPad. This is that comparison.

use kineto_engine::player::KinetoPlayer;
use kineto_engine::render::{build_render_scene, path_segments, RenderShape};

const SEGMENTS: &str = include_str!("../../../test/fixtures/geometry/path_segments.txt");

#[derive(Debug, PartialEq)]
struct ExpectedSegment {
    start: (f64, f64),
    control1: (f64, f64),
    control2: (f64, f64),
    end: (f64, f64),
}

#[derive(Debug)]
struct ExpectedNode {
    id: String,
    closed: bool,
    segments: Vec<ExpectedSegment>,
}

fn parse_expectations() -> Vec<ExpectedNode> {
    let mut nodes: Vec<ExpectedNode> = Vec::new();
    for line in SEGMENTS.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line == "end" {
            continue;
        }
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("node") => nodes.push(ExpectedNode {
                id: parts.next().expect("node id").to_owned(),
                closed: false,
                segments: Vec::new(),
            }),
            Some("contour") => {
                let _index = parts.next();
                let closed = parts.next() == Some("closed");
                nodes.last_mut().expect("contour before node").closed = closed;
            }
            Some("segment") => {
                let numbers: Vec<f64> = parts
                    .map(|value| value.parse().expect("segment number"))
                    .collect();
                assert_eq!(numbers.len(), 8, "a cubic is eight numbers");
                nodes
                    .last_mut()
                    .expect("segment before node")
                    .segments
                    .push(ExpectedSegment {
                        start: (numbers[0], numbers[1]),
                        control1: (numbers[2], numbers[3]),
                        control2: (numbers[4], numbers[5]),
                        end: (numbers[6], numbers[7]),
                    });
            }
            other => panic!("unexpected line {other:?}"),
        }
    }
    nodes
}

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
}

#[test]
fn the_runtime_reads_the_same_curves_the_editor_wrote() {
    let bytes = include_bytes!("fixtures/path_parity.kvk");
    let mut player = KinetoPlayer::load(bytes.to_vec()).unwrap();
    let frame = player.advance(0.0).unwrap();
    let document = kineto_engine::format::load(bytes).unwrap();
    let scene = build_render_scene(document, &frame);

    let expected = parse_expectations();
    assert!(!expected.is_empty(), "the fixture table is empty");

    for node in &expected {
        let command = scene
            .commands
            .iter()
            .find(|command| command.node_id == node.id)
            .unwrap_or_else(|| panic!("the runtime drew nothing for {}", node.id));

        let RenderShape::Path { contours, .. } = &command.shape else {
            panic!("{} is not a path in the runtime's scene", node.id);
        };

        let decomposed = path_segments(contours);
        assert_eq!(
            decomposed.len(),
            1,
            "{} should have one contour",
            node.id
        );
        let contour = &decomposed[0];
        assert_eq!(contour.closed, node.closed, "{} closedness", node.id);
        assert_eq!(
            contour.segments.len(),
            node.segments.len(),
            "{} segment count — a reader that drops the closing segment shows up here",
            node.id
        );

        for (index, (actual, want)) in contour.segments.iter().zip(&node.segments).enumerate() {
            assert!(
                close(actual.start, want.start)
                    && close(actual.control1, want.control1)
                    && close(actual.control2, want.control2)
                    && close(actual.end, want.end),
                "{} segment {index}: runtime {actual:?} != editor {want:?}",
                node.id
            );
        }
    }
}
