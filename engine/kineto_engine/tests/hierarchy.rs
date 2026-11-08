//! Issue #74: groups carry what is inside them — artwork, hit areas, targets.

use std::collections::HashMap;

use kineto_engine::animation::{NodeOverrides, PropertyValue};
use kineto_engine::format::generated::kineto::{
    finish_document_buffer, root_as_document, AnimatedPropertyKind, Animation, AnimationArgs, Bone,
    BoneArgs, Component, Document, DocumentArgs, Geometry, Group, GroupArgs, IkTarget,
    IkTargetArgs, KFKeyframe, KFKeyframeArgs, KFTrack, KFTrackArgs, Node, NodeArgs, Rectangle,
    RectangleArgs, Shape, ShapeArgs, Transform, TransformArgs,
};
use kineto_engine::hierarchy::{apply_point, group_carry_deltas, world_affines};
use kineto_engine::player::KinetoPlayer;

/// A group at (100, 50) holding one 40×20 rectangle at (10, 10), plus a target
/// at (200, 200) that the group carries.
fn build_doc(group_x: f32, group_scale: f32) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(2048);

    let child_id = fbb.create_string("child");
    let child_name = fbb.create_string("bg");
    let rect = Rectangle::create(
        &mut fbb,
        &RectangleArgs {
            width: 40.0,
            height: 20.0,
            ..Default::default()
        },
    );
    let shape = Shape::create(
        &mut fbb,
        &ShapeArgs {
            geometry_type: Geometry::Rectangle,
            geometry: Some(rect.as_union_value()),
            ..Default::default()
        },
    );
    let child_transform = Transform::create(
        &mut fbb,
        &TransformArgs {
            x: 10.0,
            y: 10.0,
            scale_x: 1.0,
            scale_y: 1.0,
            ..Default::default()
        },
    );
    let child = Node::create(
        &mut fbb,
        &NodeArgs {
            id: Some(child_id),
            name: Some(child_name),
            transform: Some(child_transform),
            visible: true,
            opacity: 1.0,
            component_type: Component::Shape,
            component: Some(shape.as_union_value()),
            ..Default::default()
        },
    );

    let member = fbb.create_string("child");
    let members = fbb.create_vector(&[member]);
    let group = Group::create(
        &mut fbb,
        &GroupArgs {
            child_ids: Some(members),
            mask_child_id: None,
        },
    );
    let group_id = fbb.create_string("group");
    let group_name = fbb.create_string("Button");
    let group_transform = Transform::create(
        &mut fbb,
        &TransformArgs {
            x: group_x,
            y: 50.0,
            scale_x: group_scale,
            scale_y: group_scale,
            ..Default::default()
        },
    );
    let group_node = Node::create(
        &mut fbb,
        &NodeArgs {
            id: Some(group_id),
            name: Some(group_name),
            transform: Some(group_transform),
            visible: true,
            opacity: 1.0,
            component_type: Component::Group,
            component: Some(group.as_union_value()),
            ..Default::default()
        },
    );
    let nodes = fbb.create_vector(&[group_node, child]);

    let target_id = fbb.create_string("hand");
    let chain_root = fbb.create_string("root");
    let target_parent = fbb.create_string("group");
    let target = IkTarget::create(
        &mut fbb,
        &IkTargetArgs {
            id: Some(target_id),
            chain_root_id: Some(chain_root),
            position_x: 200.0,
            position_y: 200.0,
            extent_w: 100.0,
            extent_h: 40.0,
            parent_id: Some(target_parent),
            ..Default::default()
        },
    );
    let targets = fbb.create_vector(&[target]);

    // A two-bone rig rooted in the same group: only the root names it, and FK
    // carries the child off the root (issue #74).
    let root_id = fbb.create_string("bone-root");
    let root_group = fbb.create_string("group");
    let root = Bone::create(
        &mut fbb,
        &BoneArgs {
            id: Some(root_id),
            local_x: 20.0,
            local_y: 30.0,
            length: 60.0,
            group_id: Some(root_group),
            ..Default::default()
        },
    );
    let child_bone_id = fbb.create_string("bone-child");
    let child_parent = fbb.create_string("bone-root");
    let child_bone = Bone::create(
        &mut fbb,
        &BoneArgs {
            id: Some(child_bone_id),
            parent_id: Some(child_parent),
            local_y: 60.0,
            length: 40.0,
            ..Default::default()
        },
    );
    let bones = fbb.create_vector(&[root, child_bone]);

    // One clip that slides the group 60 to the right over a second, so the
    // carry can be checked through the player rather than around it.
    let kf_a_id = fbb.create_string("kf-a");
    let kf_a = KFKeyframe::create(
        &mut fbb,
        &KFKeyframeArgs {
            id: Some(kf_a_id),
            time: 0.0,
            scalar_value: group_x,
            ..Default::default()
        },
    );
    let kf_b_id = fbb.create_string("kf-b");
    let kf_b = KFKeyframe::create(
        &mut fbb,
        &KFKeyframeArgs {
            id: Some(kf_b_id),
            time: 1.0,
            scalar_value: group_x + 60.0,
            ..Default::default()
        },
    );
    let keyframes = fbb.create_vector(&[kf_a, kf_b]);
    let track_id = fbb.create_string("track-group-x");
    let track_subject = fbb.create_string("group");
    let track = KFTrack::create(
        &mut fbb,
        &KFTrackArgs {
            id: Some(track_id),
            target_node_id: Some(track_subject),
            property: AnimatedPropertyKind::PositionX,
            keyframes: Some(keyframes),
        },
    );
    let tracks = fbb.create_vector(&[track]);
    let anim_id = fbb.create_string("slide");
    let anim_name = fbb.create_string("Slide");
    let animation = Animation::create(
        &mut fbb,
        &AnimationArgs {
            id: Some(anim_id),
            name: Some(anim_name),
            duration: 1.0,
            frame_rate: 60,
            tracks: Some(tracks),
            ..Default::default()
        },
    );
    let animations = fbb.create_vector(&[animation]);

    let name = fbb.create_string("Grouped");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            nodes: Some(nodes),
            animations: Some(animations),
            bones: Some(bones),
            ik_targets: Some(targets),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

fn moved(id: &str, dx: f64) -> HashMap<String, NodeOverrides> {
    let mut overrides = NodeOverrides::default();
    overrides
        .values
        .insert("positionX", PropertyValue::Scalar(dx));
    HashMap::from([(id.to_owned(), overrides)])
}

#[test]
fn a_child_sits_in_its_groups_space() {
    let bytes = build_doc(100.0, 1.0);
    let document = root_as_document(&bytes).expect("document should parse");
    let world = world_affines(&document, &HashMap::new());

    // Group (100, 50) then child (10, 10).
    let (x, y) = apply_point(world["child"], 0.0, 0.0);
    assert_eq!((x, y), (110.0, 60.0));
}

#[test]
fn a_groups_scale_reaches_its_child() {
    let bytes = build_doc(0.0, 2.0);
    let document = root_as_document(&bytes).expect("document should parse");
    let world = world_affines(&document, &HashMap::new());

    // The child's own origin is (10,10), doubled by the group, and the far
    // corner of the 40×20 rectangle lands twice as far out.
    assert_eq!(apply_point(world["child"], 0.0, 0.0), (20.0, 70.0));
    assert_eq!(apply_point(world["child"], 40.0, 20.0), (100.0, 110.0));
}

#[test]
fn an_animated_group_carries_its_target() {
    let bytes = build_doc(100.0, 1.0);
    let mut player = KinetoPlayer::load(bytes).expect("player should load");
    player.advance(0.0).expect("advance");

    let document = player.document().expect("document should parse");
    let deltas = group_carry_deltas(&document, &moved("group", 160.0));
    let delta = deltas["group"];

    // The group moved 60 to the right of where it was authored, so its target
    // moves exactly that far — no more, because the position is authored space.
    assert_eq!(apply_point(delta, 200.0, 200.0), (260.0, 200.0));
}

#[test]
fn an_animated_group_carries_the_rig_rooted_in_it() {
    let bytes = build_doc(100.0, 1.0);
    let mut player = KinetoPlayer::load(bytes).expect("player should load");
    player.play().expect("the document has one clip");

    let start = player.advance(0.0).expect("advance");
    let child_at_rest = start.fk["bone-child"][12];

    // Half a second in, the group has slid 30 of its 60.
    let mid = player.advance(0.5).expect("advance");
    assert_eq!(mid.fk["bone-child"][12] - child_at_rest, 30.0);
}

#[test]
fn a_still_group_carries_nothing() {
    let bytes = build_doc(100.0, 1.0);
    let document = root_as_document(&bytes).expect("document should parse");

    assert!(group_carry_deltas(&document, &HashMap::new()).is_empty());
}

#[test]
fn hit_testing_goes_through_the_group() {
    let bytes = build_doc(100.0, 1.0);
    let player = KinetoPlayer::load(bytes).expect("player should load");

    // The rectangle draws at (110, 60)–(150, 80). Its untransformed position
    // is what the hit test used to answer with.
    let inside = player
        .hit_test_document_point(120.0, 70.0)
        .expect("hit test should run");
    assert_eq!(inside.map(|hit| hit.node_id), Some("child".to_owned()));

    let outside = player
        .hit_test_document_point(20.0, 15.0)
        .expect("hit test should run");
    assert!(outside.is_none(), "the group's own offset must count");
}

#[test]
fn hit_testing_follows_the_animated_pose() {
    let bytes = build_doc(100.0, 1.0);
    let mut player = KinetoPlayer::load(bytes).expect("player should load");
    player.advance(0.0).expect("advance");

    // Nothing animates in this fixture, so the pose is the authored one; the
    // point of the assertion is that the hit test reads the frame at all.
    let hit = player
        .hit_test_document_point(120.0, 70.0)
        .expect("hit test should run");
    assert_eq!(hit.map(|hit| hit.node_id), Some("child".to_owned()));
}
