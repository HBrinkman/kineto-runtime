//! Number inputs driving properties, the runtime half of the editor's
//! `input_binding_eval.dart`.
//!
//! A binding is a live keyframe: the same subject and the same property a clip
//! would animate, with the value arriving from an input instead of a track.
//! It therefore produces the very same `NodeOverrides` a clip produces, merged
//! *over* them — so a bound property beats a keyframed one on the same
//! property, and everything downstream (skeletal solve, render, FFI) needs to
//! know nothing about bindings at all.

use std::collections::HashMap;

use crate::animation::{property_key, NodeOverrides, PropertyValue};
use crate::format::{self, AnimatedPropertyKind};

/// Merges the bindings' values over `base`.
///
/// An input with no value yet is skipped rather than read as zero: "not driven
/// yet" and "driven to the bottom of its range" are different states, and only
/// the second may move anything.
pub fn apply_bindings(
    document: &format::Document<'_>,
    number_values: &HashMap<String, f64>,
    base: &mut HashMap<String, NodeOverrides>,
) {
    let bindings = document.input_bindings().unwrap_or_default();
    if bindings.is_empty() || number_values.is_empty() {
        return;
    }

    let targets = document.ik_targets().unwrap_or_default();

    for binding in bindings.iter() {
        let Some(value) = number_values.get(binding.input_id()) else {
            continue;
        };
        let target = targets
            .iter()
            .find(|target| target.id() == binding.subject_id());
        let Some(resolved) = resolve(&binding, *value, target.as_ref()) else {
            continue;
        };
        base.entry(binding.subject_id().to_owned())
            .or_default()
            .values
            .insert(property_key(binding.property()), PropertyValue::Scalar(resolved));
    }
}

/// `None` when the binding cannot produce a value: a spatial binding on a
/// target that is still a point has nowhere to travel, which is what makes
/// collapsing an extent a way to park a binding rather than destroy it.
fn resolve(
    binding: &format::InputBinding<'_>,
    value: f64,
    target: Option<&format::IkTarget<'_>>,
) -> Option<f64> {
    let t = if value.is_nan() { 0.0 } else { value.clamp(0.0, 1.0) };
    let travel = if binding.invert() { 1.0 - t } else { t };

    if binding.has_mapping() {
        let min = binding.mapping_min() as f64;
        let max = binding.mapping_max() as f64;
        return Some(min + (max - min) * travel);
    }

    // No mapping: the subject owns the range. Only a target's extent does today.
    let target = target?;
    let (min, max) = match binding.property() {
        AnimatedPropertyKind::IkTargetPositionX => (
            target.position_x() as f64 - target.extent_w() as f64 / 2.0,
            target.position_x() as f64 + target.extent_w() as f64 / 2.0,
        ),
        AnimatedPropertyKind::IkTargetPositionY => (
            target.position_y() as f64 - target.extent_h() as f64 / 2.0,
            target.position_y() as f64 + target.extent_h() as f64 / 2.0,
        ),
        _ => return None,
    };
    if min == max {
        return None;
    }
    Some(min + (max - min) * travel)
}
