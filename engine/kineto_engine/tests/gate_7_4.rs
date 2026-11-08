/// Step 7.4 de-risking gate: build an animated .kvk Document in Rust,
/// serialize it (with KRTM file identifier), reload from bytes,
/// and print evaluated state at several time points.
///
/// Run with: cargo test gate_ -- --nocapture
use std::io::Write;

use kineto_engine::animation::{evaluate_animation, PropertyValue};
use kineto_engine::format::generated::kineto::{
    finish_document_buffer, root_as_document, Animation, AnimationArgs, AnimatedPropertyKind,
    Component, Document, DocumentArgs, KFKeyframe, KFKeyframeArgs, KFTrack, KFTrackArgs,
    KeyframeInterpolationKind, Node, NodeArgs, Shape, ShapeArgs, DOCUMENT_IDENTIFIER,
};

// All string offsets must be created BEFORE the enclosing `create` call —
// FlatBuffers Rust does not allow borrowing fbb twice in the same expression.

fn build_animated_document() -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(2048);

    // ── Shape node ────────────────────────────────────────────────────────────
    let shape = Shape::create(&mut fbb, &ShapeArgs::default());

    let node_id   = fbb.create_string("node-box");
    let node_name = fbb.create_string("Box");
    let node = Node::create(
        &mut fbb,
        &NodeArgs {
            id: Some(node_id),
            name: Some(node_name),
            component_type: Component::Shape,
            component: Some(shape.as_union_value()),
            visible: true,
            opacity: 1.0,
            ..Default::default()
        },
    );
    let nodes_vec = fbb.create_vector(&[node]);

    // ── Opacity track: 0.0 at t=0 → 1.0 at t=1.0 (linear) ───────────────────
    let id0 = fbb.create_string("kf-op-0");
    let kf_op0 = KFKeyframe::create(&mut fbb, &KFKeyframeArgs {
        id: Some(id0),
        time: 0.0,
        scalar_value: 0.0,
        interpolation: KeyframeInterpolationKind::Linear,
        ..Default::default()
    });
    let id1 = fbb.create_string("kf-op-1");
    let kf_op1 = KFKeyframe::create(&mut fbb, &KFKeyframeArgs {
        id: Some(id1),
        time: 1.0,
        scalar_value: 1.0,
        interpolation: KeyframeInterpolationKind::Linear,
        ..Default::default()
    });
    let kfs_op = fbb.create_vector(&[kf_op0, kf_op1]);
    let tid_op  = fbb.create_string("track-opacity");
    let nid_op  = fbb.create_string("node-box");
    let track_op = KFTrack::create(&mut fbb, &KFTrackArgs {
        id: Some(tid_op),
        target_node_id: Some(nid_op),
        property: AnimatedPropertyKind::Opacity,
        keyframes: Some(kfs_op),
    });

    // ── PositionX track: 0.0 at t=0 → 400.0 at t=2.0 (auto-bezier) ──────────
    let id2 = fbb.create_string("kf-px-0");
    let kf_px0 = KFKeyframe::create(&mut fbb, &KFKeyframeArgs {
        id: Some(id2),
        time: 0.0,
        scalar_value: 0.0,
        interpolation: KeyframeInterpolationKind::AutoBezier,
        ..Default::default()
    });
    let id3 = fbb.create_string("kf-px-1");
    let kf_px1 = KFKeyframe::create(&mut fbb, &KFKeyframeArgs {
        id: Some(id3),
        time: 2.0,
        scalar_value: 400.0,
        interpolation: KeyframeInterpolationKind::Linear,
        ..Default::default()
    });
    let kfs_px  = fbb.create_vector(&[kf_px0, kf_px1]);
    let tid_px  = fbb.create_string("track-positionX");
    let nid_px  = fbb.create_string("node-box");
    let track_px = KFTrack::create(&mut fbb, &KFTrackArgs {
        id: Some(tid_px),
        target_node_id: Some(nid_px),
        property: AnimatedPropertyKind::PositionX,
        keyframes: Some(kfs_px),
    });

    let tracks_vec = fbb.create_vector(&[track_op, track_px]);

    // ── Animation clip ────────────────────────────────────────────────────────
    let anim_id   = fbb.create_string("anim-logo-loop");
    let anim_name = fbb.create_string("Logo Loop");
    let anim = Animation::create(&mut fbb, &AnimationArgs {
        id: Some(anim_id),
        name: Some(anim_name),
        duration: 2.0,
        frame_rate: 60,
        tracks: Some(tracks_vec),
        loop_: true,
        ..Default::default()
    });
    let anims_vec = fbb.create_vector(&[anim]);

    // ── Document root ─────────────────────────────────────────────────────────
    let doc_name = fbb.create_string("Gate 7.4 Fixture");
    let doc = Document::create(&mut fbb, &DocumentArgs {
        name: Some(doc_name),
        canvas_width: 800.0,
        canvas_height: 600.0,
        frame_rate: 60,
        nodes: Some(nodes_vec),
        animations: Some(anims_vec),
        ..Default::default()
    });

    finish_document_buffer(&mut fbb, doc);
    fbb.finished_data().to_vec()
}

#[test]
fn gate_7_4_build_serialize_reload_evaluate() {
    let buf = build_animated_document();

    // Verify KRTM file identifier
    assert!(
        flatbuffers::buffer_has_identifier(&buf, DOCUMENT_IDENTIFIER, false),
        "KRTM identifier missing"
    );

    // Persist for manual inspection
    let fixture_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/animated_sample.kvk");
    if let Ok(mut f) = std::fs::File::create(fixture_path) {
        let _ = f.write_all(&buf);
        println!("Wrote {fixture_path} ({} bytes)", buf.len());
    }

    // Reload from bytes — same as runtime path
    let doc = root_as_document(&buf).expect("reload failed");

    println!("\n=== Gate 7.4: .kvk evaluated state ===");
    println!(
        "Document: \"{}\" {}x{} @ {}fps",
        doc.name(), doc.canvas_width(), doc.canvas_height(), doc.frame_rate()
    );
    println!("Nodes:      {}", doc.nodes().map(|n| n.len()).unwrap_or(0));

    let animations = doc.animations().unwrap_or_default();
    println!("Animations: {}", animations.len());

    for anim in &animations {
        println!("\nClip \"{}\" duration={}s", anim.name(), anim.duration());
        println!("  {:<8}  {:>8}  {:>12}", "t (s)", "opacity", "positionX");
        println!("  {}", "-".repeat(32));
        for t in [0.0f64, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0] {
            let ov = evaluate_animation(&anim, t);
            let node = ov.get("node-box");
            let opacity  = node.and_then(|o| o.scalar("opacity")).unwrap_or(f64::NAN);
            let pos_x    = node.and_then(|o| o.scalar("positionX")).unwrap_or(f64::NAN);
            println!("  {t:<8.2}  {opacity:>8.4}  {pos_x:>12.3}");
        }
    }
    println!("\n=== Gate PASSED ===\n");

    // ── Assertions ─────────────────────────────────────────────────────────────
    let anim = animations.get(0);

    let scalar_at = |t: f64, key: &'static str| -> f64 {
        evaluate_animation(&anim, t)
            .remove("node-box")
            .and_then(|mut o| o.values.remove(key))
            .and_then(|v| if let PropertyValue::Scalar(s) = v { Some(s) } else { None })
            .unwrap_or(f64::NAN)
    };

    // opacity: linear 0→1 over [0,1]
    assert!((scalar_at(0.0, "opacity") - 0.0).abs() < 1e-6, "opacity at t=0");
    assert!((scalar_at(0.5, "opacity") - 0.5).abs() < 0.001, "opacity at t=0.5");
    assert!((scalar_at(1.0, "opacity") - 1.0).abs() < 1e-6, "opacity at t=1");
    assert!((scalar_at(2.0, "opacity") - 1.0).abs() < 1e-6, "opacity clamps past last kf");

    // positionX: auto-bezier 0→400 over [0,2]
    assert!((scalar_at(0.0, "positionX") -   0.0).abs() < 1e-4, "positionX at t=0");
    assert!((scalar_at(2.0, "positionX") - 400.0).abs() < 1e-4, "positionX at t=2");
    let px_mid = scalar_at(1.0, "positionX");
    assert!(px_mid > 0.0 && px_mid < 400.0, "positionX at t=1 in (0,400): {px_mid}");
}
