//! Issue #111: a state machine names sounds, and the runtime reports them.
//!
//! The engine never plays anything — a `.kvk` carries no audio bytes, so
//! "firing a sound" means telling the host which sound fired, in order, once.

use kineto_engine::format::generated::kineto::{
    finish_document_buffer, AudioEvent, AudioEventArgs, Document, DocumentArgs, IkTarget,
    IkTargetArgs, InputKind, ListenerActionKind, ListenerEventKind, SMListener, SMListenerArgs,
    SMState, SMStateArgs, SMTransition, SMTransitionArgs, StateMachine, StateMachineArgs,
    StateMachineInput, StateMachineInputArgs, TransitionCondition, TransitionConditionArgs,
    TransitionConditionKind,
};
use kineto_engine::player::KinetoPlayer;

/// One interactive target, one trigger input, Idle → Pressed on that trigger,
/// and whatever sounds the caller asks for.
fn build_doc(transition_sounds: &[&str], listener_sound: Option<&str>) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(2048);

    let target_id = fbb.create_string("press-area");
    let chain_root = fbb.create_string("root");
    let target = IkTarget::create(
        &mut fbb,
        &IkTargetArgs {
            id: Some(target_id),
            chain_root_id: Some(chain_root),
            position_x: 200.0,
            position_y: 200.0,
            extent_w: 200.0,
            extent_h: 100.0,
            interactive: true,
            ..Default::default()
        },
    );
    let targets = fbb.create_vector(&[target]);

    let input_id = fbb.create_string("input-press");
    let input_name = fbb.create_string("press");
    let input = StateMachineInput::create(
        &mut fbb,
        &StateMachineInputArgs {
            id: Some(input_id),
            name: Some(input_name),
            kind: InputKind::Trigger,
            ..Default::default()
        },
    );
    let inputs = fbb.create_vector(&[input]);

    let idle_id = fbb.create_string("idle");
    let idle_name = fbb.create_string("Idle");
    let idle_clip = fbb.create_string("clip-1");
    let idle = SMState::create(
        &mut fbb,
        &SMStateArgs {
            id: Some(idle_id),
            name: Some(idle_name),
            clip_id: Some(idle_clip),
            speed: 1.0,
            ..Default::default()
        },
    );
    let pressed_id = fbb.create_string("pressed");
    let pressed_name = fbb.create_string("Pressed");
    let pressed_clip = fbb.create_string("clip-1");
    let pressed = SMState::create(
        &mut fbb,
        &SMStateArgs {
            id: Some(pressed_id),
            name: Some(pressed_name),
            clip_id: Some(pressed_clip),
            speed: 1.0,
            ..Default::default()
        },
    );
    let states = fbb.create_vector(&[idle, pressed]);

    let condition_input = fbb.create_string("input-press");
    let condition = TransitionCondition::create(
        &mut fbb,
        &TransitionConditionArgs {
            kind: TransitionConditionKind::TriggerFired,
            input_id: Some(condition_input),
            ..Default::default()
        },
    );
    let conditions = fbb.create_vector(&[condition]);

    let sound_offsets: Vec<_> = transition_sounds
        .iter()
        .map(|id| fbb.create_string(id))
        .collect();
    let sounds_vec = if sound_offsets.is_empty() {
        None
    } else {
        Some(fbb.create_vector(&sound_offsets))
    };

    let transition_id = fbb.create_string("t-press");
    let from = fbb.create_string("idle");
    let to = fbb.create_string("pressed");
    let transition = SMTransition::create(
        &mut fbb,
        &SMTransitionArgs {
            id: Some(transition_id),
            from_state_id: Some(from),
            to_state_id: Some(to),
            conditions: Some(conditions),
            duration_seconds: 0.0,
            enabled: true,
            audio_event_ids: sounds_vec,
            ..Default::default()
        },
    );
    let transitions = fbb.create_vector(&[transition]);

    let listeners_vec = listener_sound.map(|sound| {
        let listener_id = fbb.create_string("listener-tap");
        let subject = fbb.create_string("press-area");
        let event_id = fbb.create_string(sound);
        let listener = SMListener::create(
            &mut fbb,
            &SMListenerArgs {
                id: Some(listener_id),
                subject_id: Some(subject),
                // No input at all: this is the one action that writes none.
                input_id: None,
                event: ListenerEventKind::PointerDown,
                action: ListenerActionKind::PlayAudio,
                audio_event_id: Some(event_id),
                ..Default::default()
            },
        );
        fbb.create_vector(&[listener])
    });

    let machine_id = fbb.create_string("machine");
    let machine_name = fbb.create_string("Button");
    let initial = fbb.create_string("idle");
    let machine = StateMachine::create(
        &mut fbb,
        &StateMachineArgs {
            id: Some(machine_id),
            name: Some(machine_name),
            initial_state_id: Some(initial),
            states: Some(states),
            transitions: Some(transitions),
            inputs: Some(inputs),
            listeners: listeners_vec,
            ..Default::default()
        },
    );
    let machines = fbb.create_vector(&[machine]);

    let click_id = fbb.create_string("event-click");
    let click_name = fbb.create_string("ClickSound");
    let click_asset = fbb.create_string("asset-click");
    let click_source = fbb.create_string("click.wav");
    let click = AudioEvent::create(
        &mut fbb,
        &AudioEventArgs {
            id: Some(click_id),
            name: Some(click_name),
            asset_id: Some(click_asset),
            source_name: Some(click_source),
        },
    );
    let pop_id = fbb.create_string("event-pop");
    let pop_name = fbb.create_string("PopSound");
    let pop_asset = fbb.create_string("asset-pop");
    let pop = AudioEvent::create(
        &mut fbb,
        &AudioEventArgs {
            id: Some(pop_id),
            name: Some(pop_name),
            asset_id: Some(pop_asset),
            source_name: None,
        },
    );
    let audio_events = fbb.create_vector(&[click, pop]);

    let name = fbb.create_string("Sounds");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            version: 1,
            name: Some(name),
            canvas_width: 800.0,
            canvas_height: 600.0,
            frame_rate: 60,
            ik_targets: Some(targets),
            state_machines: Some(machines),
            audio_events: Some(audio_events),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

#[test]
fn a_transition_reports_its_sounds_when_it_starts() {
    let mut player = KinetoPlayer::load(build_doc(&["event-click"], None)).unwrap();

    player.advance(0.016).unwrap();
    assert!(
        player.take_fired_audio_events().is_empty(),
        "nothing has fired the transition yet"
    );

    player.set_trigger("machine", "press").unwrap();
    player.advance(0.016).unwrap();

    assert_eq!(player.take_fired_audio_events(), vec!["event-click"]);
}

#[test]
fn taking_the_firings_drains_them() {
    let mut player = KinetoPlayer::load(build_doc(&["event-click"], None)).unwrap();

    player.set_trigger("machine", "press").unwrap();
    player.advance(0.016).unwrap();
    assert_eq!(player.take_fired_audio_events().len(), 1);

    // A host that polls every frame must not be handed the same firing twice.
    player.advance(0.016).unwrap();
    assert!(player.take_fired_audio_events().is_empty());
}

#[test]
fn a_transition_reports_several_sounds_in_order() {
    let mut player =
        KinetoPlayer::load(build_doc(&["event-click", "event-pop"], None)).unwrap();

    player.set_trigger("machine", "press").unwrap();
    player.advance(0.016).unwrap();

    assert_eq!(
        player.take_fired_audio_events(),
        vec!["event-click", "event-pop"]
    );
}

#[test]
fn a_pointer_listener_reports_its_sound_and_writes_no_input() {
    let mut player = KinetoPlayer::load(build_doc(&[], Some("event-click"))).unwrap();

    let fired = player
        .pointer_event(ListenerEventKind::PointerDown, 200.0, 200.0)
        .unwrap();

    assert_eq!(fired, vec!["listener-tap"]);
    assert_eq!(player.take_fired_audio_events(), vec!["event-click"]);
}

#[test]
fn two_taps_are_two_firings() {
    let mut player = KinetoPlayer::load(build_doc(&[], Some("event-click"))).unwrap();

    player
        .pointer_event(ListenerEventKind::PointerDown, 200.0, 200.0)
        .unwrap();
    player
        .pointer_event(ListenerEventKind::PointerDown, 200.0, 200.0)
        .unwrap();

    // Never de-duplicated: that is what a double-tap sounds like.
    assert_eq!(
        player.take_fired_audio_events(),
        vec!["event-click", "event-click"]
    );
}

#[test]
fn the_host_can_read_what_each_sound_is() {
    let player = KinetoPlayer::load(build_doc(&["event-click"], None)).unwrap();

    let events = player.audio_events().unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, "event-click");
    assert_eq!(events[0].1, "ClickSound");
    assert_eq!(events[0].2.as_deref(), Some("click.wav"));
    // A sound authored without a breadcrumb reports none.
    assert_eq!(events[1].2, None);
}

// ── The C ABI a Swift or Android host actually calls (issue #111) ────────────

#[test]
fn the_c_abi_hands_the_host_the_firings_and_the_catalogue() {
    use kineto_engine::ffi::c::{
        kineto_audio_events, kineto_destroy, kineto_load, kineto_pointer_event,
        kineto_take_fired_audio_events, KinetoPointerEvent, KinetoStatus,
    };
    use std::ffi::CStr;

    let bytes = build_doc(&[], Some("event-click"));
    let mut status = KinetoStatus::LoadFailed;
    let player = unsafe { kineto_load(bytes.as_ptr(), bytes.len(), &mut status) };
    assert_eq!(status, KinetoStatus::Ok);

    // Nothing fired yet: an empty string and a zero count, not a null.
    let mut count: u32 = 7;
    let joined = unsafe { kineto_take_fired_audio_events(player, &mut count) };
    assert_eq!(unsafe { CStr::from_ptr(joined) }.to_str().unwrap(), "");
    assert_eq!(count, 0);

    let mut fired: u32 = 0;
    assert_eq!(
        unsafe {
            kineto_pointer_event(player, KinetoPointerEvent::Down, 200.0, 200.0, &mut fired)
        },
        KinetoStatus::Ok
    );

    let joined = unsafe { kineto_take_fired_audio_events(player, &mut count) };
    assert_eq!(
        unsafe { CStr::from_ptr(joined) }.to_str().unwrap(),
        "event-click"
    );
    assert_eq!(count, 1);

    // Drained: a host polling every frame is not handed it again.
    let joined = unsafe { kineto_take_fired_audio_events(player, &mut count) };
    assert_eq!(unsafe { CStr::from_ptr(joined) }.to_str().unwrap(), "");
    assert_eq!(count, 0);

    // The catalogue: `id\tname\tsourceName`, with an empty third field for a
    // sound that carries no breadcrumb.
    let catalogue = unsafe { kineto_audio_events(player) };
    assert_eq!(
        unsafe { CStr::from_ptr(catalogue) }.to_str().unwrap(),
        "event-click\tClickSound\tclick.wav\nevent-pop\tPopSound\t"
    );

    unsafe { kineto_destroy(player) };
}

// ── Introspection a reference app needs to show a document (issue #111) ──────

#[test]
fn the_host_can_enumerate_machines_inputs_and_clips() {
    let player = KinetoPlayer::load(build_doc(&["event-click"], None)).unwrap();

    let machines = player.state_machines().unwrap();
    assert_eq!(machines, vec![("machine".to_owned(), "Button".to_owned())]);

    let inputs = player.machine_inputs("machine").unwrap();
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].0, "input-press");
    assert_eq!(inputs[0].1, "press");
    // 1 == trigger, the schema ordinal (Bool 0, Trigger 1, Number 2).
    assert_eq!(inputs[0].2, 1);

    // A machine nobody carries reads the same as one with no inputs.
    assert!(player.machine_inputs("nope").unwrap().is_empty());

    // This fixture has no animation clips, and that is an empty list rather
    // than an error.
    assert!(player.animations().unwrap().is_empty());
}

#[test]
fn the_c_abi_exposes_the_same_introspection() {
    use kineto_engine::ffi::c::{
        kineto_destroy, kineto_load, kineto_machine_inputs, kineto_state_machines, KinetoStatus,
    };
    use std::ffi::{CStr, CString};

    let bytes = build_doc(&["event-click"], None);
    let mut status = KinetoStatus::LoadFailed;
    let player = unsafe { kineto_load(bytes.as_ptr(), bytes.len(), &mut status) };
    assert_eq!(status, KinetoStatus::Ok);

    let machines = unsafe { kineto_state_machines(player) };
    assert_eq!(
        unsafe { CStr::from_ptr(machines) }.to_str().unwrap(),
        "machine\tButton"
    );

    let machine_id = CString::new("machine").unwrap();
    let inputs = unsafe { kineto_machine_inputs(player, machine_id.as_ptr()) };
    assert_eq!(
        unsafe { CStr::from_ptr(inputs) }.to_str().unwrap(),
        "input-press\tpress\t1\t0\t0"
    );

    unsafe { kineto_destroy(player) };
}

// ── The bridge the reference app actually crosses (issue #111 device QA) ─────
//
// Every test above builds its buffer with the Rust FlatBuffers builders, and
// the Dart side tests its own round trip. Neither covers *Dart wrote it, Rust
// reads it* — which is the only path a `.kvk` from the editor ever takes, and
// the one the first iPad session was blocked on.

#[test]
fn a_document_exported_by_the_editor_carries_its_sounds_into_the_runtime() {
    let bytes = include_bytes!("fixtures/sounds_demo.kvk");
    let mut player = KinetoPlayer::load(bytes.to_vec()).unwrap();

    // The catalogue the host maps onto its own files.
    let events = player.audio_events().unwrap();
    assert_eq!(
        events
            .iter()
            .map(|(id, name, _)| (id.as_str(), name.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("event-click", "ClickSound"),
            ("event-success", "Success"),
        ]
    );
    assert_eq!(events[0].2.as_deref(), Some("click.wav"));

    // One machine, and it is the one the fixture names.
    let machines = player.state_machines().unwrap();
    assert_eq!(machines.len(), 1);
    assert_eq!(machines[0].0, "machine-button");

    // Firing the trigger takes the transition *and* reports its sound.
    player.set_trigger("machine-button", "press").unwrap();
    player.advance(0.016).unwrap();
    assert_eq!(player.take_fired_audio_events(), vec!["event-click"]);

    // The way back fires the other sound after its exit time, which is what
    // lets a tester tell two sounds apart by ear.
    player.advance(1.1).unwrap();
    assert_eq!(player.take_fired_audio_events(), vec!["event-success"]);
}

#[test]
fn a_tap_on_the_exported_document_reports_its_sound() {
    let bytes = include_bytes!("fixtures/sounds_demo.kvk");
    let mut player = KinetoPlayer::load(bytes.to_vec()).unwrap();

    // The shape sits at (300,240) with a 200x120 box, so its middle is
    // (400,300) in document space.
    let fired = player
        .pointer_event(ListenerEventKind::PointerDown, 400.0, 300.0)
        .unwrap();

    assert_eq!(fired, vec!["listener-tap"]);
    assert_eq!(player.take_fired_audio_events(), vec!["event-click"]);
}

// ── Timeline audio: the soundtrack a clip carries (issue #111) ───────────────

#[test]
fn the_timeline_audio_of_an_exported_clip_reaches_the_runtime() {
    let bytes = include_bytes!("fixtures/sounds_demo.kvk");
    let mut player = KinetoPlayer::load(bytes.to_vec()).unwrap();

    // Nothing is audible before a clip is playing: audio belongs to a
    // timeline, and no timeline is running yet.
    assert!(player.active_audio_clips().unwrap().is_empty());

    player.select_animation("clip-idle").unwrap();
    player.play().unwrap();

    // The track starts at 1s, so at 0s there is still silence.
    assert!(player.active_audio_clips().unwrap().is_empty());

    // Half a second into the track: the file should be sounding at 0.5s in,
    // at the authored volume.
    player.advance(1.5).unwrap();
    let audible = player.active_audio_clips().unwrap();
    assert_eq!(audible.len(), 1);
    let (track_id, _asset_id, file_name, position, volume) = &audible[0];
    assert_eq!(track_id, "track-1");
    assert_eq!(file_name, "success.wav");
    assert!(
        (position - 0.5).abs() < 1e-3,
        "expected ~0.5s into the file, got {position}"
    );
    assert!((volume - 0.8).abs() < 1e-3);

    // Past the end of a 0.7s file: silence, not a stretched tail.
    player.advance(1.0).unwrap();
    assert!(player.active_audio_clips().unwrap().is_empty());
}

#[test]
fn the_c_abi_reports_the_active_timeline_audio() {
    use kineto_engine::ffi::c::{
        kineto_active_audio_clips, kineto_advance, kineto_destroy, kineto_load, KinetoStatus,
    };
    use std::ffi::CStr;

    let bytes = include_bytes!("fixtures/sounds_demo.kvk");
    let mut status = KinetoStatus::LoadFailed;
    let player = unsafe { kineto_load(bytes.as_ptr(), bytes.len(), &mut status) };
    assert_eq!(status, KinetoStatus::Ok);

    // Without a clip playing the host is told there is nothing to sound.
    let records = unsafe { kineto_active_audio_clips(player) };
    assert_eq!(unsafe { CStr::from_ptr(records) }.to_str().unwrap(), "");

    unsafe { kineto_destroy(player) };
    let _ = kineto_advance;
}

#[test]
fn the_c_abi_refuses_an_animation_the_document_does_not_have() {
    use kineto_engine::ffi::c::{
        kineto_destroy, kineto_load, kineto_select_animation, KinetoStatus,
    };
    use std::ffi::CString;

    let bytes = include_bytes!("fixtures/sounds_demo.kvk");
    let player = unsafe { kineto_load(bytes.as_ptr(), bytes.len(), std::ptr::null_mut()) };
    let known = CString::new("clip-idle").unwrap();
    let unknown = CString::new("no-such-clip").unwrap();

    assert_eq!(unsafe { kineto_select_animation(player, known.as_ptr()) }, KinetoStatus::Ok);
    assert_eq!(
        unsafe { kineto_select_animation(player, unknown.as_ptr()) },
        KinetoStatus::InvalidArgument
    );

    unsafe { kineto_destroy(player) };
}
