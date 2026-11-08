//! Cross-runtime rig parity: bone constraints and Attach to Bone.
//!
//! The editor poses a rig with constraints on its bones (FK, IK, then
//! `applyConstraints`) and draws an attached object at `boneWorld · offset`.
//! The runtime did neither: a bone constrained to follow a target sat still,
//! and an attached object never left its authored place. This holds the
//! runtime to the table `test/tool/rig_parity_fixture_test.dart` writes with
//! the editor's own solvers.

use kineto_engine::format;
use kineto_engine::hierarchy::{multiply, Affine};
use kineto_engine::player::KinetoPlayer;
use kineto_engine::render::{build_render_scene, RenderTransform};

const EXPECTED: &str = include_str!("../../../test/fixtures/constraints/rig_parity.txt");

fn expected(kind: &str) -> Vec<(String, Affine)> {
    EXPECTED
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with(kind))
        .map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let v: Vec<f64> = parts[2..8].iter().map(|p| p.parse().unwrap()).collect();
            (parts[1].to_owned(), [v[0], v[1], v[2], v[3], v[4], v[5]])
        })
        .collect()
}

fn assert_affine(label: &str, got: Affine, want: Affine) {
    for i in 0..6 {
        assert!(
            (got[i] - want[i]).abs() < 1e-3,
            "{label}: got {got:?}, the editor has {want:?}"
        );
    }
}

fn transform_affine(t: RenderTransform) -> Affine {
    let (sin, cos) = t.rotation.sin_cos();
    let to_anchor = [1.0, 0.0, 0.0, 1.0, t.x + t.anchor_x, t.y + t.anchor_y];
    let rotate_scale = [cos * t.scale_x, sin * t.scale_x, -sin * t.scale_y, cos * t.scale_y, 0.0, 0.0];
    let from_anchor = [1.0, 0.0, 0.0, 1.0, -t.anchor_x, -t.anchor_y];
    multiply(multiply(to_anchor, rotate_scale), from_anchor)
}

#[test]
fn the_runtime_poses_constrained_bones_and_attached_objects_where_the_editor_does() {
    let bytes = include_bytes!("fixtures/rig_parity.kvk");
    let document = format::load(bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.to_vec()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let bones = expected("bone");
    assert!(!bones.is_empty());
    for (id, want) in bones {
        let m = frame.fk.get(&id).unwrap_or_else(|| panic!("bone {id} did not pose"));
        assert_affine(&id, [m[0], m[1], m[4], m[5], m[12], m[13]], want);
    }

    let scene = build_render_scene(document, &frame);
    let nodes = expected("node");
    assert!(!nodes.is_empty());
    for (id, want) in nodes {
        let command = scene
            .commands
            .iter()
            .find(|c| c.node_id == id)
            .unwrap_or_else(|| panic!("node {id} did not draw"));
        let world = multiply(command.parent, transform_affine(command.transform));
        assert_affine(&id, world, want);
    }
}
