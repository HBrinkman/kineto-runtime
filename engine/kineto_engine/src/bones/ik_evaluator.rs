use std::collections::HashMap;

use crate::format;

use super::fk_solver::{bone_world_tip, solve_fk, Matrix4, Point2, RuntimeBone};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeIkSolverKind {
    TwoBone,
    Fabrik,
}

impl From<format::IkSolverKind> for RuntimeIkSolverKind {
    fn from(value: format::IkSolverKind) -> Self {
        match value {
            format::IkSolverKind::TwoBone => Self::TwoBone,
            _ => Self::Fabrik,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeIkChain {
    pub id: String,
    pub root_bone_id: String,
    pub effector_bone_id: String,
    pub target_id: String,
    pub solver_kind: RuntimeIkSolverKind,
    /// Which way a two-bone chain folds: negative one way, positive the other,
    /// and 0 for "nothing authored — read it off the rest pose".
    pub bend_direction: i8,
}

impl RuntimeIkChain {
    pub fn from_flatbuffer(chain: format::IkChain<'_>) -> Self {
        Self {
            id: chain.id().to_owned(),
            root_bone_id: chain.root_bone_id().to_owned(),
            effector_bone_id: chain.effector_bone_id().to_owned(),
            target_id: chain.target_id().to_owned(),
            solver_kind: chain.solver_kind().into(),
            bend_direction: chain.bend_direction(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeIkTarget {
    pub id: String,
    pub position: Point2,
    /// How far an input may carry this point, centred on `position`
    /// (issue #74). Zero is a plain point.
    pub extent: (f64, f64),
    /// The Group node that carries this target through the Scene, if any.
    pub parent_id: Option<String>,
    /// Whether this target catches touch (issue #74).
    pub interactive: bool,
}

impl RuntimeIkTarget {
    pub fn from_flatbuffer(target: format::IkTarget<'_>) -> Self {
        Self {
            id: target.id().to_owned(),
            position: Point2 {
                x: target.position_x() as f64,
                y: target.position_y() as f64,
            },
            extent: (target.extent_w() as f64, target.extent_h() as f64),
            parent_id: target.parent_id().map(|id| id.to_owned()),
            interactive: target.interactive(),
        }
    }
}

pub fn ik_chains_from_document(document: &format::Document<'_>) -> Vec<RuntimeIkChain> {
    document
        .ik_chains()
        .unwrap_or_default()
        .iter()
        .map(RuntimeIkChain::from_flatbuffer)
        .collect()
}

pub fn ik_targets_from_document(document: &format::Document<'_>) -> Vec<RuntimeIkTarget> {
    document
        .ik_targets()
        .unwrap_or_default()
        .iter()
        .map(RuntimeIkTarget::from_flatbuffer)
        .collect()
}

/// Evaluates IK chains and returns bone id -> local rotation overrides.
///
/// Mirrors `packages/kineto_runtime/lib/src/bones/ik_evaluator.dart`.
pub fn evaluate_ik(
    bones: &[RuntimeBone],
    chains: &[RuntimeIkChain],
    targets: &[RuntimeIkTarget],
    fk: &HashMap<String, Matrix4>,
) -> HashMap<String, f64> {
    let mut result = HashMap::new();
    let by_id: HashMap<&str, &RuntimeBone> =
        bones.iter().map(|bone| (bone.id.as_str(), bone)).collect();
    let target_by_id: HashMap<&str, &RuntimeIkTarget> = targets
        .iter()
        .map(|target| (target.id.as_str(), target))
        .collect();

    for chain in chains {
        let Some(target) = target_by_id.get(chain.target_id.as_str()) else {
            continue;
        };
        let chain_bones = extract_chain(chain, &by_id);
        if chain_bones.is_empty() {
            continue;
        }

        let Some(joints) = extract_joint_positions(&chain_bones, fk) else {
            continue;
        };

        let new_joints = match chain.solver_kind {
            RuntimeIkSolverKind::TwoBone if chain_bones.len() == 2 => {
                solve_two_bone(
                    &joints,
                    &chain_bones,
                    target.position,
                    bend_sign(chain.bend_direction, chain_bones[1]),
                )
            }
            _ => solve_fabrik(&joints, &chain_bones, target.position),
        };

        joints_to_local_rotations(&chain_bones, &new_joints, fk, &mut result);
    }

    result
}

/// Runs the full skeletal pipeline: FK -> IK override -> final FK.
pub fn evaluate_skeletal(
    bones: &[RuntimeBone],
    chains: &[RuntimeIkChain],
    targets: &[RuntimeIkTarget],
) -> HashMap<String, Matrix4> {
    let initial_fk = solve_fk(bones);
    if chains.is_empty() {
        return initial_fk;
    }

    let overrides = evaluate_ik(bones, chains, targets, &initial_fk);
    if overrides.is_empty() {
        return initial_fk;
    }

    let resolved_bones: Vec<_> = bones
        .iter()
        .map(|bone| {
            let mut resolved = bone.clone();
            if let Some(rotation) = overrides.get(&bone.id) {
                resolved.local_rotation = *rotation;
            }
            resolved
        })
        .collect();

    solve_fk(&resolved_bones)
}

fn extract_chain<'a>(
    chain: &RuntimeIkChain,
    by_id: &HashMap<&str, &'a RuntimeBone>,
) -> Vec<&'a RuntimeBone> {
    let Some(mut current) = by_id.get(chain.effector_bone_id.as_str()).copied() else {
        return Vec::new();
    };

    let mut path = Vec::new();
    loop {
        path.insert(0, current);
        if current.id == chain.root_bone_id {
            break;
        }
        let Some(parent_id) = current.parent_id.as_deref() else {
            break;
        };
        let Some(parent) = by_id.get(parent_id).copied() else {
            break;
        };
        current = parent;
        if path.len() > 64 {
            break;
        }
    }

    if path.first().map(|bone| bone.id.as_str()) != Some(chain.root_bone_id.as_str()) {
        return Vec::new();
    }
    path
}

fn extract_joint_positions(
    chain_bones: &[&RuntimeBone],
    fk: &HashMap<String, Matrix4>,
) -> Option<Vec<Point2>> {
    let mut positions = Vec::new();
    for bone in chain_bones {
        let matrix = fk.get(&bone.id)?;
        positions.push(Point2 {
            x: matrix[12],
            y: matrix[13],
        });
    }

    let last = chain_bones.last()?;
    let last_matrix = *fk.get(&last.id)?;
    positions.push(bone_world_tip(last, last_matrix));
    Some(positions)
}

fn solve_fabrik(joints: &[Point2], chain_bones: &[&RuntimeBone], target: Point2) -> Vec<Point2> {
    let n = chain_bones.len();
    let lengths: Vec<_> = chain_bones.iter().map(|bone| bone.length).collect();
    let root = joints[0];
    let mut result = joints.to_vec();

    let total_len: f64 = lengths.iter().sum();
    let distance = length(sub(target, root));

    if distance >= total_len {
        let dir = normalize(sub(target, root));
        let mut pos = root;
        for i in 0..n {
            result[i] = pos;
            pos = add(pos, scale(dir, lengths[i]));
        }
        result[n] = pos;
        return result;
    }

    const MAX_ITERATIONS: usize = 16;
    const TOLERANCE: f64 = 0.01;

    for _ in 0..MAX_ITERATIONS {
        result[n] = target;
        for i in (0..n).rev() {
            let dir = normalize(sub(result[i], result[i + 1]));
            result[i] = add(result[i + 1], scale(dir, lengths[i]));
        }

        result[0] = root;
        for i in 1..=n {
            let dir = normalize(sub(result[i], result[i - 1]));
            result[i] = add(result[i - 1], scale(dir, lengths[i - 1]));
        }

        if length(sub(result[n], target)) < TOLERANCE {
            break;
        }
    }

    result
}

/// The side a two-bone chain folds to.
///
/// Mirrors `resolveTwoBoneBendSign` (`ik_two_bone.dart:127`): an authored
/// direction wins, because it is the user saying "the knee bends *this* way"
/// and it has to survive dragging the target through the straight pose where
/// the pose-derived guess flips. Only "nothing authored" reads the rest pose.
fn bend_sign(bend_direction: i8, mid: &RuntimeBone) -> f64 {
    if bend_direction != 0 {
        return if bend_direction < 0 { -1.0 } else { 1.0 };
    }
    if mid.local_rotation < -1e-6 {
        -1.0
    } else {
        1.0
    }
}

fn solve_two_bone(
    joints: &[Point2],
    chain_bones: &[&RuntimeBone],
    target: Point2,
    bend_sign: f64,
) -> Vec<Point2> {
    let l1 = chain_bones[0].length;
    let l2 = chain_bones[1].length;
    let root = joints[0];

    let dx = target.x - root.x;
    let dy = target.y - root.y;
    let distance = (dx * dx + dy * dy).sqrt();

    if distance < 1e-6 {
        return joints.to_vec();
    }

    let total = l1 + l2;
    if distance >= total {
        let dir = normalize(sub(target, root));
        let mid = add(root, scale(dir, l1));
        return vec![root, mid, add(mid, scale(dir, l2))];
    }

    let cos_alpha = ((l1 * l1) + (distance * distance) - (l2 * l2)) / (2.0 * l1 * distance);
    let alpha = cos_alpha.clamp(-1.0, 1.0).acos();

    // Which side the elbow folds to. The runtime hardcoded one, so a chain
    // authored to bend the other way flipped on device (issue #111 parity
    // audit §4). `resolveTwoBoneBendSign` in `ik_two_bone.dart:127` is the
    // rule: an authored direction wins, and 0 falls back to reading the rest
    // pose — which the caller has already done.
    let root_world_rot = dy.atan2(dx) - std::f64::consts::PI / 2.0 - bend_sign * alpha;
    let mid = Point2 {
        x: root.x + l1 * (-(root_world_rot + std::f64::consts::PI / 2.0).sin()),
        y: root.y + l1 * (root_world_rot + std::f64::consts::PI / 2.0).cos(),
    };
    let dir2 = normalize(sub(target, mid));
    vec![root, mid, add(mid, scale(dir2, l2))]
}

fn joints_to_local_rotations(
    chain_bones: &[&RuntimeBone],
    joints: &[Point2],
    fk: &HashMap<String, Matrix4>,
    result: &mut HashMap<String, f64>,
) {
    for i in 0..chain_bones.len() {
        let bone = chain_bones[i];
        let dx = joints[i + 1].x - joints[i].x;
        let dy = joints[i + 1].y - joints[i].y;
        let world_rot = (-dx).atan2(dy);

        let parent_world_rot = if i > 0 {
            let pdx = joints[i].x - joints[i - 1].x;
            let pdy = joints[i].y - joints[i - 1].y;
            (-pdx).atan2(pdy)
        } else {
            bone.parent_id
                .as_ref()
                .and_then(|parent_id| fk.get(parent_id))
                .map(|pm| pm[1].atan2(pm[0]))
                .unwrap_or(0.0)
        };

        result.insert(
            bone.id.clone(),
            normalize_angle(world_rot - parent_world_rot),
        );
    }
}

fn add(a: Point2, b: Point2) -> Point2 {
    Point2 {
        x: a.x + b.x,
        y: a.y + b.y,
    }
}

fn sub(a: Point2, b: Point2) -> Point2 {
    Point2 {
        x: a.x - b.x,
        y: a.y - b.y,
    }
}

fn scale(v: Point2, factor: f64) -> Point2 {
    Point2 {
        x: v.x * factor,
        y: v.y * factor,
    }
}

fn length(v: Point2) -> f64 {
    (v.x * v.x + v.y * v.y).sqrt()
}

fn normalize(v: Point2) -> Point2 {
    let len = length(v);
    if len > 1e-9 {
        scale(v, 1.0 / len)
    } else {
        Point2 { x: 0.0, y: 1.0 }
    }
}

fn normalize_angle(mut angle: f64) -> f64 {
    while angle > std::f64::consts::PI {
        angle -= 2.0 * std::f64::consts::PI;
    }
    while angle < -std::f64::consts::PI {
        angle += 2.0 * std::f64::consts::PI;
    }
    angle
}
