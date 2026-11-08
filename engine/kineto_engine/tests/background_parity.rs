//! Cross-runtime artboard background parity (issue #233, ADR-0011).
//!
//! The editor exports four documents — a half-transparent colour, an opaque
//! one, a colour with "Include background" off, and a legacy artboard with
//! none — and writes the straight RGBA each runtime must clear to into
//! `test/fixtures/background/background_parity.txt`. This loads the same
//! `.kvk` files and requires the engine to agree, through the render scene
//! (what the vello surface clears to), the player accessor and the C ABI the
//! Swift package reads.

use kineto_engine::format;
use kineto_engine::player::KinetoPlayer;
use kineto_engine::render::vello_scene::build_render_scene;

const EXPECTED: &str = include_str!("../../../test/fixtures/background/background_parity.txt");

fn fixture(name: &str) -> &'static [u8] {
    match name {
        "background_half" => include_bytes!("fixtures/background_half.kvk"),
        "background_opaque" => include_bytes!("fixtures/background_opaque.kvk"),
        "background_off" => include_bytes!("fixtures/background_off.kvk"),
        "background_legacy" => include_bytes!("fixtures/background_legacy.kvk"),
        other => panic!("no fixture {other}"),
    }
}

fn expectations() -> Vec<(&'static str, Option<[u8; 4]>)> {
    EXPECTED
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next().unwrap();
            let rest: Vec<&str> = parts.collect();
            let rgba = if rest == ["none"] {
                None
            } else {
                let v: Vec<u8> = rest.iter().map(|p| p.parse().unwrap()).collect();
                Some([v[0], v[1], v[2], v[3]])
            };
            (name, rgba)
        })
        .collect()
}

#[test]
fn the_runtime_clears_to_the_background_the_editor_exported() {
    let table = expectations();
    assert_eq!(table.len(), 4, "the table lists every fixture");
    for (name, want) in table {
        let bytes = fixture(name);
        let mut player = KinetoPlayer::load(bytes.to_vec()).expect("player should load");
        let frame = player.seek(0.0).expect("frame should evaluate");
        let document = format::load(bytes).expect("document should parse");

        let scene = build_render_scene(document, &frame);

        assert_eq!(scene.background, want, "{name}: render scene");
        assert_eq!(
            player.background_color().unwrap(),
            want.unwrap_or([0; 4]),
            "{name}: player accessor"
        );
    }
}

#[test]
fn a_legacy_file_without_the_field_is_transparent() {
    // `background_legacy` has no background written: the vtable slot is 0.
    // (The regenerated builder's vtable is one slot longer than before, so
    // the bytes are not identical to a pre-#233 export — they read the same.)
    let bytes = fixture("background_legacy");
    let document = format::load(bytes).expect("document should parse");
    assert!(document.background().is_none());
}

#[test]
fn excluded_and_absent_backgrounds_are_the_same_bytes() {
    assert_eq!(fixture("background_off"), fixture("background_legacy"));
}

#[test]
fn the_c_abi_hands_swift_the_same_straight_rgba() {
    use kineto_engine::ffi::c::{kineto_background_color, kineto_destroy, kineto_load, KinetoStatus};

    for (name, want) in expectations() {
        let bytes = fixture(name);
        let mut status = KinetoStatus::LoadFailed;
        let player = unsafe { kineto_load(bytes.as_ptr(), bytes.len(), &mut status) };
        assert!(!player.is_null(), "{name}: load");

        let mut rgba = [9u8; 4];
        let status = unsafe { kineto_background_color(player, rgba.as_mut_ptr()) };

        assert_eq!(status, KinetoStatus::Ok, "{name}");
        assert_eq!(rgba, want.unwrap_or([0; 4]), "{name}: C ABI");
        assert_eq!(
            unsafe { kineto_background_color(player, std::ptr::null_mut()) },
            KinetoStatus::NullPointer
        );
        unsafe { kineto_destroy(player) };
    }
}
