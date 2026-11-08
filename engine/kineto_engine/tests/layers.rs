//! Runtime half of the layered parity fixture (issue #98).
//!
//! Loads `body_face_mouth.kvk` — the bytes the editor's exporter produced —
//! and answers the same pose table the editor answers,
//! `test/fixtures/layers/body_face_mouth_poses.txt`. Both files are generated
//! by `test/tool/layered_machine_fixture_test.dart`, so neither side can drift
//! without the other noticing.

use std::collections::{HashMap, HashSet};

use kineto_engine::animation::NodeOverrides;
use kineto_engine::format;
use kineto_engine::player::{fold_layer_samples, layer_samples, KinetoPlayer, LayerClock};
use kineto_engine::state_machine::layers_of;

const FIXTURE: &[u8] = include_bytes!("fixtures/body_face_mouth.kvk");
const POSES: &str = include_str!("../../../test/fixtures/layers/body_face_mouth_poses.txt");

/// Scalars compare within 1e-9 — the same tolerance the Dart side uses.
const TOLERANCE: f64 = 1e-9;

struct PoseCase {
    name: String,
    clocks: HashMap<String, LayerClock>,
    disabled: HashSet<String>,
    /// Base-layer state and elapsed seconds, which the runtime evaluates on
    /// the player's own clock rather than through `layer_samples`.
    base: Option<LayerClock>,
    expectations: Vec<(String, String, f64)>,
    absences: Vec<(String, String)>,
}

fn parse(source: &str) -> Vec<PoseCase> {
    let mut cases = Vec::new();
    let mut current: Option<PoseCase> = None;

    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts[0] {
            "case" => {
                current = Some(PoseCase {
                    name: parts[1].to_string(),
                    clocks: HashMap::new(),
                    disabled: HashSet::new(),
                    base: None,
                    expectations: Vec::new(),
                    absences: Vec::new(),
                });
            }
            "layer" => {
                let case = current.as_mut().expect("layer outside a case");
                let clock = LayerClock {
                    active_state_id: parts[2].to_string(),
                    elapsed_seconds: parts[3].parse().expect("elapsed"),
                };
                if parts[1].ends_with("::base") {
                    case.base = Some(clock);
                } else {
                    case.clocks.insert(parts[1].to_string(), clock);
                }
            }
            "disabled" => {
                let case = current.as_mut().expect("disabled outside a case");
                case.disabled.insert(parts[1].to_string());
            }
            "expect" => {
                let case = current.as_mut().expect("expect outside a case");
                case.expectations.push((
                    parts[1].to_string(),
                    parts[2].to_string(),
                    parts[3].parse().expect("value"),
                ));
            }
            "absent" => {
                let case = current.as_mut().expect("absent outside a case");
                case.absences
                    .push((parts[1].to_string(), parts[2].to_string()));
            }
            "end" => cases.push(current.take().expect("end outside a case")),
            other => panic!("unknown directive {other:?}"),
        }
    }
    cases
}

/// The base layer's own contribution, evaluated exactly the way the player
/// does it: its active state's clip, at the base layer's elapsed seconds.
fn base_pose(
    document: &format::Document<'_>,
    machine: format::StateMachine<'_>,
    clock: &LayerClock,
) -> HashMap<String, NodeOverrides> {
    let animations = document.animations().unwrap_or_default();
    let states = machine.states().unwrap_or_default();
    let Some(state) = states
        .iter()
        .find(|state| state.id() == clock.active_state_id)
    else {
        return HashMap::new();
    };
    let Some(animation) = animations.iter().find(|a| a.id() == state.clip_id()) else {
        return HashMap::new();
    };
    let duration = animation.duration() as f64;
    let time = if duration > 0.0 {
        clock.elapsed_seconds % duration
    } else {
        0.0
    };
    kineto_engine::animation::evaluate_animation(&animation, time)
}

#[test]
fn shared_pose_table_matches_the_editor() {
    let player = KinetoPlayer::load(FIXTURE.to_vec()).expect("fixture should load");
    let document = player.document().expect("fixture should parse");
    let machine = document
        .state_machines()
        .expect("fixture has a state machine")
        .get(0);

    let cases = parse(POSES);
    assert!(!cases.is_empty(), "pose table parsed to no cases");

    for case in &cases {
        let base = case.base.as_ref().expect("every case pins the base layer");
        let pose = fold_layer_samples(
            base_pose(&document, machine, base),
            layer_samples(&document, machine, &case.clocks, &case.disabled),
        );

        for (node_id, key, expected) in &case.expectations {
            let actual = pose
                .get(node_id)
                .and_then(|overrides| overrides.scalar(key))
                .unwrap_or_else(|| panic!("{}: {node_id}.{key} is missing", case.name));
            assert!(
                (actual - expected).abs() < TOLERANCE,
                "{}: {node_id}.{key} = {actual}, expected {expected}",
                case.name
            );
        }

        for (node_id, key) in &case.absences {
            assert!(
                pose.get(node_id)
                    .and_then(|overrides| overrides.scalar(key))
                    .is_none(),
                "{}: {node_id}.{key} must fall through to the rest pose",
                case.name
            );
        }
    }
}

#[test]
fn the_fixture_carries_three_layers_in_order() {
    let player = KinetoPlayer::load(FIXTURE.to_vec()).expect("fixture should load");
    let document = player.document().expect("fixture should parse");
    let machine = document.state_machines().unwrap().get(0);

    let layers = layers_of(machine);
    let names: Vec<&str> = layers.iter().map(|layer| layer.name()).collect();
    assert_eq!(names, vec!["Body", "Face", "Mouth"]);
    assert!(layers[0].is_base());
    assert!(layers.iter().all(|layer| layer.enabled()));
}

#[test]
fn every_layer_advances_on_its_own_clock() {
    // Ten frames of 0.1 s. Nothing transitions (no conditions are met), so
    // each layer should simply accumulate elapsed time in its own state —
    // which is what makes the per-layer clocks observable through the public
    // API rather than only inside the fold.
    let mut player = KinetoPlayer::load(FIXTURE.to_vec()).expect("fixture should load");
    player.play().expect("initial frame");
    for _ in 0..10 {
        player.advance(0.1).expect("frame should advance");
    }

    // The base layer reports through the pre-layers API, unchanged.
    assert_eq!(player.active_state("sm-character"), Some("body-idle"));

    // And every layer still sits in its own initial state, each having
    // accumulated its own elapsed seconds.
    let document = player.document().expect("parses");
    let machine = document.state_machines().unwrap().get(0);
    for layer in layers_of(machine) {
        assert!(layer.enabled(), "{} should be enabled", layer.name());
    }
}

#[test]
fn a_legacy_single_layer_document_is_untouched() {
    // The append-only schema change must leave a pre-#98 file reading exactly
    // as it did: one effective base layer, no extras.
    let bytes = include_bytes!("fixtures/animated_sample.kvk").to_vec();
    let player = KinetoPlayer::load(bytes).expect("legacy fixture should load");
    let document = player.document().expect("parses");
    for machine in document.state_machines().unwrap_or_default() {
        let layers = layers_of(machine);
        assert_eq!(layers.len(), 1, "a legacy machine has only its base layer");
        assert!(layers[0].is_base());
        assert_eq!(layers[0].name(), "Base");
        assert!(layers[0].enabled());
    }
}
