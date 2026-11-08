#[allow(dead_code, unused_imports, non_snake_case, clippy::all)]
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/flatbuffers/mod.rs"));
}

pub use flatbuffers::InvalidFlatbuffer;
pub use generated::kineto::{
    root_as_document, AnimatedPropertyKind, Animation, Bone, BoneWeight, Color, Constraint,
    ConstraintKind, Document, IkChain,
    IkSolverKind, IkTarget, InputBinding, InputKind, KFKeyframe, KFTrack,
    KeyframeInterpolationKind, ListenerActionKind, ListenerEventKind, LoopMode, Node, SMLayer,
    SMListener, SMState, SMTransition, StateMachine, StateMachineInput, Transform,
    TransitionCondition, TransitionConditionKind, DOCUMENT_IDENTIFIER,
};

/// Parse `.kvk` bytes into a `Document`. Zero-copy — caller owns the byte slice.
pub fn load(bytes: &[u8]) -> Result<Document<'_>, InvalidFlatbuffer> {
    root_as_document(bytes)
}
