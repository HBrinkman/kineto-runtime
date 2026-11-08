use std::collections::HashMap;
use std::f64::consts::PI;

use kineto_engine::bones::{
    apply_skeletal_transforms, bone_world_tip, bones_from_document, evaluate_ik, evaluate_skeletal,
    ik_chains_from_document, ik_targets_from_document, solve_fk, Point2, RuntimeBone,
    RuntimeBoneWeight, RuntimeIkChain, RuntimeIkSolverKind, RuntimeIkTarget, RuntimePathPoint,
    IDENTITY_MATRIX4,
};
use kineto_engine::format::generated::kineto::{
    finish_document_buffer, root_as_document, AnimatedPropertyKind, Animation, AnimationArgs, Bone,
    BoneArgs, BoneWeight, BoneWeightArgs, Document, DocumentArgs, IkChain, IkChainArgs,
    IkSolverKind, IkTarget, IkTargetArgs, KFKeyframe, KFKeyframeArgs, KFTrack, KFTrackArgs,
    PathNodeFBV2, PathNodeFBV2Args,
};

const DEG: f64 = PI / 180.0;

fn bone(id: &str) -> RuntimeBone {
    RuntimeBone {
        id: id.to_owned(),
        parent_id: None,
        local_position: Point2::ZERO,
        local_rotation: 0.0,
        length: 50.0,
        min_rotation: -PI,
        max_rotation: PI,
        group_id: None,
    }
}

fn rotation_of(m: [f64; 16]) -> f64 {
    m[1].atan2(m[0])
}

fn assert_close(actual: f64, expected: f64, epsilon: f64) {
    assert!(
        (actual - expected).abs() <= epsilon,
        "expected {actual} to be within {epsilon} of {expected}"
    );
}

#[test]
fn places_root_bone_at_local_position() {
    let root = RuntimeBone {
        local_position: Point2 { x: 100.0, y: 200.0 },
        ..bone("root")
    };

    let fk = solve_fk(&[root]);

    let m = fk.get("root").expect("root FK missing");
    assert_close(m[12], 100.0, 0.01);
    assert_close(m[13], 200.0, 0.01);
}

#[test]
fn applies_local_rotation_to_root_bone() {
    let root = RuntimeBone {
        local_rotation: 45.0 * DEG,
        ..bone("root")
    };

    let fk = solve_fk(&[root]);

    assert_close(
        rotation_of(*fk.get("root").expect("root FK missing")),
        45.0 * DEG,
        0.001,
    );
}

#[test]
fn cascades_parent_rotation_to_child() {
    let parent = RuntimeBone {
        local_rotation: 30.0 * DEG,
        length: 100.0,
        ..bone("parent")
    };
    let child = RuntimeBone {
        id: "child".to_owned(),
        parent_id: Some("parent".to_owned()),
        local_position: Point2 { x: 0.0, y: 100.0 },
        ..bone("child")
    };

    let fk = solve_fk(&[parent, child]);

    assert_close(
        rotation_of(*fk.get("child").expect("child FK missing")),
        30.0 * DEG,
        0.001,
    );
}

#[test]
fn cascades_through_three_bone_chain() {
    let a = RuntimeBone {
        local_rotation: 10.0 * DEG,
        length: 50.0,
        ..bone("a")
    };
    let b = RuntimeBone {
        id: "b".to_owned(),
        parent_id: Some("a".to_owned()),
        local_position: Point2 { x: 0.0, y: 50.0 },
        local_rotation: 20.0 * DEG,
        length: 50.0,
        ..bone("b")
    };
    let c = RuntimeBone {
        id: "c".to_owned(),
        parent_id: Some("b".to_owned()),
        local_position: Point2 { x: 0.0, y: 50.0 },
        ..bone("c")
    };

    let fk = solve_fk(&[a, b, c]);

    assert_close(
        rotation_of(*fk.get("c").expect("c FK missing")),
        30.0 * DEG,
        0.01,
    );
}

#[test]
fn clamps_local_rotation_to_limits() {
    let max_limited = RuntimeBone {
        local_rotation: 2.0,
        max_rotation: 1.5,
        ..bone("max")
    };
    let min_limited = RuntimeBone {
        local_rotation: -2.0,
        min_rotation: -1.0,
        ..bone("min")
    };

    let fk = solve_fk(&[max_limited, min_limited]);

    assert_close(
        rotation_of(*fk.get("max").expect("max FK missing")),
        1.5,
        0.001,
    );
    assert_close(
        rotation_of(*fk.get("min").expect("min FK missing")),
        -1.0,
        0.001,
    );
}

#[test]
fn treats_missing_parent_as_root() {
    let root = RuntimeBone {
        parent_id: Some("missing".to_owned()),
        local_position: Point2 { x: 50.0, y: 50.0 },
        ..bone("root")
    };

    let fk = solve_fk(&[root]);

    let m = fk.get("root").expect("root FK missing");
    assert_close(m[12], 50.0, 0.01);
    assert_close(m[13], 50.0, 0.01);
}

#[test]
fn handles_unsorted_bone_list() {
    let child = RuntimeBone {
        id: "child".to_owned(),
        parent_id: Some("parent".to_owned()),
        local_position: Point2 { x: 0.0, y: 40.0 },
        ..bone("child")
    };
    let parent = RuntimeBone {
        id: "parent".to_owned(),
        local_rotation: 45.0 * DEG,
        length: 40.0,
        ..bone("parent")
    };

    let fk = solve_fk(&[child, parent]);

    assert_close(
        rotation_of(*fk.get("child").expect("child FK missing")),
        45.0 * DEG,
        0.001,
    );
}

#[test]
fn returns_tip_along_local_y_axis() {
    let root = RuntimeBone {
        local_position: Point2 { x: 100.0, y: 200.0 },
        length: 60.0,
        ..bone("root")
    };

    let fk = solve_fk(std::slice::from_ref(&root));
    let tip = bone_world_tip(&root, *fk.get("root").expect("root FK missing"));

    assert_close(tip.x, 100.0, 0.01);
    assert_close(tip.y, 260.0, 0.01);
}

#[test]
fn skeletal_transform_leaves_unbound_vertex_unchanged() {
    let point = RuntimePathPoint::unbound(
        Point2 { x: 10.0, y: 20.0 },
        Point2 { x: 5.0, y: 20.0 },
        Point2 { x: 15.0, y: 20.0 },
    );
    let fk = solve_fk(&[bone("b0")]);

    let result = apply_skeletal_transforms(&[point.clone()], &fk, &fk, IDENTITY_MATRIX4, IDENTITY_MATRIX4);

    assert_eq!(result.first().expect("point missing"), &point);
}

#[test]
fn skeletal_transform_moves_bound_vertex_with_bone_rotation() {
    let point = RuntimePathPoint {
        anchor: Point2 { x: 0.0, y: 100.0 },
        in_handle: Point2 { x: 0.0, y: 100.0 },
        out_handle: Point2 { x: 0.0, y: 100.0 },
        bone_id: Some("b0".to_owned()),
        bone_influence: 1.0,
        weights: Vec::new(),
    };
    let rest_fk = solve_fk(&[RuntimeBone {
        length: 100.0,
        ..bone("b0")
    }]);
    let current_fk = solve_fk(&[RuntimeBone {
        local_rotation: PI / 2.0,
        length: 100.0,
        ..bone("b0")
    }]);

    let result = apply_skeletal_transforms(&[point], &current_fk, &rest_fk, IDENTITY_MATRIX4, IDENTITY_MATRIX4);
    let anchor = result.first().expect("point missing").anchor;

    assert_close(anchor.x, -100.0, 1e-9);
    assert_close(anchor.y, 0.0, 1e-9);
}

#[test]
fn skeletal_transform_leaves_unknown_bone_vertex_unchanged() {
    let point = RuntimePathPoint {
        anchor: Point2 { x: 5.0, y: 5.0 },
        in_handle: Point2 { x: 5.0, y: 5.0 },
        out_handle: Point2 { x: 5.0, y: 5.0 },
        bone_id: Some("missing".to_owned()),
        bone_influence: 1.0,
        weights: Vec::new(),
    };
    let fk = solve_fk(&[bone("b0")]);

    let result = apply_skeletal_transforms(&[point.clone()], &fk, &fk, IDENTITY_MATRIX4, IDENTITY_MATRIX4);

    assert_eq!(result.first().expect("point missing"), &point);
}

#[test]
fn weighted_skinning_blends_positions_from_two_bones() {
    let b_a = RuntimeBone {
        length: 100.0,
        ..bone("bA")
    };
    let b_b = RuntimeBone {
        local_position: Point2 { x: 200.0, y: 0.0 },
        length: 100.0,
        ..bone("bB")
    };
    let rest_fk = solve_fk(&[b_a.clone(), b_b.clone()]);

    let current_fk = solve_fk(&[
        RuntimeBone {
            local_rotation: PI / 2.0,
            ..b_a
        },
        b_b,
    ]);

    let point = RuntimePathPoint {
        anchor: Point2 { x: 0.0, y: 50.0 },
        in_handle: Point2 { x: 0.0, y: 10.0 },
        out_handle: Point2 { x: 0.0, y: -10.0 },
        bone_id: None,
        bone_influence: 1.0,
        weights: vec![
            RuntimeBoneWeight {
                bone_id: "bA".to_owned(),
                weight: 0.5,
            },
            RuntimeBoneWeight {
                bone_id: "bB".to_owned(),
                weight: 0.5,
            },
        ],
    };

    let result = apply_skeletal_transforms(&[point], &current_fk, &rest_fk, IDENTITY_MATRIX4, IDENTITY_MATRIX4);
    let moved = result.first().expect("point missing");

    assert_close(moved.anchor.x, -25.0, 1e-9);
    assert_close(moved.anchor.y, 25.0, 1e-9);
    assert_close(moved.in_handle.x, -5.0, 1e-9);
    assert_close(moved.in_handle.y, 5.0, 1e-9);
}

#[test]
fn weighted_skinning_renormalizes_valid_weights() {
    let rest_fk = solve_fk(&[RuntimeBone {
        length: 100.0,
        ..bone("b0")
    }]);
    let current_fk = solve_fk(&[RuntimeBone {
        local_rotation: PI / 2.0,
        length: 100.0,
        ..bone("b0")
    }]);
    let point = RuntimePathPoint {
        anchor: Point2 { x: 0.0, y: 50.0 },
        in_handle: Point2::ZERO,
        out_handle: Point2::ZERO,
        bone_id: None,
        bone_influence: 1.0,
        weights: vec![
            RuntimeBoneWeight {
                bone_id: "b0".to_owned(),
                weight: 0.25,
            },
            RuntimeBoneWeight {
                bone_id: "missing".to_owned(),
                weight: 0.75,
            },
        ],
    };

    let result = apply_skeletal_transforms(&[point], &current_fk, &rest_fk, IDENTITY_MATRIX4, IDENTITY_MATRIX4);
    let moved = result.first().expect("point missing");

    assert_close(moved.anchor.x, -50.0, 1e-9);
    assert_close(moved.anchor.y, 0.0, 1e-9);
}

#[test]
fn evaluate_ik_returns_no_overrides_for_unknown_target() {
    let bones = vec![
        RuntimeBone {
            length: 100.0,
            ..bone("b0")
        },
        RuntimeBone {
            parent_id: Some("b0".to_owned()),
            local_position: Point2 { x: 0.0, y: 100.0 },
            length: 100.0,
            ..bone("b1")
        },
    ];
    let fk = solve_fk(&bones);
    let chain = RuntimeIkChain {
        id: "c0".to_owned(),
        root_bone_id: "b0".to_owned(),
        effector_bone_id: "b1".to_owned(),
        target_id: "t0".to_owned(),
        solver_kind: RuntimeIkSolverKind::Fabrik,
        bend_direction: 0,
    };

    let overrides = evaluate_ik(
        &bones,
        &[chain],
        &[RuntimeIkTarget {
            id: "ghost".to_owned(),
            position: Point2 { x: 100.0, y: 0.0 },
            extent: (0.0, 0.0),
            parent_id: None,
            interactive: false,
        }],
        &fk,
    );

    assert!(overrides.is_empty());
}

#[test]
fn evaluate_ik_fabrik_moves_effector_tip_toward_target() {
    let bones = two_bone_chain();
    let fk = solve_fk(&bones);
    let chain = two_bone_chain_descriptor(RuntimeIkSolverKind::Fabrik);

    let overrides = evaluate_ik(
        &bones,
        &[chain],
        &[RuntimeIkTarget {
            id: "t0".to_owned(),
            position: Point2 { x: 100.0, y: 100.0 },
            extent: (0.0, 0.0),
            parent_id: None,
            interactive: false,
        }],
        &fk,
    );

    let solved = apply_rotation_overrides(&bones, &overrides);
    let solved_fk = solve_fk(&solved);
    let tip = bone_world_tip(&solved[1], *solved_fk.get("b1").expect("b1 FK missing"));

    assert_close(tip.x, 100.0, 5.0);
    assert_close(tip.y, 100.0, 5.0);
}

#[test]
fn evaluate_ik_two_bone_moves_effector_tip_toward_target() {
    let bones = two_bone_chain();
    let fk = solve_fk(&bones);
    let chain = two_bone_chain_descriptor(RuntimeIkSolverKind::TwoBone);

    let overrides = evaluate_ik(
        &bones,
        &[chain],
        &[RuntimeIkTarget {
            id: "t0".to_owned(),
            position: Point2 { x: 100.0, y: 100.0 },
            extent: (0.0, 0.0),
            parent_id: None,
            interactive: false,
        }],
        &fk,
    );

    let solved = apply_rotation_overrides(&bones, &overrides);
    let solved_fk = solve_fk(&solved);
    let tip = bone_world_tip(&solved[1], *solved_fk.get("b1").expect("b1 FK missing"));

    assert_close(tip.x, 100.0, 5.0);
    assert_close(tip.y, 100.0, 5.0);
}

#[test]
fn evaluate_skeletal_matches_fk_when_no_chains_exist() {
    let bones = two_bone_chain();

    let from_skeletal = evaluate_skeletal(&bones, &[], &[]);
    let from_fk = solve_fk(&bones);

    assert_close(
        from_skeletal.get("b0").expect("b0 skeletal missing")[12],
        from_fk.get("b0").expect("b0 FK missing")[12],
        1e-9,
    );
    assert_close(
        from_skeletal.get("b1").expect("b1 skeletal missing")[12],
        from_fk.get("b1").expect("b1 FK missing")[12],
        1e-9,
    );
}

#[test]
fn evaluate_skeletal_applies_ik_chain() {
    let bones = two_bone_chain();
    let chain = two_bone_chain_descriptor(RuntimeIkSolverKind::Fabrik);
    let target = RuntimeIkTarget {
            id: "t0".to_owned(),
        position: Point2 { x: 100.0, y: 100.0 },
            extent: (0.0, 0.0),
            parent_id: None,
            interactive: false,
        };

    let fk = evaluate_skeletal(&bones, &[chain], &[target]);
    let tip = bone_world_tip(&bones[1], *fk.get("b1").expect("b1 FK missing"));
    let dist = ((tip.x - 100.0).powi(2) + (tip.y - 100.0).powi(2)).sqrt();

    assert!(dist < 10.0, "tip should be near target, got {tip:?}");
}

#[test]
fn converts_bones_and_ik_from_flatbuffer_document() {
    let bytes = build_bones_ik_document();
    let document = root_as_document(&bytes).expect("document should parse");

    let bones = bones_from_document(&document);
    let chains = ik_chains_from_document(&document);
    let targets = ik_targets_from_document(&document);

    assert_eq!(bones.len(), 2);
    assert_eq!(bones[0].id, "b0");
    assert_eq!(bones[1].parent_id.as_deref(), Some("b0"));
    assert_close(bones[1].local_position.y, 100.0, 1e-9);
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].solver_kind, RuntimeIkSolverKind::TwoBone);
    assert_eq!(targets.len(), 1);
    assert_close(targets[0].position.x, 100.0, 1e-9);

    let fk = evaluate_skeletal(&bones, &chains, &targets);
    let tip = bone_world_tip(&bones[1], *fk.get("b1").expect("b1 FK missing"));

    assert_close(tip.x, 100.0, 5.0);
    assert_close(tip.y, 100.0, 5.0);
}

#[test]
fn converts_weighted_path_node_from_flatbuffer() {
    let bytes = build_weighted_path_node();
    let node = flatbuffers::root::<PathNodeFBV2>(&bytes).expect("path node should parse");

    let point = RuntimePathPoint::from_path_node_v2(node);

    assert_close(point.anchor.x, 0.0, 1e-9);
    assert_close(point.anchor.y, 50.0, 1e-9);
    assert_eq!(point.bone_id.as_deref(), Some("legacy-bone"));
    assert_close(point.bone_influence, 0.75, 1e-9);
    assert_eq!(point.weights.len(), 2);
    assert_eq!(point.weights[0].bone_id, "b0");
    assert_close(point.weights[0].weight, 0.25, 1e-9);
    assert_eq!(point.weights[1].bone_id, "b1");
    assert_close(point.weights[1].weight, 0.75, 1e-9);
}

fn two_bone_chain() -> Vec<RuntimeBone> {
    vec![
        RuntimeBone {
            length: 100.0,
            ..bone("b0")
        },
        RuntimeBone {
            id: "b1".to_owned(),
            parent_id: Some("b0".to_owned()),
            local_position: Point2 { x: 0.0, y: 100.0 },
            length: 100.0,
            ..bone("b1")
        },
    ]
}

fn two_bone_chain_descriptor(solver_kind: RuntimeIkSolverKind) -> RuntimeIkChain {
    RuntimeIkChain {
        id: "c0".to_owned(),
        root_bone_id: "b0".to_owned(),
        effector_bone_id: "b1".to_owned(),
        target_id: "t0".to_owned(),
        solver_kind,
        bend_direction: 0,
    }
}

fn apply_rotation_overrides(
    bones: &[RuntimeBone],
    overrides: &std::collections::HashMap<String, f64>,
) -> Vec<RuntimeBone> {
    bones
        .iter()
        .map(|bone| {
            let mut solved = bone.clone();
            if let Some(rotation) = overrides.get(&bone.id) {
                solved.local_rotation = *rotation;
            }
            solved
        })
        .collect()
}

fn build_bones_ik_document() -> Vec<u8> {
    build_bones_ik_document_with(None)
}

/// The same rig, optionally with a `boneLength` track on the root bone.
fn build_bones_ik_document_with(bone_length: Option<f32>) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(2048);

    let b0_id = fbb.create_string("b0");
    let b0 = Bone::create(
        &mut fbb,
        &BoneArgs {
            id: Some(b0_id),
            length: 100.0,
            ..Default::default()
        },
    );

    let b1_id = fbb.create_string("b1");
    let b1_parent = fbb.create_string("b0");
    let b1 = Bone::create(
        &mut fbb,
        &BoneArgs {
            id: Some(b1_id),
            parent_id: Some(b1_parent),
            local_y: 100.0,
            length: 100.0,
            ..Default::default()
        },
    );
    let bones = fbb.create_vector(&[b0, b1]);

    let chain_id = fbb.create_string("c0");
    let root_id = fbb.create_string("b0");
    let effector_id = fbb.create_string("b1");
    let chain_target_id = fbb.create_string("t0");
    let chain = IkChain::create(
        &mut fbb,
        &IkChainArgs {
            id: Some(chain_id),
            root_bone_id: Some(root_id),
            effector_bone_id: Some(effector_id),
            target_id: Some(chain_target_id),
            solver_kind: IkSolverKind::TwoBone,
            bend_direction: 0,
        },
    );
    let chains = fbb.create_vector(&[chain]);

    let target_id = fbb.create_string("t0");
    let target_chain_root = fbb.create_string("b0");
    let target = IkTarget::create(
        &mut fbb,
        &IkTargetArgs {
            id: Some(target_id),
            chain_root_id: Some(target_chain_root),
            position_x: 100.0,
            position_y: 100.0,
            ..Default::default()
        },
    );
    let targets = fbb.create_vector(&[target]);

    let animations = bone_length.map(|length| {
        let mut keyframes = Vec::new();
        for (index, time) in [0.0f32, 1.0].into_iter().enumerate() {
            let kf_id = fbb.create_string(if index == 0 { "kf0" } else { "kf1" });
            keyframes.push(KFKeyframe::create(
                &mut fbb,
                &KFKeyframeArgs {
                    id: Some(kf_id),
                    time,
                    scalar_value: length,
                    ..Default::default()
                },
            ));
        }
        let keyframes = fbb.create_vector(&keyframes);
        let track_id = fbb.create_string("track-length");
        let target = fbb.create_string("b0");
        let track = KFTrack::create(
            &mut fbb,
            &KFTrackArgs {
                id: Some(track_id),
                target_node_id: Some(target),
                property: AnimatedPropertyKind::BoneLength,
                keyframes: Some(keyframes),
                ..Default::default()
            },
        );
        let tracks = fbb.create_vector(&[track]);
        let id = fbb.create_string("clip");
        let clip_name = fbb.create_string("Clip");
        let animation = Animation::create(
            &mut fbb,
            &AnimationArgs {
                id: Some(id),
                name: Some(clip_name),
                duration: 1.0,
                tracks: Some(tracks),
                ..Default::default()
            },
        );
        fbb.create_vector(&[animation])
    });

    let name = fbb.create_string("Bones IK Fixture");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            bones: Some(bones),
            ik_chains: Some(chains),
            ik_targets: Some(targets),
            animations,
            ..Default::default()
        },
    );

    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

fn build_weighted_path_node() -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(512);

    let b0 = fbb.create_string("b0");
    let weight0 = BoneWeight::create(
        &mut fbb,
        &BoneWeightArgs {
            bone_id: Some(b0),
            weight: 0.25,
        },
    );

    let b1 = fbb.create_string("b1");
    let weight1 = BoneWeight::create(
        &mut fbb,
        &BoneWeightArgs {
            bone_id: Some(b1),
            weight: 0.75,
        },
    );
    let weights = fbb.create_vector(&[weight0, weight1]);

    let legacy_bone = fbb.create_string("legacy-bone");
    let node = PathNodeFBV2::create(
        &mut fbb,
        &PathNodeFBV2Args {
            anchor_x: 0.0,
            anchor_y: 50.0,
            in_handle_x: 0.0,
            in_handle_y: 10.0,
            out_handle_x: 0.0,
            out_handle_y: -10.0,
            bone_id: Some(legacy_bone),
            bone_influence: 0.75,
            weights: Some(weights),
            ..Default::default()
        },
    );

    fbb.finish(node, None);
    fbb.finished_data().to_vec()
}

/// The bones live in world space, so a vertex has to travel out of node space
/// and back. Applying the deform straight to node-local points agrees with the
/// editor (`lbs_deformer.dart:139-165`) only when the node's world matrix is
/// identity — so rigged artwork inside a moved group deformed differently on
/// device (issue #111 parity audit §5.6).
#[test]
fn skinning_happens_in_world_space_not_node_space() {
    let mut rest_fk = HashMap::new();
    rest_fk.insert("bone".to_string(), IDENTITY_MATRIX4);
    let mut current_fk = HashMap::new();
    // A pure world-space translation of the bone.
    current_fk.insert(
        "bone".to_string(),
        [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            10.0, 0.0, 0.0, 1.0,
        ],
    );

    // Off the origin on purpose: a vertex at (0,0) survives the trip out to
    // world space unchanged under a pure scale, so it cannot tell the two
    // implementations apart.
    let point = RuntimePathPoint {
        anchor: Point2 { x: 4.0, y: 0.0 },
        in_handle: Point2 { x: 0.0, y: 0.0 },
        out_handle: Point2 { x: 0.0, y: 0.0 },
        bone_id: Some("bone".to_string()),
        bone_influence: 1.0,
        weights: Vec::new(),
    };

    // The node sits inside a group scaled by 2. The vertex is at node x=4,
    // so world x=8; the bone moves it to 18; back in node space that is 9.
    // Skinning in node space would answer 4 + 10 = 14, halved to 7.
    let scale_2x = [
        2.0, 0.0, 0.0, 0.0, //
        0.0, 2.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ];
    let half = [
        0.5, 0.0, 0.0, 0.0, //
        0.0, 0.5, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ];

    let scaled = apply_skeletal_transforms(
        &[point.clone()],
        &current_fk,
        &rest_fk,
        scale_2x,
        half,
    );
    assert!(
        (scaled[0].anchor.x - 9.0).abs() < 1e-9,
        "got {}",
        scaled[0].anchor.x
    );

    // Identity world matrix keeps the old behaviour, which is the right answer
    // for a top-level node.
    let plain = apply_skeletal_transforms(
        &[point],
        &current_fk,
        &rest_fk,
        IDENTITY_MATRIX4,
        IDENTITY_MATRIX4,
    );
    assert!(
        (plain[0].anchor.x - 14.0).abs() < 1e-9,
        "got {}",
        plain[0].anchor.x
    );
}

/// The runtime hardcoded one side of the elbow, so a chain authored to bend
/// the other way flipped on device (issue #111 parity audit §4).
#[test]
fn a_two_bone_chain_folds_to_the_authored_side() {
    let bones = two_bone_chain();
    let target = Point2 { x: 60.0, y: 60.0 };

    let mut positive = two_bone_chain_descriptor(RuntimeIkSolverKind::TwoBone);
    positive.bend_direction = 1;
    let mut negative = two_bone_chain_descriptor(RuntimeIkSolverKind::TwoBone);
    negative.bend_direction = -1;

    let one = solve_chain(&bones, &positive, target);
    let other = solve_chain(&bones, &negative, target);

    assert!(
        (one - other).abs() > 1e-6,
        "the two bend directions should not produce the same elbow: {one} vs {other}"
    );
}

fn solve_chain(bones: &[RuntimeBone], chain: &RuntimeIkChain, target: Point2) -> f64 {
    let fk = solve_fk(bones);
    let targets = vec![RuntimeIkTarget {
        id: "t0".to_owned(),
        position: target,
        extent: (0.0, 0.0),
        parent_id: None,
        interactive: false,
    }];
    let result = evaluate_ik(bones, &[chain.clone()], &targets, &fk);
    *result.get("b1").expect("the mid bone should be solved")
}

/// `boneLength` used to export as `BoneRotation`, so a stretch track not only
/// failed to stretch the bone but *overwrote* any real rotation on it —
/// runtime overrides are keyed by property name (issue #111).
///
/// FK places a child at its own authored position, so length is not what puts
/// bones on screen; it is what the IK solver measures the chain with. That is
/// the consumer this checks, and it is the one a stretch track is for.
#[test]
fn a_bone_length_track_reaches_the_solver_through_the_player() {
    use kineto_engine::player::KinetoPlayer;

    let plain = KinetoPlayer::load(build_bones_ik_document_with(None))
        .expect("load")
        .play()
        .expect("frame");
    let stretched = KinetoPlayer::load(build_bones_ik_document_with(Some(240.0)))
        .expect("load")
        .play()
        .expect("frame");

    let plain_tip = plain.fk.get("b1").expect("b1 solved")[12];
    let stretched_tip = stretched.fk.get("b1").expect("b1 solved")[12];
    assert!(
        (plain_tip - stretched_tip).abs() > 1e-6,
        "the boneLength track should change the IK pose: {plain_tip} vs {stretched_tip}"
    );
}

#[test]
fn a_bone_length_override_changes_the_ik_solve() {
    let bones = two_bone_chain();
    let chain = two_bone_chain_descriptor(RuntimeIkSolverKind::TwoBone);
    let target = Point2 { x: 60.0, y: 60.0 };

    let rest = solve_chain(&bones, &chain, target);

    let mut stretched_bones = bones.clone();
    stretched_bones[0].length = 200.0;
    let stretched = solve_chain(&stretched_bones, &chain, target);

    assert!(
        (rest - stretched).abs() > 1e-6,
        "a longer root bone should reach the target differently: {rest} vs {stretched}"
    );
}
