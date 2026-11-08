use std::collections::HashMap;

use crate::format;

pub type Matrix4 = [f64; 16];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeBone {
    pub id: String,
    pub parent_id: Option<String>,
    pub local_position: Point2,
    pub local_rotation: f64,
    pub length: f64,
    pub min_rotation: f64,
    pub max_rotation: f64,
    /// The Group node that carries this bone through the Scene, on root bones
    /// only (issue #74). The chain defines a bone's space; a group carries the
    /// whole rig.
    pub group_id: Option<String>,
}

impl RuntimeBone {
    pub fn from_flatbuffer(bone: format::Bone<'_>) -> Self {
        Self {
            id: bone.id().to_owned(),
            parent_id: bone.parent_id().map(str::to_owned),
            local_position: Point2 {
                x: bone.local_x() as f64,
                y: bone.local_y() as f64,
            },
            local_rotation: bone.local_rotation() as f64,
            length: bone.length() as f64,
            min_rotation: bone.min_rotation() as f64,
            max_rotation: bone.max_rotation() as f64,
            group_id: bone.group_id().map(str::to_owned),
        }
    }
}

pub fn bones_from_document(document: &format::Document<'_>) -> Vec<RuntimeBone> {
    document
        .bones()
        .unwrap_or_default()
        .iter()
        .map(RuntimeBone::from_flatbuffer)
        .collect()
}

/// Pure forward-kinematics solver.
///
/// Returns a world-space column-major matrix for every bone in `bones`.
/// Mirrors `lib/src/domain/bones/fk_solver.dart`.
pub fn solve_fk(bones: &[RuntimeBone]) -> HashMap<String, Matrix4> {
    let by_id: HashMap<&str, &RuntimeBone> =
        bones.iter().map(|bone| (bone.id.as_str(), bone)).collect();
    let mut result = HashMap::new();

    for bone in bones {
        world_of(&bone.id, &by_id, &mut result);
    }

    result
}

fn world_of(
    id: &str,
    by_id: &HashMap<&str, &RuntimeBone>,
    result: &mut HashMap<String, Matrix4>,
) -> Matrix4 {
    if let Some(matrix) = result.get(id) {
        return *matrix;
    }

    let Some(bone) = by_id.get(id) else {
        return identity();
    };

    let parent_matrix = bone
        .parent_id
        .as_deref()
        .filter(|parent_id| by_id.contains_key(parent_id))
        .map(|parent_id| world_of(parent_id, by_id, result))
        .unwrap_or_else(identity);

    let clamped = bone
        .local_rotation
        .clamp(bone.min_rotation, bone.max_rotation);
    let cos_r = clamped.cos();
    let sin_r = clamped.sin();

    let local = [
        cos_r,
        sin_r,
        0.0,
        0.0,
        -sin_r,
        cos_r,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        bone.local_position.x,
        bone.local_position.y,
        0.0,
        1.0,
    ];

    let world = multiply(parent_matrix, local);
    result.insert(id.to_owned(), world);
    world
}

pub fn bone_world_tip(bone: &RuntimeBone, world_matrix: Matrix4) -> Point2 {
    Point2 {
        x: world_matrix[4] * bone.length + world_matrix[12],
        y: world_matrix[5] * bone.length + world_matrix[13],
    }
}

pub(crate) fn identity() -> Matrix4 {
    [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]
}

/// Column-major 4x4 matrix multiply: result = a * b.
pub(crate) fn multiply(a: Matrix4, b: Matrix4) -> Matrix4 {
    let mut c = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            c[col * 4 + row] = sum;
        }
    }
    c
}

pub(crate) fn transform(m: Matrix4, v: [f64; 4]) -> [f64; 4] {
    [
        m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12] * v[3],
        m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13] * v[3],
        m[2] * v[0] + m[6] * v[1] + m[10] * v[2] + m[14] * v[3],
        m[3] * v[0] + m[7] * v[1] + m[11] * v[2] + m[15] * v[3],
    ]
}

pub(crate) fn invert_affine_4x4(m: Matrix4) -> Option<Matrix4> {
    let det = m[0] * (m[5] * m[10] - m[6] * m[9]) - m[4] * (m[1] * m[10] - m[2] * m[9])
        + m[8] * (m[1] * m[6] - m[2] * m[5]);
    if det.abs() < 1e-10 {
        return None;
    }

    let inv_det = 1.0 / det;
    let mut inv = [0.0; 16];

    inv[0] = (m[5] * m[10] - m[6] * m[9]) * inv_det;
    inv[1] = (m[2] * m[9] - m[1] * m[10]) * inv_det;
    inv[2] = (m[1] * m[6] - m[2] * m[5]) * inv_det;
    inv[4] = (m[6] * m[8] - m[4] * m[10]) * inv_det;
    inv[5] = (m[0] * m[10] - m[2] * m[8]) * inv_det;
    inv[6] = (m[2] * m[4] - m[0] * m[6]) * inv_det;
    inv[8] = (m[4] * m[9] - m[5] * m[8]) * inv_det;
    inv[9] = (m[1] * m[8] - m[0] * m[9]) * inv_det;
    inv[10] = (m[0] * m[5] - m[1] * m[4]) * inv_det;

    inv[12] = -(inv[0] * m[12] + inv[4] * m[13] + inv[8] * m[14]);
    inv[13] = -(inv[1] * m[12] + inv[5] * m[13] + inv[9] * m[14]);
    inv[14] = -(inv[2] * m[12] + inv[6] * m[13] + inv[10] * m[14]);
    inv[15] = 1.0;

    Some(inv)
}
