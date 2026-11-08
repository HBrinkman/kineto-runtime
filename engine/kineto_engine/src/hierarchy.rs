//! Group nesting: which node contains which, and what that does to space.
//!
//! Groups have always been exported with their `child_ids`, and nothing here
//! read them — every node, target and hit test used a node's own transform, so
//! a group with a transform rendered its contents in the wrong place and left
//! the rig controls inside it behind (issue #74).
//!
//! One composition, used by everything that needs world space: the renderer,
//! the hit test, and the targets a group carries.

use std::collections::HashMap;

use crate::animation::NodeOverrides;
use crate::format;

/// A 2D affine transform as `[a, b, c, d, e, f]`, mapping
/// `(x, y) → (a·x + c·y + e, b·x + d·y + f)` — the same column order Skia,
/// Canvas2D and Vello use, so a consumer can hand it straight over.
pub type Affine = [f64; 6];

pub const IDENTITY: Affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

pub fn multiply(outer: Affine, inner: Affine) -> Affine {
    [
        outer[0] * inner[0] + outer[2] * inner[1],
        outer[1] * inner[0] + outer[3] * inner[1],
        outer[0] * inner[2] + outer[2] * inner[3],
        outer[1] * inner[2] + outer[3] * inner[3],
        outer[0] * inner[4] + outer[2] * inner[5] + outer[4],
        outer[1] * inner[4] + outer[3] * inner[5] + outer[5],
    ]
}

pub fn apply_point(m: Affine, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// `None` for a degenerate transform — a group scaled to nothing has no
/// inverse, and a target inside one must survive that rather than vanish.
pub fn invert(m: Affine) -> Option<Affine> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < f64::EPSILON {
        return None;
    }
    let inv = 1.0 / det;
    Some([
        m[3] * inv,
        -m[1] * inv,
        -m[2] * inv,
        m[0] * inv,
        (m[2] * m[5] - m[3] * m[4]) * inv,
        (m[1] * m[4] - m[0] * m[5]) * inv,
    ])
}

/// The affine a node's own transform describes, with this frame's overrides
/// applied. Anchor first, exactly like the editor and the Vello path.
pub fn node_affine(node: format::Node<'_>, overrides: Option<&NodeOverrides>) -> Affine {
    let transform = node.transform();
    let scalar = |key: &str, fallback: f64| {
        overrides
            .and_then(|values| values.scalar(key))
            .unwrap_or(fallback)
    };
    let base = |get: fn(&format::Transform<'_>) -> f32, fallback: f64| {
        transform.as_ref().map(get).map(|v| v as f64).unwrap_or(fallback)
    };

    let x = scalar("positionX", base(|t| t.x(), 0.0));
    let y = scalar("positionY", base(|t| t.y(), 0.0));
    let rotation = scalar("rotation", base(|t| t.rotation(), 0.0));
    let scale_x = scalar("scaleX", base(|t| t.scale_x(), 1.0));
    let scale_y = scalar("scaleY", base(|t| t.scale_y(), 1.0));
    let anchor_x = scalar("anchorX", base(|t| t.anchor_x(), 0.0));
    let anchor_y = scalar("anchorY", base(|t| t.anchor_y(), 0.0));

    let (sin, cos) = rotation.sin_cos();
    let rotate_scale: Affine = [
        cos * scale_x,
        sin * scale_x,
        -sin * scale_y,
        cos * scale_y,
        0.0,
        0.0,
    ];
    let to_anchor: Affine = [1.0, 0.0, 0.0, 1.0, x + anchor_x, y + anchor_y];
    let from_anchor: Affine = [1.0, 0.0, 0.0, 1.0, -anchor_x, -anchor_y];
    multiply(multiply(to_anchor, rotate_scale), from_anchor)
}

/// Child id → the id of the Group node that contains it.
pub fn parent_map(document: &format::Document<'_>) -> HashMap<String, String> {
    let mut parents = HashMap::new();
    for node in document.nodes().unwrap_or_default() {
        let Some(group) = node.component_as_group() else {
            continue;
        };
        for child in group.child_ids().unwrap_or_default() {
            parents.insert(child.to_owned(), node.id().to_owned());
        }
    }
    parents
}

/// Composed world affines, one per node, ancestors applied outermost-first.
///
/// A cycle in the child lists (which a hand-written file could contain) stops
/// the walk rather than hanging the runtime.
pub fn world_affines(
    document: &format::Document<'_>,
    overrides: &HashMap<String, NodeOverrides>,
) -> HashMap<String, Affine> {
    world_affines_attached(document, overrides, &HashMap::new())
}

/// Where each Attach-to-Bone object sits this frame: `boneWorldNow · offset`
/// (`flattenForRender` in the editor). Keyed by the attached node.
///
/// `fk` holds the posed bone world matrices. A binding whose bone did not
/// pose is left out, so its object falls back to its own transform.
pub fn attach_worlds(
    document: &format::Document<'_>,
    fk: &HashMap<String, crate::bones::fk_solver::Matrix4>,
) -> HashMap<String, Affine> {
    let mut out = HashMap::new();
    for binding in document.attach_bindings().unwrap_or_default() {
        let Some(m) = fk.get(binding.bone_id()) else {
            continue;
        };
        let bone: Affine = [m[0], m[1], m[4], m[5], m[12], m[13]];
        let offset: Affine = [
            binding.a() as f64,
            binding.b() as f64,
            binding.c() as f64,
            binding.d() as f64,
            binding.tx() as f64,
            binding.ty() as f64,
        ];
        out.insert(binding.target_id().to_owned(), multiply(bone, offset));
    }
    out
}

/// [`world_affines`], with attached objects placed by their bone.
///
/// An attached node's world *replaces* its parent chain and its own transform,
/// as in the editor; its children compose under it as usual.
pub fn world_affines_attached(
    document: &format::Document<'_>,
    overrides: &HashMap<String, NodeOverrides>,
    attached: &HashMap<String, Affine>,
) -> HashMap<String, Affine> {
    let parents = parent_map(document);
    let nodes: HashMap<String, format::Node<'_>> = document
        .nodes()
        .unwrap_or_default()
        .iter()
        .map(|node| (node.id().to_owned(), node))
        .collect();

    let mut world = HashMap::with_capacity(nodes.len());
    for id in nodes.keys() {
        let mut chain = Vec::new();
        let mut base = IDENTITY;
        let mut current = Some(id.clone());
        let mut guard = 0;
        while let Some(node_id) = current {
            if guard > nodes.len() {
                break;
            }
            guard += 1;
            let Some(node) = nodes.get(&node_id) else {
                break;
            };
            if let Some(world) = attached.get(&node_id) {
                base = *world;
                break;
            }
            chain.push(node_affine(*node, overrides.get(&node_id)));
            current = parents.get(&node_id).cloned();
        }

        let mut composed = base;
        for affine in chain.iter().rev() {
            composed = multiply(composed, *affine);
        }
        world.insert(id.clone(), composed);
    }
    world
}

/// The motion a group has this frame, relative to the pose it was authored in:
/// `world_now ∘ world_authored⁻¹`.
///
/// This is what carries a target that sits in a group. Positions stay authored
/// space — the same decision `ik_target_group_carry.dart` documents on the
/// editor side, so keyframes written against them keep their meaning.
pub fn group_carry_deltas(
    document: &format::Document<'_>,
    overrides: &HashMap<String, NodeOverrides>,
) -> HashMap<String, Affine> {
    if overrides.is_empty() {
        return HashMap::new();
    }
    let authored = world_affines(document, &HashMap::new());
    let posed = world_affines(document, overrides);

    let mut deltas = HashMap::new();
    for (id, posed_affine) in posed {
        let Some(authored_affine) = authored.get(&id) else {
            continue;
        };
        let Some(inverse) = invert(*authored_affine) else {
            continue;
        };
        let delta = multiply(posed_affine, inverse);
        if delta != IDENTITY {
            deltas.insert(id, delta);
        }
    }
    deltas
}
