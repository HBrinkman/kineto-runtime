//! Cross-runtime camera parity (issue #202, ADR 0010).
//!
//! The editor previews a camera move — position, zoom and rotation, each
//! eased differently, on an artboard away from the document origin — and
//! writes the camera affine it draws through at a handful of times into
//! `test/fixtures/camera/camera_parity.txt`, next to the `.kvk` it exports.
//! This plays the same `.kvk` and requires the runtime to frame the scene
//! the same way at every one of those times.

use kineto_engine::format;
use kineto_engine::player::KinetoPlayer;
use kineto_engine::render::vello_scene::camera_affine;

const EXPECTED: &str = include_str!("../../../test/fixtures/camera/camera_parity.txt");

fn parse() -> ((f64, f64), Vec<(f64, [f64; 6])>) {
    let mut canvas = (0.0, 0.0);
    let mut frames = Vec::new();
    for line in EXPECTED.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<f64> = line
            .split_whitespace()
            .skip(1)
            .map(|v| v.parse().unwrap())
            .collect();
        if line.starts_with("canvas") {
            canvas = (parts[0], parts[1]);
        } else if line.starts_with("frame") {
            frames.push((
                parts[0],
                [parts[1], parts[2], parts[3], parts[4], parts[5], parts[6]],
            ));
        } else {
            panic!("unexpected line {line}");
        }
    }
    (canvas, frames)
}

#[test]
fn the_runtime_frames_the_scene_where_the_editor_does() {
    let bytes = include_bytes!("fixtures/camera_parity.kvk");
    let (canvas, frames) = parse();
    assert!(!frames.is_empty(), "the fixture table is empty");

    let mut player = KinetoPlayer::load(bytes.to_vec()).expect("player should load");
    player.play().expect("the clip should start");
    player.pause();
    for (time, want) in frames {
        let frame = player.seek(time).expect("frame should evaluate");
        let document = format::load(bytes).expect("document should parse");
        let got = camera_affine(&document, &frame, canvas);
        for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
            // f32 storage in the package; a thousandth of a pixel or scale
            // unit is far below anything visible.
            assert!(
                (g - w).abs() < 1e-3,
                "t={time} component {i}: runtime {g:.6} != editor {w:.6} ({got:?} vs {want:?})"
            );
        }
    }
}
