use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

use kineto_engine::ffi::c::{
    kineto_advance, kineto_destroy, kineto_get_active_state, kineto_last_error_message,
    kineto_load, kineto_set_bool, kineto_set_number, kineto_surface_create_from_ca_metal_layer, kineto_surface_resize,
    KinetoStatus,
};
use kineto_engine::format::generated::kineto::{
    finish_document_buffer, Document, DocumentArgs, SMState, SMStateArgs, StateMachine,
    StateMachineArgs,
};

#[test]
fn ffi_loads_advances_and_exposes_active_state() {
    let bytes = build_state_machine_doc();
    let mut status = KinetoStatus::LoadFailed;

    let player = unsafe { kineto_load(bytes.as_ptr(), bytes.len(), &mut status) };
    assert!(!player.is_null());
    assert_eq!(status, KinetoStatus::Ok);

    assert_eq!(unsafe { kineto_advance(player, 0.0) }, KinetoStatus::Ok);

    let machine = CString::new("sm1").unwrap();
    let required = unsafe { kineto_get_active_state(player, machine.as_ptr(), ptr::null_mut(), 0) };
    assert_eq!(required, "idle".len() + 1);

    let mut buffer = vec![0 as c_char; required];
    let copied = unsafe {
        kineto_get_active_state(player, machine.as_ptr(), buffer.as_mut_ptr(), buffer.len())
    };
    assert_eq!(copied, required);
    let state = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_str().unwrap();
    assert_eq!(state, "idle");

    unsafe { kineto_destroy(player) };
}

#[test]
fn ffi_rejects_a_number_on_an_unknown_input() {
    let bytes = build_state_machine_doc();
    let player = unsafe { kineto_load(bytes.as_ptr(), bytes.len(), ptr::null_mut()) };
    assert!(!player.is_null());

    let machine = CString::new("sm1").unwrap();
    let input = CString::new("pointerX").unwrap();
    let status = unsafe { kineto_set_number(player, machine.as_ptr(), input.as_ptr(), 0.5) };
    assert_eq!(status, KinetoStatus::InvalidArgument);

    unsafe { kineto_destroy(player) };
}

#[test]
fn ffi_rejects_a_number_on_a_null_player() {
    let machine = CString::new("sm1").unwrap();
    let input = CString::new("pointerX").unwrap();
    let status = unsafe {
        kineto_set_number(ptr::null_mut(), machine.as_ptr(), input.as_ptr(), 0.5)
    };
    assert_eq!(status, KinetoStatus::NullPointer);
}

#[test]
fn ffi_reports_errors_for_unknown_inputs() {
    let bytes = build_state_machine_doc();
    let player = unsafe { kineto_load(bytes.as_ptr(), bytes.len(), ptr::null_mut()) };
    assert!(!player.is_null());

    let machine = CString::new("sm1").unwrap();
    let input = CString::new("missing").unwrap();
    let status = unsafe { kineto_set_bool(player, machine.as_ptr(), input.as_ptr(), true) };
    assert_eq!(status, KinetoStatus::InvalidArgument);

    let error = unsafe { CStr::from_ptr(kineto_last_error_message(player)) }
        .to_str()
        .unwrap();
    assert!(error.contains("Unknown input name"), "error={error}");

    unsafe { kineto_destroy(player) };
}

#[test]
fn ffi_rejects_invalid_surface_arguments() {
    let mut status = KinetoStatus::Ok;
    let surface = unsafe {
        kineto_surface_create_from_ca_metal_layer(ptr::null_mut(), 100, 100, &mut status)
    };
    assert!(surface.is_null());
    assert_eq!(status, KinetoStatus::NullPointer);

    assert_eq!(
        unsafe { kineto_surface_resize(ptr::null_mut(), 100, 100) },
        KinetoStatus::NullPointer
    );
}

fn build_state_machine_doc() -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(1024);
    let idle = sm_state(&mut fbb, "idle");
    let states = fbb.create_vector(&[idle]);
    let machine_id = fbb.create_string("sm1");
    let machine_name = fbb.create_string("Machine");
    let initial = fbb.create_string("idle");
    let machine = StateMachine::create(
        &mut fbb,
        &StateMachineArgs {
            id: Some(machine_id),
            name: Some(machine_name),
            initial_state_id: Some(initial),
            states: Some(states),
            ..Default::default()
        },
    );
    let machines = fbb.create_vector(&[machine]);
    let name = fbb.create_string("FFI Fixture");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            canvas_width: 100.0,
            canvas_height: 100.0,
            state_machines: Some(machines),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

fn sm_state<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
) -> flatbuffers::WIPOffset<SMState<'a>> {
    let id_offset = fbb.create_string(id);
    let name = fbb.create_string(id);
    let clip_id = fbb.create_string("");
    SMState::create(
        fbb,
        &SMStateArgs {
            id: Some(id_offset),
            name: Some(name),
            clip_id: Some(clip_id),
            ..Default::default()
        },
    )
}

#[test]
fn ffi_reads_an_evaluated_scalar_and_rejects_an_absent_one() {
    use kineto_engine::ffi::c::kineto_override_scalar;

    let bytes = include_bytes!("../../../packages/kineto_runtime/example/assets/pointer_binding.kvk");
    let player = unsafe { kineto_load(bytes.as_ptr(), bytes.len(), ptr::null_mut()) };
    assert!(!player.is_null());

    let node = CString::new("target-hand").unwrap();
    let key = CString::new("ikTargetPositionX").unwrap();
    let absent = CString::new("no-such-key").unwrap();
    let mut value = 0.0;

    let status = unsafe { kineto_override_scalar(player, node.as_ptr(), key.as_ptr(), &mut value) };
    assert_eq!(status, KinetoStatus::Ok);
    assert_eq!(value, 100.0);

    let status =
        unsafe { kineto_override_scalar(player, node.as_ptr(), absent.as_ptr(), &mut value) };
    assert_eq!(status, KinetoStatus::InvalidArgument);

    let status = unsafe { kineto_override_scalar(player, node.as_ptr(), key.as_ptr(), ptr::null_mut()) };
    assert_eq!(status, KinetoStatus::NullPointer);

    unsafe { kineto_destroy(player) };
}

#[test]
fn ffi_keeps_a_retained_player_alive_until_every_owner_lets_go() {
    use kineto_engine::ffi::c::kineto_retain;

    let bytes = build_state_machine_doc();
    let player = unsafe { kineto_load(bytes.as_ptr(), bytes.len(), ptr::null_mut()) };
    assert_eq!(unsafe { kineto_retain(player) }, KinetoStatus::Ok);

    unsafe { kineto_destroy(player) };
    // Still owned by the second reference. Run under Miri or ASan, this would
    // report a use after free if the first destroy had freed it.
    assert_eq!(unsafe { kineto_advance(player, 0.016) }, KinetoStatus::Ok);
    unsafe { kineto_destroy(player) };

    assert_eq!(unsafe { kineto_retain(ptr::null_mut()) }, KinetoStatus::NullPointer);
}
