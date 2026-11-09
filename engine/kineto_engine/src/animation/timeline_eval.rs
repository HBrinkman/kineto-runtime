use std::collections::HashMap;

use super::easing::{auto_bezier_ease, cubic_bezier_ease};
use crate::format::{AnimatedPropertyKind, Animation, KeyframeInterpolationKind};

/// Animated value: a scalar, an ARGB-packed colour, or a path morph.
///
/// The morph is one complete vertex snapshot, the way the editor stores it
/// (`KeyframeValue.path`). It used to have no representation here at all: a
/// `pathData` track exported as a `PositionX` of 0.0, so a morphing shape
/// teleported to x=0 instead of changing shape (issue #111).
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Scalar(f64),
    Color(u32),
    Path(Vec<MorphVertex>),
}

/// One vertex of a path morph, in the node's own space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MorphVertex {
    pub anchor: (f64, f64),
    pub in_handle: (f64, f64),
    pub out_handle: (f64, f64),
}

/// Per-node animated overrides at a specific playhead time.
#[derive(Debug, Default)]
pub struct NodeOverrides {
    pub values: HashMap<&'static str, PropertyValue>,
}

impl NodeOverrides {
    pub fn scalar(&self, key: &str) -> Option<f64> {
        match self.values.get(key) {
            Some(PropertyValue::Scalar(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn color(&self, key: &str) -> Option<u32> {
        match self.values.get(key) {
            Some(PropertyValue::Color(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn path(&self, key: &str) -> Option<&[MorphVertex]> {
        match self.values.get(key) {
            Some(PropertyValue::Path(vertices)) => Some(vertices),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Every key `property_key` can produce, so a key that has been through a
/// `String` (the layer blend seam works on owned keys) can be mapped back to
/// the `&'static str` `NodeOverrides` stores (issue #98).
pub const PROPERTY_KEYS: [&str; 40] = [
    "positionX",
    "positionY",
    "rotation",
    "scaleX",
    "scaleY",
    "anchorX",
    "anchorY",
    "opacity",
    "fillColor",
    "strokeColor",
    "strokeWidth",
    "skewX",
    "skewY",
    "width",
    "height",
    "cornerRadius",
    "fontSize",
    "letterSpacing",
    "lineHeight",
    "boneRotation",
    "ikTargetPositionX",
    "ikTargetPositionY",
    "boneLength",
    "constraintStrength",
    "constraintOffset",
    "gradientStopColor",
    "gradientStopOpacity",
    "gradientStopOffset",
    "cameraX",
    "cameraY",
    "cameraZoom",
    "cameraRotation",
    "pathData",
    "strokeTrimStart",
    "strokeTrimEnd",
    "strokeTrimOffset",
    "soloIndex",
    "drawOrder",
    "bonePositionX",
    "bonePositionY",
];

/// The `&'static str` form of `key`, or `None` when it is not a property key
/// this runtime knows.
pub fn static_property_key(key: &str) -> Option<&'static str> {
    PROPERTY_KEYS
        .iter()
        .find(|candidate| **candidate == key)
        .copied()
}

/// Map `AnimatedPropertyKind` to its camelCase string key (matches Dart `AnimatedProperty.name`).
pub fn property_key(kind: AnimatedPropertyKind) -> &'static str {
    match kind {
        AnimatedPropertyKind::PositionX => "positionX",
        AnimatedPropertyKind::PositionY => "positionY",
        AnimatedPropertyKind::Rotation => "rotation",
        AnimatedPropertyKind::ScaleX => "scaleX",
        AnimatedPropertyKind::ScaleY => "scaleY",
        AnimatedPropertyKind::AnchorX => "anchorX",
        AnimatedPropertyKind::AnchorY => "anchorY",
        AnimatedPropertyKind::Opacity => "opacity",
        AnimatedPropertyKind::FillColor => "fillColor",
        AnimatedPropertyKind::StrokeColor => "strokeColor",
        AnimatedPropertyKind::StrokeWidth => "strokeWidth",
        AnimatedPropertyKind::SkewX => "skewX",
        AnimatedPropertyKind::SkewY => "skewY",
        AnimatedPropertyKind::Width => "width",
        AnimatedPropertyKind::Height => "height",
        AnimatedPropertyKind::CornerRadius => "cornerRadius",
        AnimatedPropertyKind::FontSize => "fontSize",
        AnimatedPropertyKind::LetterSpacing => "letterSpacing",
        AnimatedPropertyKind::LineHeight => "lineHeight",
        AnimatedPropertyKind::BoneRotation => "boneRotation",
        AnimatedPropertyKind::IkTargetPositionX => "ikTargetPositionX",
        AnimatedPropertyKind::IkTargetPositionY => "ikTargetPositionY",
        AnimatedPropertyKind::BoneLength => "boneLength",
        AnimatedPropertyKind::ConstraintStrength => "constraintStrength",
        AnimatedPropertyKind::ConstraintOffset => "constraintOffset",
        AnimatedPropertyKind::GradientStopColor => "gradientStopColor",
        AnimatedPropertyKind::GradientStopOpacity => "gradientStopOpacity",
        AnimatedPropertyKind::GradientStopOffset => "gradientStopOffset",
        AnimatedPropertyKind::CameraX => "cameraX",
        AnimatedPropertyKind::CameraY => "cameraY",
        AnimatedPropertyKind::CameraZoom => "cameraZoom",
        AnimatedPropertyKind::CameraRotation => "cameraRotation",
        AnimatedPropertyKind::PathData => "pathData",
        AnimatedPropertyKind::StrokeTrimStart => "strokeTrimStart",
        AnimatedPropertyKind::StrokeTrimEnd => "strokeTrimEnd",
        AnimatedPropertyKind::StrokeTrimOffset => "strokeTrimOffset",
        AnimatedPropertyKind::SoloIndex => "soloIndex",
        AnimatedPropertyKind::DrawOrder => "drawOrder",
        AnimatedPropertyKind::BonePositionX => "bonePositionX",
        AnimatedPropertyKind::BonePositionY => "bonePositionY",
        // FlatBuffers generates enums as newtype structs, not Rust enums, so
        // the compiler cannot force this match to be exhaustive the way the
        // editor-side ledger is (`lib/src/data/runtime_capabilities.dart`).
        // `tests/schema_coverage.rs` walks `ENUM_VALUES_ANIMATED_PROPERTY_KIND`
        // — which the generator keeps in step with the schema — and fails if
        // any member lands here. Adding a kind to the schema without handling
        // it breaks that test.
        _ => UNHANDLED_PROPERTY,
    }
}

/// What [`property_key`] returns for a schema kind it does not know.
///
/// Public so the coverage test can look for exactly this rather than guessing
/// at a magic string.
pub const UNHANDLED_PROPERTY: &str = "unknown";

/// Pure evaluator: given a FlatBuffers `Animation` and playhead `t` (seconds),
/// returns per-node animated overrides keyed by node-id string.
/// Mirrors `evaluateAnimation` in `timeline_evaluator.dart`.
pub fn evaluate_animation<'a>(animation: &Animation<'a>, t: f64) -> HashMap<String, NodeOverrides> {
    let mut result: HashMap<String, NodeOverrides> = HashMap::new();
    let tracks = match animation.tracks() {
        Some(ts) => ts,
        None => return result,
    };

    for track in tracks {
        let node_id = track.target_node_id().to_owned();
        let value = match evaluate_track_at(track.keyframes(), t) {
            Some(v) => v,
            None => continue,
        };
        let key = property_key(track.property());
        result.entry(node_id).or_default().values.insert(key, value);
    }
    result
}

fn evaluate_track_at<'a>(
    keyframes: Option<
        flatbuffers::Vector<
            'a,
            flatbuffers::ForwardsUOffset<crate::format::generated::kineto::KFKeyframe<'a>>,
        >,
    >,
    t: f64,
) -> Option<PropertyValue> {
    let kfs = keyframes?;
    if kfs.is_empty() {
        return None;
    }

    let first = kfs.get(0);
    let last = kfs.get(kfs.len() - 1);

    if t <= first.time() as f64 {
        return Some(raw_value(first));
    }
    if t >= last.time() as f64 {
        return Some(raw_value(last));
    }

    let mut lo = 0usize;
    for i in 1..kfs.len() {
        if (kfs.get(i).time() as f64) > t {
            break;
        }
        lo = i;
    }
    let a = kfs.get(lo);
    let b = kfs.get(lo + 1);

    let seg = (b.time() - a.time()) as f64;
    if seg <= 0.0 {
        return Some(raw_value(b));
    }

    let local_t = (t - a.time() as f64) / seg;

    let eased_t = match a.interpolation() {
        KeyframeInterpolationKind::Hold => return Some(raw_value(a)),
        KeyframeInterpolationKind::Linear => local_t,
        KeyframeInterpolationKind::Bezier => {
            let ox = a.bezier_out().map(|h| h.x()).unwrap_or(0.25) as f64;
            let oy = a.bezier_out().map(|h| h.y()).unwrap_or(0.1) as f64;
            let ix = a.bezier_in().map(|h| h.x()).unwrap_or(0.75) as f64;
            let iy = a.bezier_in().map(|h| h.y()).unwrap_or(0.9) as f64;
            cubic_bezier_ease(local_t, ox, oy, ix, iy)
        }
        KeyframeInterpolationKind::AutoBezier => auto_bezier_ease(local_t),
        _ => local_t,
    };

    Some(interpolate(a, b, eased_t))
}

fn raw_value(kf: crate::format::generated::kineto::KFKeyframe<'_>) -> PropertyValue {
    if let Some(vertices) = morph_vertices(kf) {
        return PropertyValue::Path(vertices);
    }
    if kf.is_color() {
        PropertyValue::Color(kf.color_value())
    } else {
        PropertyValue::Scalar(kf.scalar_value() as f64)
    }
}

/// A keyframe's morph snapshot, if it carries one.
///
/// Handles are stored relative to their anchor, the same as every other path
/// in the format.
fn morph_vertices(
    kf: crate::format::generated::kineto::KFKeyframe<'_>,
) -> Option<Vec<MorphVertex>> {
    let vertices = kf.path_vertices()?;
    if vertices.is_empty() {
        return None;
    }
    Some(
        vertices
            .iter()
            .map(|v| MorphVertex {
                anchor: (v.anchor_x() as f64, v.anchor_y() as f64),
                in_handle: (v.in_handle_x() as f64, v.in_handle_y() as f64),
                out_handle: (v.out_handle_x() as f64, v.out_handle_y() as f64),
            })
            .collect(),
    )
}

fn interpolate(
    a: crate::format::generated::kineto::KFKeyframe<'_>,
    b: crate::format::generated::kineto::KFKeyframe<'_>,
    t: f64,
) -> PropertyValue {
    if let (Some(from), Some(to)) = (morph_vertices(a), morph_vertices(b)) {
        // Vertex counts must agree, or there is no correspondence to
        // interpolate along. The editor holds the earlier keyframe in that
        // case (`timeline_eval.dart:106`) and so does this.
        if from.len() != to.len() {
            return PropertyValue::Path(from);
        }
        let lerp = |a: (f64, f64), b: (f64, f64)| {
            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
        };
        return PropertyValue::Path(
            from.iter()
                .zip(&to)
                .map(|(x, y)| MorphVertex {
                    anchor: lerp(x.anchor, y.anchor),
                    in_handle: lerp(x.in_handle, y.in_handle),
                    out_handle: lerp(x.out_handle, y.out_handle),
                })
                .collect(),
        );
    }

    if a.is_color() && b.is_color() {
        PropertyValue::Color(lerp_color(a.color_value(), b.color_value(), t))
    } else {
        let av = a.scalar_value() as f64;
        let bv = b.scalar_value() as f64;
        PropertyValue::Scalar(av + (bv - av) * t)
    }
}

fn lerp_color(a: u32, b: u32, t: f64) -> u32 {
    let lerp_ch =
        |ca: u8, cb: u8| -> u8 { (ca as f64 + (cb as f64 - ca as f64) * t).round() as u8 };
    let [a0, a1, a2, a3] = a.to_be_bytes();
    let [b0, b1, b2, b3] = b.to_be_bytes();
    u32::from_be_bytes([
        lerp_ch(a0, b0),
        lerp_ch(a1, b1),
        lerp_ch(a2, b2),
        lerp_ch(a3, b3),
    ])
}
