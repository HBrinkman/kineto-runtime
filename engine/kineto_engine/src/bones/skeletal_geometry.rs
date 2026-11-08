use std::collections::HashMap;

use crate::format;

use super::fk_solver::{invert_affine_4x4, multiply, transform, Matrix4, Point2};

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeBoneWeight {
    pub bone_id: String,
    pub weight: f64,
}

impl RuntimeBoneWeight {
    pub fn from_flatbuffer(weight: format::BoneWeight<'_>) -> Self {
        Self {
            bone_id: weight.bone_id().to_owned(),
            weight: weight.weight() as f64,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimePathPoint {
    pub anchor: Point2,
    pub in_handle: Point2,
    pub out_handle: Point2,
    pub bone_id: Option<String>,
    pub bone_influence: f64,
    pub weights: Vec<RuntimeBoneWeight>,
}

impl RuntimePathPoint {
    pub fn unbound(anchor: Point2, in_handle: Point2, out_handle: Point2) -> Self {
        Self {
            anchor,
            in_handle,
            out_handle,
            bone_id: None,
            bone_influence: 1.0,
            weights: Vec::new(),
        }
    }

    pub fn from_path_node_v2(node: format::generated::kineto::PathNodeFBV2<'_>) -> Self {
        Self {
            anchor: Point2 {
                x: node.anchor_x() as f64,
                y: node.anchor_y() as f64,
            },
            in_handle: Point2 {
                x: node.in_handle_x() as f64,
                y: node.in_handle_y() as f64,
            },
            out_handle: Point2 {
                x: node.out_handle_x() as f64,
                y: node.out_handle_y() as f64,
            },
            bone_id: node.bone_id().map(str::to_owned),
            bone_influence: node.bone_influence() as f64,
            weights: node
                .weights()
                .unwrap_or_default()
                .iter()
                .map(RuntimeBoneWeight::from_flatbuffer)
                .collect(),
        }
    }
}

/// Applies skeletal transforms to path points.
///
/// M5 rigid binding uses `bone_id`. M5.5 weighted skinning uses `weights`;
/// when weights are present they take precedence and are renormalised by the
/// total valid weight, matching the existing Dart runtime behavior.
/// Deforms `points` by the skeleton, in the space the bones live in.
///
/// `local_to_world` is the node's own world matrix and `world_to_local` its
/// inverse. Bone matrices are world-space, so a point has to make that round
/// trip: the runtime used to apply the deform straight to node-local points,
/// which agrees with the editor (`lbs_deformer.dart:139-165`) only when the
/// node's world transform happens to be identity. Any rigged artwork inside a
/// moved or scaled group deformed differently on device than in the editor
/// (issue #111 parity audit §5.6).
///
/// Passing identity for both is still meaningful: it deforms in the node's own
/// space, which is what a top-level node's world matrix already is.
pub fn apply_skeletal_transforms(
    points: &[RuntimePathPoint],
    current_fk: &HashMap<String, Matrix4>,
    rest_fk: &HashMap<String, Matrix4>,
    local_to_world: Matrix4,
    world_to_local: Matrix4,
) -> Vec<RuntimePathPoint> {
    points
        .iter()
        .map(|point| {
            let world = RuntimePathPoint {
                anchor: transform_point(local_to_world, point.anchor),
                in_handle: transform_vector(local_to_world, point.in_handle),
                out_handle: transform_vector(local_to_world, point.out_handle),
                ..point.clone()
            };
            let deformed = if !world.weights.is_empty() {
                apply_weighted(&world, current_fk, rest_fk)
            } else {
                apply_single_bone(&world, current_fk, rest_fk)
            };
            RuntimePathPoint {
                anchor: transform_point(world_to_local, deformed.anchor),
                in_handle: transform_vector(world_to_local, deformed.in_handle),
                out_handle: transform_vector(world_to_local, deformed.out_handle),
                ..deformed
            }
        })
        .collect()
}

/// The 4x4 form of a 2D affine, in the column-major layout the FK solver uses.
pub fn matrix4_from_affine(affine: [f64; 6]) -> Matrix4 {
    let [a, b, c, d, e, f] = affine;
    [
        a, b, 0.0, 0.0, //
        c, d, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        e, f, 0.0, 1.0,
    ]
}

/// The identity, for a node whose world transform is not known or not wanted.
pub const IDENTITY_MATRIX4: Matrix4 = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

fn apply_single_bone(
    point: &RuntimePathPoint,
    current_fk: &HashMap<String, Matrix4>,
    rest_fk: &HashMap<String, Matrix4>,
) -> RuntimePathPoint {
    let Some(bone_id) = point.bone_id.as_deref() else {
        return point.clone();
    };
    let Some(deform) = deform_for_bone(bone_id, current_fk, rest_fk) else {
        return point.clone();
    };

    let mut moved = point.clone();
    moved.anchor = transform_point(deform, point.anchor);
    moved.in_handle = transform_vector(deform, point.in_handle);
    moved.out_handle = transform_vector(deform, point.out_handle);
    moved
}

fn apply_weighted(
    point: &RuntimePathPoint,
    current_fk: &HashMap<String, Matrix4>,
    rest_fk: &HashMap<String, Matrix4>,
) -> RuntimePathPoint {
    let mut anchor = Point2::ZERO;
    let mut in_handle = Point2::ZERO;
    let mut out_handle = Point2::ZERO;
    let mut total_weight = 0.0;

    for weight in &point.weights {
        let Some(deform) = deform_for_bone(&weight.bone_id, current_fk, rest_fk) else {
            continue;
        };
        let transformed_anchor = transform_point(deform, point.anchor);
        let transformed_in = transform_vector(deform, point.in_handle);
        let transformed_out = transform_vector(deform, point.out_handle);

        anchor.x += weight.weight * transformed_anchor.x;
        anchor.y += weight.weight * transformed_anchor.y;
        in_handle.x += weight.weight * transformed_in.x;
        in_handle.y += weight.weight * transformed_in.y;
        out_handle.x += weight.weight * transformed_out.x;
        out_handle.y += weight.weight * transformed_out.y;
        total_weight += weight.weight;
    }

    if total_weight.abs() < 1e-9 {
        return point.clone();
    }

    let mut moved = point.clone();
    moved.anchor = Point2 {
        x: anchor.x / total_weight,
        y: anchor.y / total_weight,
    };
    moved.in_handle = Point2 {
        x: in_handle.x / total_weight,
        y: in_handle.y / total_weight,
    };
    moved.out_handle = Point2 {
        x: out_handle.x / total_weight,
        y: out_handle.y / total_weight,
    };
    moved
}

fn deform_for_bone(
    bone_id: &str,
    current_fk: &HashMap<String, Matrix4>,
    rest_fk: &HashMap<String, Matrix4>,
) -> Option<Matrix4> {
    let current = current_fk.get(bone_id)?;
    let rest = rest_fk.get(bone_id)?;
    let rest_inv = invert_affine_4x4(*rest)?;
    Some(multiply(*current, rest_inv))
}

fn transform_point(matrix: Matrix4, point: Point2) -> Point2 {
    let transformed = transform(matrix, [point.x, point.y, 0.0, 1.0]);
    Point2 {
        x: transformed[0],
        y: transformed[1],
    }
}

fn transform_vector(matrix: Matrix4, vector: Point2) -> Point2 {
    let transformed = transform(matrix, [vector.x, vector.y, 0.0, 0.0]);
    Point2 {
        x: transformed[0],
        y: transformed[1],
    }
}
