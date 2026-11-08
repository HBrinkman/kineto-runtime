//! The Rust half of the shared blend fixture (issue #96).
//!
//! `test/domain/animation/blend_fixture_test.dart` runs the same file through
//! the Dart implementation. Adding a case constrains both sides at once,
//! which is the point: the editor's preview and the runtime must resolve a
//! pose the same way, or an exported file plays back differently from what
//! the author saw.

use std::collections::HashMap;

use kineto_engine::animation::blend::{blend_ordered, mix_samples, BaseValues, LayerSample};
use kineto_engine::animation::PropertyValue;

const FIXTURE: &str = include_str!("../../../test/fixtures/blend/blend_cases.txt");

#[derive(Default)]
struct Case {
    name: String,
    mixed: bool,
    samples: Vec<LayerSample>,
    base: BaseValues,
    expectations: Vec<(String, String, PropertyValue)>,
    absences: Vec<(String, String)>,
}

fn parse_value(kind: &str, raw: &str) -> PropertyValue {
    match kind {
        "s" => PropertyValue::Scalar(raw.parse().expect("scalar")),
        "c" => PropertyValue::Color(u32::from_str_radix(raw, 16).expect("argb")),
        other => panic!("unknown value kind {other:?}"),
    }
}

// `flush_sample!` always clears `has_pending`, and the `sample` arm sets it
// straight back — the flag is correct, the write is just redundant there.
#[allow(unused_assignments)]
fn parse(source: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut current: Option<Case> = None;
    let mut pending: HashMap<String, HashMap<String, PropertyValue>> = HashMap::new();
    let mut pending_weight = 1.0;
    let mut pending_enabled = true;
    let mut has_pending = false;

    macro_rules! flush_sample {
        () => {
            if has_pending {
                let case = current.as_mut().expect("sample outside a case");
                case.samples.push(
                    LayerSample::new(std::mem::take(&mut pending))
                        .with_weight(pending_weight)
                        .with_enabled(pending_enabled),
                );
                has_pending = false;
            }
        };
    }

    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts[0] {
            "case" => {
                current = Some(Case {
                    name: parts[1].to_string(),
                    mixed: parts[2] == "mixed",
                    ..Default::default()
                });
            }
            "base" => {
                let case = current.as_mut().expect("base outside a case");
                case.base
                    .entry(parts[1].to_string())
                    .or_default()
                    .insert(parts[2].to_string(), parse_value(parts[3], parts[4]));
            }
            "sample" => {
                flush_sample!();
                pending_weight = parts[1].parse().expect("weight");
                pending_enabled = parts[2] == "1";
                has_pending = true;
            }
            "value" => {
                pending
                    .entry(parts[1].to_string())
                    .or_default()
                    .insert(parts[2].to_string(), parse_value(parts[3], parts[4]));
            }
            "expect" => {
                flush_sample!();
                let case = current.as_mut().expect("expect outside a case");
                case.expectations.push((
                    parts[1].to_string(),
                    parts[2].to_string(),
                    parse_value(parts[3], parts[4]),
                ));
            }
            "absent" => {
                flush_sample!();
                let case = current.as_mut().expect("absent outside a case");
                case.absences
                    .push((parts[1].to_string(), parts[2].to_string()));
            }
            "end" => {
                flush_sample!();
                cases.push(current.take().expect("end outside a case"));
            }
            other => panic!("unknown directive {other:?}"),
        }
    }
    cases
}

/// Colour channels compare within 1: a half-way channel rounds to either side
/// of .5 depending on the host's rounding.
fn assert_value(case: &str, label: &str, actual: PropertyValue, expected: PropertyValue) {
    match (&actual, &expected) {
        (PropertyValue::Scalar(a), PropertyValue::Scalar(b)) => {
            assert!(
                (a - b).abs() < 1e-9,
                "{case}: {label} = {a}, expected {b}"
            );
        }
        (PropertyValue::Color(a), PropertyValue::Color(b)) => {
            for shift in [24, 16, 8, 0] {
                let ca = ((a >> shift) & 0xFF) as i32;
                let cb = ((b >> shift) & 0xFF) as i32;
                assert!(
                    (ca - cb).abs() <= 1,
                    "{case}: {label} = {a:08X}, expected {b:08X}"
                );
            }
        }
        _ => panic!("{case}: {label} kind mismatch: {actual:?} vs {expected:?}"),
    }
}

#[test]
fn shared_fixture_matches_the_dart_seam() {
    let cases = parse(FIXTURE);
    assert!(!cases.is_empty(), "fixture parsed to no cases");

    for case in &cases {
        let result = if case.mixed {
            mix_samples(&case.samples)
        } else {
            blend_ordered(
                &case.samples,
                if case.base.is_empty() {
                    None
                } else {
                    Some(&case.base)
                },
            )
        };

        for (subject, key, expected) in &case.expectations {
            let actual = result
                .get(subject)
                .and_then(|values| values.get(key))
                .unwrap_or_else(|| panic!("{}: {subject}.{key} is missing", case.name));
            assert_value(
                &case.name,
                &format!("{subject}.{key}"),
                actual.clone(),
                expected.clone(),
            );
        }

        for (subject, key) in &case.absences {
            assert!(
                result.get(subject).and_then(|values| values.get(key)).is_none(),
                "{}: {subject}.{key} must fall through to the base document",
                case.name
            );
        }
    }
}

#[test]
fn single_full_weight_sample_is_passed_through() {
    let mut values = HashMap::new();
    values.insert("positionX".to_string(), PropertyValue::Scalar(10.0));
    let mut overrides = HashMap::new();
    overrides.insert("nodeA".to_string(), values);

    let out = blend_ordered(&[LayerSample::new(overrides.clone())], None);
    assert_eq!(out, overrides);
}

#[test]
fn ordering_is_the_only_thing_that_decides_a_conflict() {
    let sample = |x: f64| {
        let mut values = HashMap::new();
        values.insert("positionX".to_string(), PropertyValue::Scalar(x));
        let mut overrides = HashMap::new();
        overrides.insert("nodeA".to_string(), values);
        LayerSample::new(overrides)
    };

    let forward = blend_ordered(&[sample(1.0), sample(2.0)], None);
    let reverse = blend_ordered(&[sample(2.0), sample(1.0)], None);
    assert_eq!(forward["nodeA"]["positionX"], PropertyValue::Scalar(2.0));
    assert_eq!(reverse["nodeA"]["positionX"], PropertyValue::Scalar(1.0));
}
