//! The value-level evaluation seam (issue #96) — the runtime half.
//!
//! Mirrors `lib/src/domain/animation/blend_layers.dart` rule for rule. Both
//! sides are checked against the same fixture,
//! `test/fixtures/blend/blend_cases.txt`, so the editor's preview and the
//! runtime cannot drift apart: an exported file must play back as the pose
//! the author saw.
//!
//! Ordered semantics ([`blend_ordered`]) apply samples in list order onto an
//! accumulator seeded from the rest pose:
//!
//! ```text
//! acc = base
//! for each participating sample:
//!   acc = lerp(acc, sample.value, sample.weight)
//! ```
//!
//! which yields one sample at weight 1 unchanged, later-wins priority for two
//! samples at weight 1, and an exact `lerp(from, to, p)` for the transition
//! crossfade shape `[from @ 1, to @ p]`.
//!
//! Normalised semantics ([`mix_samples`]) are the blend-tree case: weights
//! that already partition 1, folded as a running weighted mean.
//!
//! Sparse participation, rest-pose seeding, the discrete winner threshold and
//! the kind-conflict rule are documented on the Dart side and implemented
//! identically here. Path data is Dart-only — the runtime's `PropertyValue`
//! carries scalars and colours — so the path rule has no counterpart yet.

use std::collections::{HashMap, HashSet};

use super::timeline_eval::PropertyValue;

/// Property keys whose value is an index, not an amount. Interpolating one
/// would land between two discrete states, so they switch outright instead.
const DISCRETE_KEYS: [&str; 3] = ["drawOrder", "soloIndex", "blendMode"];

/// The weight at which a winner-takes-all property flips to the incoming
/// value. Must stay in step with `kDiscreteWinnerThreshold` in Dart.
pub const DISCRETE_WINNER_THRESHOLD: f64 = 0.5;

/// One sampled pose entering the fold.
#[derive(Debug, Clone)]
pub struct LayerSample {
    /// Sparse: only the properties this sample actually animates, keyed by
    /// subject id and then by the same property-key strings the Dart override
    /// maps use.
    pub overrides: HashMap<String, HashMap<String, PropertyValue>>,
    /// How much of this sample reaches the result, clamped to `0..=1`.
    pub weight: f64,
    /// A disabled sample is skipped entirely: it contributes nothing and does
    /// not seed a property.
    pub enabled: bool,
}

impl LayerSample {
    pub fn new(overrides: HashMap<String, HashMap<String, PropertyValue>>) -> Self {
        Self {
            overrides,
            weight: 1.0,
            enabled: true,
        }
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    fn effective_weight(&self) -> f64 {
        if self.enabled {
            self.weight.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn participates(&self) -> bool {
        self.enabled && self.weight > 0.0 && !self.overrides.is_empty()
    }
}

/// The rest-pose value for a property, so a partially weighted sample has
/// something to blend away from. `None` means "no base", in which case the
/// first participating sample seeds the accumulator at full strength.
pub type BaseValues = HashMap<String, HashMap<String, PropertyValue>>;

/// Folds `samples` with the ordered priority semantics described above.
pub fn blend_ordered(
    samples: &[LayerSample],
    base: Option<&BaseValues>,
) -> HashMap<String, HashMap<String, PropertyValue>> {
    fold(samples, base, false)
}

/// Folds `samples` as a weighted mean — the blend-tree case.
pub fn mix_samples(samples: &[LayerSample]) -> HashMap<String, HashMap<String, PropertyValue>> {
    fold(samples, None, true)
}

fn fold(
    samples: &[LayerSample],
    base: Option<&BaseValues>,
    normalise: bool,
) -> HashMap<String, HashMap<String, PropertyValue>> {
    let participating: Vec<&LayerSample> = samples
        .iter()
        .filter(|sample| sample.participates())
        .collect();
    if participating.is_empty() {
        return HashMap::new();
    }
    if participating.len() == 1 && (normalise || participating[0].effective_weight() >= 1.0) {
        return participating[0].overrides.clone();
    }

    // Weight per sample, resolved once. In normalised mode a sample's share is
    // its weight over the running total, which turns the same lerp chain into
    // an incremental weighted mean.
    let mut weights = Vec::with_capacity(participating.len());
    let mut running_total = 0.0;
    for sample in &participating {
        let w = sample.effective_weight();
        if normalise {
            running_total += w;
            weights.push(if running_total > 0.0 {
                w / running_total
            } else {
                0.0
            });
        } else {
            weights.push(w);
        }
    }

    // One accumulator for the whole fold, written in place rather than copied
    // per sample.
    let mut accumulator: HashMap<String, HashMap<String, PropertyValue>> = HashMap::new();
    // Which (subject, property) pairs have been seeded — a property is only
    // blended against the rest pose the first time a sample produces it.
    let mut seeded: HashMap<String, HashSet<String>> = HashMap::new();

    for (index, sample) in participating.iter().enumerate() {
        let weight = weights[index];
        if weight <= 0.0 {
            continue;
        }
        for (subject_id, values) in &sample.overrides {
            let target = accumulator.entry(subject_id.clone()).or_default();
            let seeded_keys = seeded.entry(subject_id.clone()).or_default();
            for (key, value) in values {
                if seeded_keys.insert(key.clone()) {
                    let base_value = base
                        .and_then(|base| base.get(subject_id))
                        .and_then(|values| values.get(key));
                    // No base to blend away from: the first contribution
                    // lands whole.
                    let seed = match base_value {
                        Some(current) => blend_value(current, value, weight, key),
                        None => value.clone(),
                    };
                    target.insert(key.clone(), seed);
                } else {
                    let current = target[key].clone();
                    target.insert(key.clone(), blend_value(&current, value, weight, key));
                }
            }
        }
    }

    accumulator
}

/// Blends `incoming` over `current` by `weight`. `key` decides only whether
/// the property is discrete.
fn blend_value(
    current: &PropertyValue,
    incoming: &PropertyValue,
    weight: f64,
    key: &str,
) -> PropertyValue {
    if weight >= 1.0 {
        return incoming.clone();
    }
    if DISCRETE_KEYS.contains(&key) {
        return if weight >= DISCRETE_WINNER_THRESHOLD {
            incoming.clone()
        } else {
            current.clone()
        };
    }
    match (current, incoming) {
        (PropertyValue::Scalar(a), PropertyValue::Scalar(b)) => {
            PropertyValue::Scalar(a + (b - a) * weight)
        }
        (PropertyValue::Color(a), PropertyValue::Color(b)) => {
            PropertyValue::Color(lerp_argb(*a, *b, weight))
        }
        // Vertex for vertex, and only when the counts agree — the same rule
        // the editor's morph interpolation uses (`timeline_eval.dart:106`).
        // Two different shapes have no meaningful midpoint.
        (PropertyValue::Path(a), PropertyValue::Path(b)) if a.len() == b.len() => {
            PropertyValue::Path(
                a.iter()
                    .zip(b)
                    .map(|(from, to)| lerp_vertex(from, to, weight))
                    .collect(),
            )
        }
        // Different kinds: nothing sensible to interpolate, so the same
        // winner-takes-all rule the discrete properties use.
        _ => {
            if weight >= DISCRETE_WINNER_THRESHOLD {
                incoming.clone()
            } else {
                current.clone()
            }
        }
    }
}

/// Channel-wise ARGB interpolation, matching Flutter's `Color.lerp`.
fn lerp_argb(a: u32, b: u32, t: f64) -> u32 {
    let channel = |shift: u32| {
        let from = ((a >> shift) & 0xFF) as f64;
        let to = ((b >> shift) & 0xFF) as f64;
        let value = from + (to - from) * t;
        (value.round().clamp(0.0, 255.0) as u32) << shift
    };
    channel(24) | channel(16) | channel(8) | channel(0)
}

/// Interpolates one morph vertex, anchor and both handles.
fn lerp_vertex(
    from: &crate::animation::timeline_eval::MorphVertex,
    to: &crate::animation::timeline_eval::MorphVertex,
    weight: f64,
) -> crate::animation::timeline_eval::MorphVertex {
    let lerp = |a: (f64, f64), b: (f64, f64)| {
        (a.0 + (b.0 - a.0) * weight, a.1 + (b.1 - a.1) * weight)
    };
    crate::animation::timeline_eval::MorphVertex {
        anchor: lerp(from.anchor, to.anchor),
        in_handle: lerp(from.in_handle, to.in_handle),
        out_handle: lerp(from.out_handle, to.out_handle),
    }
}
