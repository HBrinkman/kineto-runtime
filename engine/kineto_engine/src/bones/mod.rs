pub mod constraints;
pub mod fk_solver;
pub mod ik_evaluator;
pub mod skeletal_geometry;

pub use fk_solver::{bone_world_tip, bones_from_document, solve_fk, Matrix4, Point2, RuntimeBone};
pub use ik_evaluator::{
    evaluate_ik, evaluate_skeletal, ik_chains_from_document, ik_targets_from_document,
    RuntimeIkChain, RuntimeIkSolverKind, RuntimeIkTarget,
};
pub use skeletal_geometry::{
    apply_skeletal_transforms, matrix4_from_affine, RuntimeBoneWeight, RuntimePathPoint,
    IDENTITY_MATRIX4,
};
