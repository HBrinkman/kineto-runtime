//! Cross-runtime constraint parity (issue #111).
//!
//! The editor solves constraints — an iris that follows the target a timeline
//! animates — and writes the resulting node transforms into
//! `test/fixtures/constraints/shape_constraints.txt`. This reads the same
//! `.kvk`, runs the runtime's own solver, and requires the two to land in the
//! same place.
//!
//! It exists because the alternative has already happened once: four readings
//! of one Bézier rule, three of them agreeing, and the disagreement only
//! visible on a device.

use std::collections::HashMap;

use kineto_engine::bones::constraints::solve_shape_constraints;

const EXPECTED: &str = include_str!("../../../test/fixtures/constraints/shape_constraints.txt");

struct Expectation {
    id: String,
    x: f64,
    y: f64,
    rotation: f64,
    scale_x: f64,
    scale_y: f64,
}

fn parse() -> (HashMap<String, (f64, f64)>, Vec<Expectation>) {
    let mut targets = HashMap::new();
    let mut nodes = Vec::new();
    for line in EXPECTED.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts[0] {
            "target" => {
                targets.insert(
                    parts[1].to_owned(),
                    (parts[2].parse().unwrap(), parts[3].parse().unwrap()),
                );
            }
            "node" => nodes.push(Expectation {
                id: parts[1].to_owned(),
                x: parts[2].parse().unwrap(),
                y: parts[3].parse().unwrap(),
                rotation: parts[4].parse().unwrap(),
                scale_x: parts[5].parse().unwrap(),
                scale_y: parts[6].parse().unwrap(),
            }),
            other => panic!("unexpected line kind {other}"),
        }
    }
    (targets, nodes)
}

#[test]
fn the_runtime_poses_constrained_shapes_where_the_editor_does() {
    let bytes = include_bytes!("fixtures/constraint_parity.kvk");
    let document = kineto_engine::format::load(bytes).unwrap();
    let (targets, expected) = parse();
    assert!(!expected.is_empty(), "the fixture table is empty");

    let solved = solve_shape_constraints(&document, &targets, &HashMap::new());

    for want in &expected {
        let got = solved
            .get(&want.id)
            .unwrap_or_else(|| panic!("the runtime posed nothing for {}", want.id));

        // A tenth of a pixel: tighter than anything an eye can see, loose
        // enough for f32 storage in the package.
        let tolerance = 0.1;
        assert!(
            (got.x - want.x).abs() < tolerance && (got.y - want.y).abs() < tolerance,
            "{}: runtime ({:.4}, {:.4}) != editor ({:.4}, {:.4})",
            want.id,
            got.x,
            got.y,
            want.x,
            want.y
        );
        assert!(
            (got.rotation - want.rotation).abs() < 1e-3,
            "{}: rotation {:.4} != {:.4}",
            want.id,
            got.rotation,
            want.rotation
        );
        assert!(
            (got.scale_x - want.scale_x).abs() < 1e-3
                && (got.scale_y - want.scale_y).abs() < 1e-3,
            "{}: scale ({:.4}, {:.4}) != ({:.4}, {:.4})",
            want.id,
            got.scale_x,
            got.scale_y,
            want.scale_x,
            want.scale_y
        );
    }
}

/// `constraintStrength` and `constraintOffset` used to export as
/// `BoneRotation`, so animating a constraint rotated a bone instead. They have
/// kinds of their own now, and the solver reads them off the constrained node.
#[test]
fn a_constraint_strength_track_weakens_the_constraint() {
    use kineto_engine::animation::timeline_eval::PropertyValue;
    use kineto_engine::animation::NodeOverrides;

    let bytes = include_bytes!("fixtures/constraint_parity.kvk");
    let document = kineto_engine::format::load(bytes).unwrap();
    let (targets, _) = parse();

    let full = solve_shape_constraints(&document, &targets, &HashMap::new());

    let mut weakened: HashMap<String, NodeOverrides> = HashMap::new();
    weakened
        .entry("iris-loose".to_string())
        .or_default()
        .values
        .insert("constraintStrength", PropertyValue::Scalar(0.0));
    let none = solve_shape_constraints(&document, &targets, &weakened);

    let full_x = full.get("iris-loose").expect("solved").x;
    let none_x = none.get("iris-loose").expect("solved").x;
    assert!(
        (full_x - none_x).abs() > 1e-6,
        "strength 0 should leave the shape where it was: {full_x} vs {none_x}"
    );
    // Strength 0 means "do not move me at all", which is the authored x.
    assert!(
        (none_x - 120.0).abs() < 0.1,
        "expected the authored position, got {none_x}"
    );
}
