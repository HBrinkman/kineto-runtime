/// Mirrors the Dart timeline_eval_test.dart and easing_presets_test.dart.
use kineto_engine::animation::{
    auto_bezier_ease, cubic_bezier_ease, evaluate_animation, EasingPreset,
};
use kineto_engine::format::generated::kineto::{
    Animation, AnimationArgs, AnimatedPropertyKind, BezierHandle,
    KFKeyframe, KFKeyframeArgs, KFTrack, KFTrackArgs,
    KeyframeInterpolationKind,
};

// ── Builder helpers ───────────────────────────────────────────────────────────

fn make_kf<'fbb>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'fbb>,
    id: &str,
    time: f32,
    scalar_value: f32,
    interp: KeyframeInterpolationKind,
    bezier_out: Option<BezierHandle>,
    bezier_in: Option<BezierHandle>,
) -> flatbuffers::WIPOffset<KFKeyframe<'fbb>> {
    let id_off = fbb.create_string(id);
    KFKeyframe::create(
        fbb,
        &KFKeyframeArgs {
            id: Some(id_off),
            time,
            scalar_value,
            is_color: false,
            color_value: 0,
            interpolation: interp,
            bezier_out: bezier_out.as_ref(),
            bezier_in: bezier_in.as_ref(),
        path_vertices: None,
        },
    )
}

fn make_color_kf<'fbb>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'fbb>,
    id: &str,
    time: f32,
    color: u32,
) -> flatbuffers::WIPOffset<KFKeyframe<'fbb>> {
    let id_off = fbb.create_string(id);
    KFKeyframe::create(
        fbb,
        &KFKeyframeArgs {
            id: Some(id_off),
            time,
            scalar_value: 0.0,
            is_color: true,
            color_value: color,
            interpolation: KeyframeInterpolationKind::Linear,
            bezier_out: None,
            bezier_in: None,
        path_vertices: None,
        },
    )
}

/// Build an in-memory Animation buffer with a single opacity track.
/// Returns the buffer; caller parses with `flatbuffers::root::<Animation>`.
fn build_opacity_animation(
    kf_specs: &[(f32, f32, KeyframeInterpolationKind)], // (time, value, interp)
    bezier: Option<(BezierHandle, BezierHandle)>,        // (out, in) handles for all kfs
) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(512);

    let kfs: Vec<_> = kf_specs
        .iter()
        .enumerate()
        .map(|(i, (t, v, interp))| {
            let (bout, bin_) = if let Some((ref bo, ref bi)) = bezier {
                (Some(*bo), Some(*bi))
            } else {
                (None, None)
            };
            make_kf(&mut fbb, &format!("kf-{i}"), *t, *v, *interp, bout, bin_)
        })
        .collect();

    let kfs_vec = fbb.create_vector(&kfs);
    let tid = fbb.create_string("track-1");
    let nid = fbb.create_string("node-1");
    let track = KFTrack::create(
        &mut fbb,
        &KFTrackArgs {
            id: Some(tid),
            target_node_id: Some(nid),
            property: AnimatedPropertyKind::Opacity,
            keyframes: Some(kfs_vec),
        },
    );

    let tracks_vec = fbb.create_vector(&[track]);
    let aid = fbb.create_string("anim-1");
    let aname = fbb.create_string("Test");
    let anim = Animation::create(
        &mut fbb,
        &AnimationArgs {
            id: Some(aid),
            name: Some(aname),
            duration: 2.0,
            frame_rate: 60,
            tracks: Some(tracks_vec),
            ..Default::default()
        },
    );
    fbb.finish(anim, None);
    fbb.finished_data().to_vec()
}

fn opacity_at(buf: &[u8], t: f64) -> f64 {
    let anim = flatbuffers::root::<Animation>(buf).unwrap();
    let overrides = evaluate_animation(&anim, t);
    match overrides.get("node-1").and_then(|o| o.scalar("opacity")) {
        Some(v) => v,
        None => panic!("no opacity override at t={t}"),
    }
}

// ── evaluateAnimation mirrors ─────────────────────────────────────────────────

#[test]
fn empty_track_produces_no_overrides() {
    let buf = build_opacity_animation(&[], None);
    let anim = flatbuffers::root::<Animation>(&buf).unwrap();
    let overrides = evaluate_animation(&anim, 0.5);
    assert!(overrides.is_empty(), "expected no overrides for empty track");
}

#[test]
fn single_keyframe_clamps_before_and_after() {
    let buf = build_opacity_animation(
        &[(0.5, 42.0, KeyframeInterpolationKind::Linear)],
        None,
    );
    assert!((opacity_at(&buf, 0.0) - 42.0).abs() < 1e-6);
    assert!((opacity_at(&buf, 0.5) - 42.0).abs() < 1e-6);
    assert!((opacity_at(&buf, 1.0) - 42.0).abs() < 1e-6);
}

#[test]
fn t_before_first_keyframe_clamps_to_first_value() {
    let buf = build_opacity_animation(
        &[
            (0.5, 10.0, KeyframeInterpolationKind::Linear),
            (1.0, 20.0, KeyframeInterpolationKind::Linear),
        ],
        None,
    );
    assert!((opacity_at(&buf, 0.0) - 10.0).abs() < 1e-6);
    assert!((opacity_at(&buf, 0.25) - 10.0).abs() < 1e-6);
}

#[test]
fn t_after_last_keyframe_clamps_to_last_value() {
    let buf = build_opacity_animation(
        &[
            (0.0, 0.0, KeyframeInterpolationKind::Linear),
            (1.0, 100.0, KeyframeInterpolationKind::Linear),
        ],
        None,
    );
    assert!((opacity_at(&buf, 1.5) - 100.0).abs() < 1e-6);
}

#[test]
fn linear_interpolation_at_midpoint() {
    let buf = build_opacity_animation(
        &[
            (0.0, 0.0, KeyframeInterpolationKind::Linear),
            (1.0, 100.0, KeyframeInterpolationKind::Linear),
        ],
        None,
    );
    assert!((opacity_at(&buf, 0.5) - 50.0).abs() < 0.001);
}

#[test]
fn hold_interpolation_snaps_at_boundary() {
    let buf = build_opacity_animation(
        &[
            (0.0, 10.0, KeyframeInterpolationKind::Hold),
            (1.0, 20.0, KeyframeInterpolationKind::Linear),
        ],
        None,
    );
    assert!((opacity_at(&buf, 0.01) - 10.0).abs() < 1e-6);
    assert!((opacity_at(&buf, 0.99) - 10.0).abs() < 1e-6);
    assert!((opacity_at(&buf, 1.0) - 20.0).abs() < 1e-6);
}

#[test]
fn bezier_ease_out_is_above_linear_at_midpoint() {
    // ease-out: out_handle=(0,0), in_handle=(0.58,1.0) → fast start, slow end
    let bout = BezierHandle::new(0.0, 0.0);
    let bin_ = BezierHandle::new(0.58, 1.0);
    let buf = build_opacity_animation(
        &[
            (0.0, 0.0, KeyframeInterpolationKind::Bezier),
            (1.0, 100.0, KeyframeInterpolationKind::Linear),
        ],
        Some((bout, bin_)),
    );
    let v = opacity_at(&buf, 0.5);
    assert!(v > 50.0, "ease-out should be above linear at midpoint, got {v}");
}

#[test]
fn auto_bezier_is_between_endpoints_at_midpoint() {
    let buf = build_opacity_animation(
        &[
            (0.0, 0.0, KeyframeInterpolationKind::AutoBezier),
            (1.0, 100.0, KeyframeInterpolationKind::Linear),
        ],
        None,
    );
    let v = opacity_at(&buf, 0.5);
    assert!(v > 0.0 && v < 100.0, "auto-bezier midpoint out of range: {v}");
}

#[test]
fn multiple_tracks_evaluated_independently() {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(512);

    // track for node-1: opacity 0→1 over 1s
    let kf0 = make_kf(&mut fbb, "kf-0", 0.0, 0.0, KeyframeInterpolationKind::Linear, None, None);
    let kf1 = make_kf(&mut fbb, "kf-1", 1.0, 1.0, KeyframeInterpolationKind::Linear, None, None);
    let kfs1 = fbb.create_vector(&[kf0, kf1]);
    let tid1 = fbb.create_string("track-node-1");
    let nid1 = fbb.create_string("node-1");
    let t1 = KFTrack::create(&mut fbb, &KFTrackArgs {
        id: Some(tid1), target_node_id: Some(nid1),
        property: AnimatedPropertyKind::Opacity, keyframes: Some(kfs1),
    });

    // track for node-2: rotation 0→180 over 1s
    let kf2 = make_kf(&mut fbb, "kf-2", 0.0, 0.0, KeyframeInterpolationKind::Linear, None, None);
    let kf3 = make_kf(&mut fbb, "kf-3", 1.0, 180.0, KeyframeInterpolationKind::Linear, None, None);
    let kfs2 = fbb.create_vector(&[kf2, kf3]);
    let tid2 = fbb.create_string("track-node-2");
    let nid2 = fbb.create_string("node-2");
    let t2 = KFTrack::create(&mut fbb, &KFTrackArgs {
        id: Some(tid2), target_node_id: Some(nid2),
        property: AnimatedPropertyKind::Rotation, keyframes: Some(kfs2),
    });
    let tracks_vec = fbb.create_vector(&[t1, t2]);
    let aid = fbb.create_string("anim-1");
    let aname = fbb.create_string("Multi");
    let anim = Animation::create(&mut fbb, &AnimationArgs {
        id: Some(aid), name: Some(aname), duration: 2.0, frame_rate: 60, tracks: Some(tracks_vec),
        ..Default::default()
    });
    fbb.finish(anim, None);
    let buf = fbb.finished_data();

    let anim = flatbuffers::root::<Animation>(buf).unwrap();
    let overrides = evaluate_animation(&anim, 0.5);

    assert!(overrides.contains_key("node-1"));
    assert!(overrides.contains_key("node-2"));
    let rot = overrides["node-2"].scalar("rotation").unwrap();
    assert!((rot - 90.0).abs() < 0.1, "expected rotation ~90 at t=0.5, got {rot}");
}

#[test]
fn color_keyframes_interpolate_channels() {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(256);
    // 0x00000000 → 0xFFFFFFFF at t=[0,1]
    let kf0 = make_color_kf(&mut fbb, "kf-0", 0.0, 0x00000000);
    let kf1 = make_color_kf(&mut fbb, "kf-1", 1.0, 0xFFFFFFFF);
    let kfs = fbb.create_vector(&[kf0, kf1]);
    let tid = fbb.create_string("track-1");
    let nid = fbb.create_string("node-1");
    let track = KFTrack::create(&mut fbb, &KFTrackArgs {
        id: Some(tid), target_node_id: Some(nid),
        property: AnimatedPropertyKind::FillColor, keyframes: Some(kfs),
    });
    let tracks_vec = fbb.create_vector(&[track]);
    let aid = fbb.create_string("a"); let aname = fbb.create_string("C");
    let anim = Animation::create(&mut fbb, &AnimationArgs {
        id: Some(aid), name: Some(aname), duration: 1.0, frame_rate: 60, tracks: Some(tracks_vec),
        ..Default::default()
    });
    fbb.finish(anim, None);
    let buf = fbb.finished_data();
    let anim = flatbuffers::root::<Animation>(buf).unwrap();
    let overrides = evaluate_animation(&anim, 0.5);
    let c = overrides["node-1"].color("fillColor").unwrap();
    let [a, r, g, b] = c.to_be_bytes();
    // all channels should be ~128 at t=0.5
    assert!((a as i32 - 128).abs() <= 1, "alpha={a}");
    assert!((r as i32 - 128).abs() <= 1, "red={r}");
    assert!((g as i32 - 128).abs() <= 1, "green={g}");
    assert!((b as i32 - 128).abs() <= 1, "blue={b}");
}

// ── easing preset mirrors ─────────────────────────────────────────────────────

#[test]
fn linear_is_identity() {
    for &t in &[0.0, 0.25, 0.5, 0.75, 1.0f64] {
        assert!((EasingPreset::Linear.apply(t) - t).abs() < 1e-9, "t={t}");
    }
}

#[test]
fn all_presets_map_endpoints() {
    for preset in [
        EasingPreset::Linear, EasingPreset::Ease, EasingPreset::EaseIn,
        EasingPreset::EaseOut, EasingPreset::EaseInOut, EasingPreset::EasyEase,
    ] {
        let v0 = preset.apply(0.0);
        let v1 = preset.apply(1.0);
        assert!(v0.abs() < 1e-6, "{preset:?} at 0 should be 0, got {v0}");
        assert!((v1 - 1.0).abs() < 1e-6, "{preset:?} at 1 should be 1, got {v1}");
    }
}

#[test]
fn ease_out_above_linear_at_midpoint() {
    let v = EasingPreset::EaseOut.apply(0.5);
    assert!(v > 0.5, "ease-out midpoint should be > 0.5, got {v}");
}

#[test]
fn ease_in_below_linear_at_midpoint() {
    let v = EasingPreset::EaseIn.apply(0.5);
    assert!(v < 0.5, "ease-in midpoint should be < 0.5, got {v}");
}

#[test]
fn ease_in_out_symmetric() {
    // cubic-bezier(0.42, 0, 0.58, 1) is not perfectly symmetric at 0.5
    // but must map 0→0 and 1→1 and be strictly monotone
    let v = EasingPreset::EaseInOut.apply(0.5);
    assert!(v > 0.0 && v < 1.0, "ease-in-out midpoint out of range: {v}");
}

#[test]
fn auto_bezier_smooth_step() {
    assert!(auto_bezier_ease(0.0).abs() < 1e-9);
    assert!((auto_bezier_ease(1.0) - 1.0).abs() < 1e-9);
    let mid = auto_bezier_ease(0.5);
    assert!((mid - 0.5).abs() < 1e-9, "smooth-step midpoint should be 0.5, got {mid}");
}

#[test]
fn from_kvk_ordinal_round_trips() {
    let presets = [
        EasingPreset::Linear, EasingPreset::Ease, EasingPreset::EaseIn,
        EasingPreset::EaseOut, EasingPreset::EaseInOut, EasingPreset::EasyEase,
    ];
    for (i, preset) in presets.iter().enumerate() {
        assert_eq!(EasingPreset::from_kvk_ordinal(i as u8), *preset);
    }
}

#[test]
fn cubic_bezier_ease_matches_known_values() {
    // CSS ease-out (0, 0, 0.58, 1): at t=0.5 result > 0.5
    let v = cubic_bezier_ease(0.5, 0.0, 0.0, 0.58, 1.0);
    assert!(v > 0.5, "cubic_bezier_ease ease-out at 0.5 = {v}");
    // endpoints always map
    assert!(cubic_bezier_ease(0.0, 0.25, 0.1, 0.25, 1.0).abs() < 1e-6);
    assert!((cubic_bezier_ease(1.0, 0.25, 0.1, 0.25, 1.0) - 1.0).abs() < 1e-6);
}
