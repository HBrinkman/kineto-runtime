//! Constraints that drive artwork from something else (issue #111).
//!
//! An iris that follows a target, a limb that aims at a hand. The thing that
//! moves is `constrained_id`; what it follows is `source_id`, which may be a
//! node or an IK target.
//!
//! ## Why this mirrors the editor so closely
//!
//! The editor already solves these, and four independent readings of one
//! Bézier rule is how a document came to render as spikes on a device. So this
//! follows `lib/src/domain/bones/shape_constraints.dart` step for step, and
//! `tests/constraint_parity.rs` holds it to the editor's own numbers.
//!
//! The steps, and why each exists:
//!
//! 1. A shape is seeded at the **centre of its bounds**, not at its transform
//!    origin — that is where the eye puts an object, so distance and aim
//!    measure from the middle of the artwork.
//! 2. Constraints are applied in world space.
//! 3. The result is decomposed back into the node's **parent space**, as a
//!    delta on the authored transform, because that is what a node stores.

use std::collections::HashMap;

use crate::bones::fk_solver::Matrix4;
use crate::bones::RuntimePathPoint;
use crate::format;
use crate::render::{RenderPathContour, RenderPathPoint};
use crate::hierarchy::{apply_point, invert, parent_map, world_affines, Affine};
use crate::animation::NodeOverrides;

/// What a constraint did to one node, in its parent's space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstraintPose {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub scale_x: f64,
    pub scale_y: f64,
}

/// Solves every constraint whose constrained object is an artwork node.
///
/// Returns one pose per constrained node, ready to be merged into the frame's
/// overrides. Nodes with no constraint are absent, so the common path costs
/// nothing.
pub fn solve_shape_constraints(
    document: &format::Document<'_>,
    ik_targets: &HashMap<String, (f64, f64)>,
    overrides: &HashMap<String, NodeOverrides>,
) -> HashMap<String, ConstraintPose> {
    solve_shape_constraints_with_bones(document, ik_targets, overrides, &HashMap::new())
}

/// [`solve_shape_constraints`], with posed bones available as sources: a shape
/// can follow a bone, as in the editor (`solveShapeConstraintTransforms`
/// seeds its solve with the rig's world matrices).
pub fn solve_shape_constraints_with_bones(
    document: &format::Document<'_>,
    ik_targets: &HashMap<String, (f64, f64)>,
    overrides: &HashMap<String, NodeOverrides>,
    fk: &HashMap<String, Matrix4>,
) -> HashMap<String, ConstraintPose> {
    let bones = bone_affines(fk);
    let constraints = document.constraints().unwrap_or_default();
    if constraints.is_empty() {
        return HashMap::new();
    }

    let nodes: HashMap<String, format::Node<'_>> = document
        .nodes()
        .unwrap_or_default()
        .iter()
        .map(|node| (node.id().to_owned(), node))
        .collect();

    let world = world_affines(document, overrides);
    let parents = parent_map(document);

    // Seed: each constrained shape sits at the centre of its own bounds.
    let mut solved: HashMap<String, SolvedWorld> = HashMap::new();
    for constraint in constraints.iter() {
        let id = constraint.constrained_id();
        if solved.contains_key(id) || !nodes.contains_key(id) {
            continue;
        }
        let Some(affine) = world.get(id) else {
            continue;
        };
        let Some(centre) = node_world_centre(&nodes[id], *affine) else {
            continue;
        };
        solved.insert(
            id.to_owned(),
            SolvedWorld {
                affine: *affine,
                centre,
                original_centre: centre,
                rotation: rotation_of(*affine),
                scale: (scale_x_of(*affine), scale_y_of(*affine)),
                original_rotation: rotation_of(*affine),
                original_scale: (scale_x_of(*affine), scale_y_of(*affine)),
            },
        );
    }

    // A constraint may read what an earlier one wrote, so order matters. The
    // editor topologically sorts (`constraint_solver.dart:546`) and the
    // runtime used to take the document's order, so a chain authored
    // "downstream first" posed one frame behind on device.
    let constraints = topo_sorted(constraints.iter().collect());

    let sources = Sources {
        world: &world,
        nodes: &nodes,
        ik_targets,
        bones: &bones,
    };
    run_constraints(constraints, &mut solved, &sources, overrides);

    // Decompose back into parent space, as a delta on what the node authored.
    let mut poses = HashMap::new();
    for (id, state) in &solved {
        let Some(node) = nodes.get(id) else { continue };
        let parent_inverse = parents
            .get(id)
            .and_then(|parent| world.get(parent))
            .and_then(|affine| invert(*affine))
            .unwrap_or(crate::hierarchy::IDENTITY);

        let before = apply_point(parent_inverse, state.original_centre.0, state.original_centre.1);
        let after = apply_point(parent_inverse, state.centre.0, state.centre.1);

        let base = authored_transform(node, overrides.get(id));
        let rotation_delta = normalize_angle(state.rotation - state.original_rotation);
        let scale_ratio_x = if state.original_scale.0 == 0.0 {
            1.0
        } else {
            state.scale.0 / state.original_scale.0
        };
        let scale_ratio_y = if state.original_scale.1 == 0.0 {
            1.0
        } else {
            state.scale.1 / state.original_scale.1
        };

        let mut pose = ConstraintPose {
            x: base.0 + (after.0 - before.0),
            y: base.1 + (after.1 - before.1),
            rotation: base.2 + rotation_delta,
            scale_x: base.3 * scale_ratio_x,
            scale_y: base.4 * scale_ratio_y,
        };

        // A node rotates and scales about its own origin, but the solve
        // happened about the shape's centre — so changing either walks the
        // centre away from where the constraint put it. Measure where the
        // candidate actually lands and slide it back, exactly as the editor
        // does (`shape_constraints.dart:140-155`, issue #79 feedback "Scale
        // does not behave correctly"). Without this the runtime and the editor
        // disagree by a translation whenever a Copy constraint touches
        // rotation or scale.
        if rotation_delta != 0.0 || scale_ratio_x != 1.0 || scale_ratio_y != 1.0 {
            let anchor = authored_anchor(node, overrides.get(id));
            let parent_world = parents
                .get(id)
                .and_then(|parent| world.get(parent))
                .copied()
                .unwrap_or(crate::hierarchy::IDENTITY);
            let candidate_world = crate::hierarchy::multiply(
                parent_world,
                pose_affine(&pose, anchor),
            );
            if let Some(landed) = node_world_centre(node, candidate_world) {
                let landed_local = apply_point(parent_inverse, landed.0, landed.1);
                pose.x += after.0 - landed_local.0;
                pose.y += after.1 - landed_local.1;
            }
        }

        poses.insert(id.clone(), pose);
    }
    poses
}

/// Everything a constraint's source can be, in world space.
struct Sources<'a, 'b> {
    world: &'a HashMap<String, Affine>,
    nodes: &'a HashMap<String, format::Node<'b>>,
    ik_targets: &'a HashMap<String, (f64, f64)>,
    /// Posed bone world transforms; a bone's position is its origin.
    bones: &'a HashMap<String, Affine>,
}

/// Applies `constraints` (already in dependency order) to the objects seeded
/// in `solved`, which is `applyConstraints` in the editor: one pass, each
/// constraint reading what earlier ones wrote.
fn run_constraints(
    constraints: Vec<format::Constraint<'_>>,
    solved: &mut HashMap<String, SolvedWorld>,
    sources: &Sources<'_, '_>,
    overrides: &HashMap<String, NodeOverrides>,
) {
    for constraint in constraints.into_iter() {
        let id = constraint.constrained_id().to_owned();
        if !solved.contains_key(&id) {
            continue;
        }
        let Some(source) = source_position(constraint.source_id(), solved, sources) else {
            continue;
        };

        // Strength and offset are animatable. They used to export as
        // BoneRotation, so animating a constraint's strength rotated a bone
        // instead (issue #111). Keyed by the *constrained* object, which is
        // the subject the editor's timeline puts the track on.
        let constraint_overrides = overrides.get(&id);
        let strength = constraint_overrides
            .and_then(|values| values.scalar("constraintStrength"))
            .unwrap_or(constraint.strength() as f64);
        let current = solved[&id];
        let next = match constraint.kind() {
            format::ConstraintKind::Distance => {
                apply_distance(constraint, current, source, strength)
            }
            format::ConstraintKind::Copy => {
                let basis = source_basis(constraint.source_id(), solved, sources);
                apply_copy(constraint, current, source, basis, strength)
            }
            format::ConstraintKind::Aim => apply_aim(
                constraint,
                current,
                source,
                strength,
                constraint_overrides.and_then(|values| values.scalar("constraintOffset")),
            ),
            // Authoring-only so far. Left alone rather than approximated: a
            // shape in the wrong place is worse than one that did not move.
            _ => current,
        };
        solved.insert(id, next);
    }
}

/// Solves every constraint whose constrained object is a bone, writing the
/// result into the bones' world matrices in place.
///
/// The editor runs this right after FK and IK (`skeletal_solver.dart`,
/// `applyConstraints` on the rig's world matrices), and the runtime skipped it
/// entirely: a bone constrained to follow a target sat still on device. A
/// bone is measured from its origin, and only its own matrix changes — its
/// child bones are not re-solved, as in the editor.
pub fn solve_bone_constraints(
    document: &format::Document<'_>,
    fk: &mut HashMap<String, Matrix4>,
    ik_targets: &HashMap<String, (f64, f64)>,
    overrides: &HashMap<String, NodeOverrides>,
) {
    let constraints = document.constraints().unwrap_or_default();
    if constraints.is_empty() || fk.is_empty() {
        return;
    }
    let bones = bone_affines(fk);
    let mut solved: HashMap<String, SolvedWorld> = HashMap::new();
    for constraint in constraints.iter() {
        let id = constraint.constrained_id();
        if solved.contains_key(id) {
            continue;
        }
        let Some(affine) = bones.get(id) else {
            continue;
        };
        let centre = (affine[4], affine[5]);
        let rotation = rotation_of(*affine);
        let scale = (scale_x_of(*affine), scale_y_of(*affine));
        solved.insert(
            id.to_owned(),
            SolvedWorld {
                affine: *affine,
                centre,
                original_centre: centre,
                rotation,
                scale,
                original_rotation: rotation,
                original_scale: scale,
            },
        );
    }
    if solved.is_empty() {
        return;
    }

    let nodes: HashMap<String, format::Node<'_>> = document
        .nodes()
        .unwrap_or_default()
        .iter()
        .map(|node| (node.id().to_owned(), node))
        .collect();
    let world = world_affines(document, overrides);
    let sources = Sources {
        world: &world,
        nodes: &nodes,
        ik_targets,
        bones: &bones,
    };
    run_constraints(topo_sorted(constraints.iter().collect()), &mut solved, &sources, overrides);

    for (id, state) in solved {
        let Some(m) = fk.get_mut(&id) else { continue };
        let (sin, cos) = state.rotation.sin_cos();
        m[0] = cos * state.scale.0;
        m[1] = sin * state.scale.0;
        m[4] = -sin * state.scale.1;
        m[5] = cos * state.scale.1;
        m[12] = state.centre.0;
        m[13] = state.centre.1;
    }
}

/// Bone world matrices as 2D affines.
fn bone_affines(fk: &HashMap<String, Matrix4>) -> HashMap<String, Affine> {
    fk.iter()
        .map(|(id, m)| (id.clone(), [m[0], m[1], m[4], m[5], m[12], m[13]]))
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct SolvedWorld {
    affine: Affine,
    centre: (f64, f64),
    original_centre: (f64, f64),
    rotation: f64,
    scale: (f64, f64),
    original_rotation: f64,
    original_scale: (f64, f64),
}

fn apply_distance(
    constraint: format::Constraint<'_>,
    mut state: SolvedWorld,
    source: (f64, f64),
    strength: f64,
) -> SolvedWorld {
    let dx = state.centre.0 - source.0;
    let dy = state.centre.1 - source.1;
    let distance = (dx * dx + dy * dy).sqrt();
    if distance == 0.0 {
        return state;
    }

    // The authored model is one distance plus a rule about which side of it
    // the object may sit on; the min/max band is the older form and the
    // fallback for documents written before that.
    let clamped = if constraint.has_distance() {
        let ring = constraint.distance() as f64;
        match constraint.distance_mode() {
            // Closer: stay inside the ring.
            0 => distance.min(ring),
            // Exactly: sit on it.
            1 => ring,
            // Further: stay beyond it.
            _ => distance.max(ring),
        }
    } else {
        distance.clamp(
            constraint.min_distance() as f64,
            constraint.max_distance() as f64,
        )
    };

    if (clamped - distance).abs() < 0.001 {
        return state;
    }

    let unit = (dx / distance, dy / distance);
    let moved = (source.0 + unit.0 * clamped, source.1 + unit.1 * clamped);
    state.centre = (
        lerp(state.centre.0, moved.0, strength),
        lerp(state.centre.1, moved.1, strength),
    );
    state
}

/// Orders constraints so a source is solved before whatever reads it.
///
/// Mirrors `_topoSort` in `constraint_solver.dart:546`, bubble sort included:
/// the two have to agree on the order for a chain to pose the same, and a
/// cleverer sort could disagree on a cycle. The pass count is bounded so a
/// cycle terminates instead of spinning.
fn topo_sorted<'a>(constraints: Vec<format::Constraint<'a>>) -> Vec<format::Constraint<'a>> {
    let mut sorted = constraints;
    let limit = sorted.len().saturating_mul(sorted.len());
    let mut passes = 0;
    let mut swapped = true;
    while swapped && passes < limit {
        swapped = false;
        passes += 1;
        for i in 0..sorted.len().saturating_sub(1) {
            if sorted[i].source_id() == sorted[i + 1].constrained_id() {
                sorted.swap(i, i + 1);
                swapped = true;
            }
        }
    }
    sorted
}

fn apply_copy(
    constraint: format::Constraint<'_>,
    mut state: SolvedWorld,
    source: (f64, f64),
    source_basis: Option<SourceBasis>,
    strength: f64,
) -> SolvedWorld {
    // Rotation and scale share the matrix's 2x2 basis, so they are written
    // together: setting one alone wipes the other
    // (`constraint_solver.dart:107`). Both were simply not solved here at all,
    // and `copy_rotation` defaults to true — so the *default* Copy constraint
    // did nothing at runtime.
    if let Some(basis) = source_basis {
        let invert = constraint.invert();

        if constraint.copy_rotation() {
            let sign = if invert { -1.0 } else { 1.0 };
            let offset = if constraint.maintain_offset() {
                constraint.offset_rotation() as f64
            } else {
                0.0
            };
            let target = basis.rotation * sign + offset;
            state.rotation = clamp_optional(
                lerp_angle(state.rotation, target, strength),
                constraint
                    .has_min_rotation()
                    .then(|| constraint.min_rotation() as f64),
                constraint
                    .has_max_rotation()
                    .then(|| constraint.max_rotation() as f64),
            );
        }

        if constraint.copy_scale() {
            let min = constraint
                .has_min_scale()
                .then(|| constraint.min_scale() as f64);
            let max = constraint
                .has_max_scale()
                .then(|| constraint.max_scale() as f64);
            let factor_x = scale_factor(
                basis.size.0,
                constraint.scale_reference_x() as f64,
                invert,
            );
            let factor_y = scale_factor(
                basis.size.1,
                constraint.scale_reference_y() as f64,
                invert,
            );
            state.scale = (
                clamp_optional(
                    lerp(state.scale.0, state.scale.0 * factor_x, strength),
                    min,
                    max,
                ),
                clamp_optional(
                    lerp(state.scale.1, state.scale.1 * factor_y, strength),
                    min,
                    max,
                ),
            );
        }
    }

    if constraint.copy_position() {
        let target = if constraint.maintain_offset() {
            (
                source.0 + constraint.offset_x() as f64,
                source.1 + constraint.offset_y() as f64,
            )
        } else {
            source
        };
        state.centre = (
            lerp(state.centre.0, target.0, strength),
            lerp(state.centre.1, target.1, strength),
        );
    }
    state
}

/// What a Copy constraint needs from its source beyond a position.
#[derive(Debug, Clone, Copy)]
struct SourceBasis {
    rotation: f64,
    /// The source's scale times its intrinsic size, which is what
    /// `scale_reference_x/y` was captured against (`_sourceSizeOf`,
    /// `constraint_solver.dart:503`).
    size: (f64, f64),
}

/// How much the source has grown since the constraint was pointed at it.
///
/// Scale copying is *relative*: 1.0 is the model's "no reference captured"
/// value, and dividing a measured size by it would read a 200px source as a
/// 200x multiplier — so sit still until a real reference exists
/// (`constraint_solver.dart:510`).
fn scale_factor(source_size: f64, reference: f64, invert: bool) -> f64 {
    if (reference - 1.0).abs() < 1e-9 {
        return 1.0;
    }
    let reference = if reference.abs() < 1e-6 { 1.0 } else { reference };
    let factor = source_size / reference;
    if !invert {
        return factor;
    }
    if factor.abs() < 1e-6 {
        1.0
    } else {
        1.0 / factor
    }
}

fn clamp_optional(value: f64, min: Option<f64>, max: Option<f64>) -> f64 {
    let mut out = value;
    if let Some(min) = min {
        if out < min {
            out = min;
        }
    }
    if let Some(max) = max {
        if out > max {
            out = max;
        }
    }
    out
}

/// Interpolates along the shorter way round, so a constraint never takes the
/// long path between two angles either side of pi.
fn lerp_angle(a: f64, b: f64, t: f64) -> f64 {
    let mut diff = (b - a) % (2.0 * std::f64::consts::PI);
    if diff > std::f64::consts::PI {
        diff -= 2.0 * std::f64::consts::PI;
    }
    if diff < -std::f64::consts::PI {
        diff += 2.0 * std::f64::consts::PI;
    }
    a + diff * t
}

fn apply_aim(
    constraint: format::Constraint<'_>,
    mut state: SolvedWorld,
    source: (f64, f64),
    strength: f64,
    offset_override: Option<f64>,
) -> SolvedWorld {
    let dx = source.0 - state.centre.0;
    let dy = source.1 - state.centre.1;
    if dx == 0.0 && dy == 0.0 {
        return state;
    }
    let wanted = dy.atan2(dx) + offset_override.unwrap_or(constraint.offset_radians() as f64);
    let delta = normalize_angle(wanted - state.rotation);
    state.rotation += delta * strength;
    state
}

/// Where a constraint's source is, in world space.
///
/// A source may be an IK target — which is the case that matters here, since
/// that is what a timeline animates — or another node, in which case it is
/// read from the solve so a chain of constraints follows through.
fn source_position(
    source_id: &str,
    solved: &HashMap<String, SolvedWorld>,
    sources: &Sources<'_, '_>,
) -> Option<(f64, f64)> {
    if let Some(state) = solved.get(source_id) {
        return Some(state.centre);
    }
    // The editor reads the rig's world matrices first
    // (`resolveConstraintSourcePosition`): a bone is where its origin is.
    if let Some(bone) = sources.bones.get(source_id) {
        return Some((bone[4], bone[5]));
    }
    if let Some(position) = sources.ik_targets.get(source_id) {
        return Some(*position);
    }
    let node = sources.nodes.get(source_id)?;
    let affine = sources.world.get(source_id)?;
    node_world_centre(node, *affine)
}

/// The affine a [`ConstraintPose`] produces, about the node's own anchor.
///
/// Mirrors `hierarchy::node_affine`; kept here because the pose is a candidate
/// that is not on any node yet.
fn pose_affine(pose: &ConstraintPose, anchor: (f64, f64)) -> Affine {
    use crate::hierarchy::multiply;

    let (sin, cos) = pose.rotation.sin_cos();
    let rotate_scale: Affine = [
        cos * pose.scale_x,
        sin * pose.scale_x,
        -sin * pose.scale_y,
        cos * pose.scale_y,
        0.0,
        0.0,
    ];
    let to_anchor: Affine = [1.0, 0.0, 0.0, 1.0, pose.x + anchor.0, pose.y + anchor.1];
    let from_anchor: Affine = [1.0, 0.0, 0.0, 1.0, -anchor.0, -anchor.1];
    multiply(multiply(to_anchor, rotate_scale), from_anchor)
}

/// The node's anchor, which a constraint never drives: a constrained shape
/// still rotates about the pivot its author set.
fn authored_anchor(node: &format::Node<'_>, overrides: Option<&NodeOverrides>) -> (f64, f64) {
    let transform = node.transform();
    let base = (
        transform.map(|t| t.anchor_x() as f64).unwrap_or(0.0),
        transform.map(|t| t.anchor_y() as f64).unwrap_or(0.0),
    );
    let Some(values) = overrides else { return base };
    (
        values.scalar("anchorX").unwrap_or(base.0),
        values.scalar("anchorY").unwrap_or(base.1),
    )
}

/// The source's rotation and effective size, for a Copy constraint.
///
/// A source that is itself constrained is read from the solved state, so a
/// chain copies what the previous link actually did rather than what it
/// authored.
fn source_basis(
    source_id: &str,
    solved: &HashMap<String, SolvedWorld>,
    sources: &Sources<'_, '_>,
) -> Option<SourceBasis> {
    let intrinsic = sources.nodes.get(source_id).and_then(node_size);

    if let Some(state) = solved.get(source_id) {
        return Some(SourceBasis {
            rotation: state.rotation,
            size: effective_size(state.scale, intrinsic),
        });
    }

    let affine = sources
        .bones
        .get(source_id)
        .or_else(|| sources.world.get(source_id))?;
    Some(SourceBasis {
        rotation: rotation_of(*affine),
        size: effective_size((scale_x_of(*affine), scale_y_of(*affine)), intrinsic),
    })
}

/// Scale times intrinsic size, or bare scale when the node has no size the
/// solver can measure — the same fallback `_sourceSizeOf` takes.
fn effective_size(scale: (f64, f64), intrinsic: Option<(f64, f64)>) -> (f64, f64) {
    match intrinsic {
        Some((w, h)) => (scale.0 * w, scale.1 * h),
        None => scale,
    }
}

/// The centre of a node's world-space bounds.
///
/// Corners are `(0,0)..(w,h)` in the node's own space — its scale and rotation
/// are already in the affine — matching `worldRectFor` in the editor.
fn node_world_centre(node: &format::Node<'_>, affine: Affine) -> Option<(f64, f64)> {
    // The node's real bounds, not `0..w`: a path's anchors do not have to
    // start at its origin, and assuming they did put its centre in the wrong
    // place.
    let (min_x, min_y, max_x, max_y) = node_bounds(node)?;
    let corners = [
        apply_point(affine, min_x, min_y),
        apply_point(affine, max_x, min_y),
        apply_point(affine, max_x, max_y),
        apply_point(affine, min_x, max_y),
    ];
    let min_x = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let max_x = corners.iter().map(|c| c.0).fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let max_y = corners.iter().map(|c| c.1).fold(f64::NEG_INFINITY, f64::max);
    Some(((min_x + max_x) / 2.0, (min_y + max_y) / 2.0))
}

/// The node's own bounds, in its own space.
///
/// A constraint pointed at a path used to find no size at all, so the node was
/// never even seeded and the constraint silently did nothing (issue #111
/// parity audit §5.8). Paths and compound paths now measure their anchors.
fn node_size(node: &format::Node<'_>) -> Option<(f64, f64)> {
    let (min_x, min_y, max_x, max_y) = node_bounds(node)?;
    Some((max_x - min_x, max_y - min_y))
}

/// `(min_x, min_y, max_x, max_y)` in the node's own space.
fn node_bounds(node: &format::Node<'_>) -> Option<(f64, f64, f64, f64)> {
    if let Some(shape) = node.component_as_shape() {
        if let Some(rect) = shape.geometry_as_rectangle() {
            return Some((0.0, 0.0, rect.width() as f64, rect.height() as f64));
        }
        if let Some(ellipse) = shape.geometry_as_ellipse() {
            return Some((0.0, 0.0, ellipse.width() as f64, ellipse.height() as f64));
        }
        return None;
    }

    if let Some(path) = node.component_as_path() {
        let points = path
            .nodes_v2()?
            .iter()
            .map(RuntimePathPoint::from_path_node_v2)
            .collect::<Vec<_>>();
        return curve_bounds(&[RenderPathContour {
            closed: path.closed(),
            points: points.into_iter().map(RenderPathPoint::from_runtime).collect(),
        }]);
    }

    if let Some(compound) = node.component_as_compound_path() {
        let contours = compound
            .contours()?
            .iter()
            .filter_map(|contour| {
                let points = contour
                    .nodes_v2()?
                    .iter()
                    .map(RuntimePathPoint::from_path_node_v2)
                    .collect::<Vec<_>>();
                Some(RenderPathContour {
                    closed: contour.closed(),
                    points: points.into_iter().map(RenderPathPoint::from_runtime).collect(),
                })
            })
            .collect::<Vec<_>>();
        return curve_bounds(&contours);
    }

    None
}

/// The tight bounds of the curves a path actually draws.
///
/// Not the anchors and not the control hull. The editor measures the drawn
/// curve (`selectionWorldBounds`), and a handle reaches well outside it — for
/// the parity fixture's blob the hull says 440 where the curve reaches 422.5.
/// A constraint aimed at a path has to agree with the editor about where that
/// path's centre is, so the extrema are solved rather than approximated.
fn curve_bounds(contours: &[RenderPathContour]) -> Option<(f64, f64, f64, f64)> {
    let mut min = (f64::INFINITY, f64::INFINITY);
    let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut any = false;

    let mut include = |x: f64, y: f64| {
        any = true;
        min.0 = min.0.min(x);
        min.1 = min.1.min(y);
        max.0 = max.0.max(x);
        max.1 = max.1.max(y);
    };

    for contour in crate::render::path_segments(contours) {
        for segment in contour.segments {
            include(segment.start.0, segment.start.1);
            include(segment.end.0, segment.end.1);
            for t in cubic_extrema(segment.start.0, segment.control1.0, segment.control2.0, segment.end.0)
                .into_iter()
                .chain(cubic_extrema(
                    segment.start.1,
                    segment.control1.1,
                    segment.control2.1,
                    segment.end.1,
                ))
            {
                let (x, y) = cubic_at(&segment, t);
                include(x, y);
            }
        }
    }

    any.then(|| (min.0, min.1, max.0, max.1))
}

/// The `t` values in (0,1) where a cubic turns around on one axis.
///
/// The derivative of a cubic Bézier is a quadratic; these are its roots.
fn cubic_extrema(p0: f64, p1: f64, p2: f64, p3: f64) -> Vec<f64> {
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;

    let mut roots = Vec::new();
    let mut push = |t: f64| {
        if t > 0.0 && t < 1.0 {
            roots.push(t);
        }
    };

    // Degenerate to a line when the quadratic term vanishes, rather than
    // dividing by zero.
    if a.abs() < 1e-12 {
        if b.abs() > 1e-12 {
            push(-c / b);
        }
        return roots;
    }

    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return roots;
    }
    let root = discriminant.sqrt();
    push((-b + root) / (2.0 * a));
    push((-b - root) / (2.0 * a));
    roots
}

fn cubic_at(segment: &crate::render::CubicSegment, t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (
        a * segment.start.0 + b * segment.control1.0 + c * segment.control2.0 + d * segment.end.0,
        a * segment.start.1 + b * segment.control1.1 + c * segment.control2.1 + d * segment.end.1,
    )
}

fn authored_transform(
    node: &format::Node<'_>,
    overrides: Option<&NodeOverrides>,
) -> (f64, f64, f64, f64, f64) {
    let transform = node.transform();
    let base = (
        transform.map(|t| t.x() as f64).unwrap_or(0.0),
        transform.map(|t| t.y() as f64).unwrap_or(0.0),
        transform.map(|t| t.rotation() as f64).unwrap_or(0.0),
        transform.map(|t| t.scale_x() as f64).unwrap_or(1.0),
        transform.map(|t| t.scale_y() as f64).unwrap_or(1.0),
    );
    let Some(values) = overrides else { return base };
    (
        values.scalar("positionX").unwrap_or(base.0),
        values.scalar("positionY").unwrap_or(base.1),
        values.scalar("rotation").unwrap_or(base.2),
        values.scalar("scaleX").unwrap_or(base.3),
        values.scalar("scaleY").unwrap_or(base.4),
    )
}

fn rotation_of(m: Affine) -> f64 {
    m[1].atan2(m[0])
}

fn scale_x_of(m: Affine) -> f64 {
    (m[0] * m[0] + m[1] * m[1]).sqrt()
}

fn scale_y_of(m: Affine) -> f64 {
    (m[2] * m[2] + m[3] * m[3]).sqrt()
}

fn normalize_angle(angle: f64) -> f64 {
    let two_pi = std::f64::consts::PI * 2.0;
    let mut a = angle % two_pi;
    if a > std::f64::consts::PI {
        a -= two_pi;
    } else if a < -std::f64::consts::PI {
        a += two_pi;
    }
    a
}

fn lerp(from: f64, to: f64, t: f64) -> f64 {
    from + (to - from) * t
}
