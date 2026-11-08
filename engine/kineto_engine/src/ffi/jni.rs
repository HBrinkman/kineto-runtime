use std::ffi::CString;
use std::os::raw::c_char;
use std::ptr;

use jni::objects::{JByteArray, JClass, JDoubleArray, JIntArray, JObject, JString};
use jni::sys::{jboolean, jdouble, jint, jlong, jstring};
use jni::JNIEnv;

use super::c::{
    kineto_active_audio_clips, kineto_advance, kineto_animations, kineto_audio_assets,
    kineto_audio_events, kineto_background_color, kineto_destroy, kineto_document_point, kineto_get_active_state,
    kineto_get_active_transition, kineto_get_transition_progress, kineto_hit_test_document_point,
    kineto_last_error_message, kineto_load, kineto_machine_inputs, kineto_override_scalar,
    kineto_pause, kineto_play, kineto_pointer_event, kineto_pointer_moved, kineto_render,
    kineto_retain,
    kineto_select_animation, kineto_set_bool, kineto_set_number, kineto_set_trigger,
    kineto_state_machines, kineto_surface_create_from_android_native_window,
    kineto_surface_destroy, kineto_surface_last_error_message, kineto_surface_resize,
    kineto_take_fired_audio_events, KinetoPlayerHandle, KinetoPointerEvent, KinetoStatus,
};

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeLoad(
    mut env: JNIEnv,
    _class: JClass,
    bytes: JByteArray,
    out_status: JIntArray,
) -> jlong {
    let Ok(bytes) = env.convert_byte_array(bytes) else {
        write_status(&mut env, &out_status, KinetoStatus::InvalidArgument);
        return 0;
    };
    let mut status = KinetoStatus::LoadFailed;
    let handle = unsafe { kineto_load(bytes.as_ptr(), bytes.len(), &mut status) };
    write_status(&mut env, &out_status, status);
    if status == KinetoStatus::Ok {
        handle as jlong
    } else {
        0
    }
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeDestroy(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    unsafe { kineto_destroy(handle_from_jlong(handle)) };
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeAdvance(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    delta_seconds: jdouble,
) -> jint {
    unsafe { kineto_advance(handle_from_jlong(handle), delta_seconds) as jint }
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeSetBool(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    state_machine_id: JString,
    input_name: JString,
    value: jboolean,
) -> jint {
    with_c_strings(
        &mut env,
        state_machine_id,
        input_name,
        |machine, input| unsafe {
            kineto_set_bool(
                handle_from_jlong(handle),
                machine.as_ptr(),
                input.as_ptr(),
                value != 0,
            ) as jint
        },
    )
    .unwrap_or(KinetoStatus::InvalidArgument as jint)
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeSetNumber(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    state_machine_id: JString,
    input_name: JString,
    value: jdouble,
) -> jint {
    with_c_strings(
        &mut env,
        state_machine_id,
        input_name,
        |machine, input| unsafe {
            kineto_set_number(
                handle_from_jlong(handle),
                machine.as_ptr(),
                input.as_ptr(),
                value,
            ) as jint
        },
    )
    .unwrap_or(KinetoStatus::InvalidArgument as jint)
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativePointerEvent(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    event: jint,
    x: jdouble,
    y: jdouble,
) -> jint {
    let event = match event {
        0 => KinetoPointerEvent::Down,
        1 => KinetoPointerEvent::Up,
        2 => KinetoPointerEvent::Enter,
        3 => KinetoPointerEvent::Exit,
        _ => return -(KinetoStatus::InvalidArgument as jint),
    };
    let mut fired = 0u32;
    let status = unsafe { kineto_pointer_event(handle_from_jlong(handle), event, x, y, &mut fired) };
    // Negative is a failed status; the host reads a count and a status from
    // one return value.
    if status == KinetoStatus::Ok {
        fired as jint
    } else {
        -(status as jint)
    }
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeSetTrigger(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    state_machine_id: JString,
    input_name: JString,
) -> jint {
    with_c_strings(
        &mut env,
        state_machine_id,
        input_name,
        |machine, input| unsafe {
            kineto_set_trigger(handle_from_jlong(handle), machine.as_ptr(), input.as_ptr()) as jint
        },
    )
    .unwrap_or(KinetoStatus::InvalidArgument as jint)
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeActiveState(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    state_machine_id: JString,
) -> jstring {
    copy_state_machine_string(&mut env, handle, state_machine_id, kineto_get_active_state)
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeActiveTransition(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    state_machine_id: JString,
) -> jstring {
    copy_state_machine_string(
        &mut env,
        handle,
        state_machine_id,
        kineto_get_active_transition,
    )
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeTransitionProgress(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    state_machine_id: JString,
    out_status: JIntArray,
) -> jdouble {
    let Ok(machine) = c_string(&mut env, state_machine_id) else {
        write_status(&mut env, &out_status, KinetoStatus::InvalidArgument);
        return 0.0;
    };
    let mut status = KinetoStatus::InvalidArgument;
    let progress = unsafe {
        kineto_get_transition_progress(handle_from_jlong(handle), machine.as_ptr(), &mut status)
    };
    write_status(&mut env, &out_status, status);
    progress
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeHitTestDocumentPoint(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    x: jdouble,
    y: jdouble,
) -> jstring {
    let required = unsafe {
        kineto_hit_test_document_point(handle_from_jlong(handle), x, y, ptr::null_mut(), 0)
    };
    if required <= 1 {
        return ptr::null_mut();
    }
    let mut buffer = vec![0 as c_char; required];
    let copied = unsafe {
        kineto_hit_test_document_point(
            handle_from_jlong(handle),
            x,
            y,
            buffer.as_mut_ptr(),
            buffer.len(),
        )
    };
    if copied != required {
        return ptr::null_mut();
    }
    new_jstring(&mut env, c_buffer_to_string(&buffer))
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeLastError(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    let message = unsafe {
        let ptr = kineto_last_error_message(handle_from_jlong(handle));
        if ptr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    };
    new_jstring(&mut env, message)
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeAttachSurface(
    env: JNIEnv,
    _class: JClass,
    surface: JObject,
    width: jint,
    height: jint,
) -> jlong {
    if width <= 0 || height <= 0 {
        return 0;
    }
    let window = unsafe {
        ndk_sys::ANativeWindow_fromSurface(env.get_native_interface().cast(), surface.as_raw())
    };
    if window.is_null() {
        return 0;
    }

    let mut status = KinetoStatus::Ok;
    let render_surface = unsafe {
        kineto_surface_create_from_android_native_window(
            window.cast(),
            width as u32,
            height as u32,
            &mut status,
        )
    };
    unsafe { ndk_sys::ANativeWindow_release(window) };
    if status == KinetoStatus::Ok {
        render_surface as jlong
    } else {
        0
    }
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeDestroySurface(
    _env: JNIEnv,
    _class: JClass,
    surface_handle: jlong,
) {
    unsafe { kineto_surface_destroy(surface_handle as *mut std::ffi::c_void) };
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeResizeSurface(
    _env: JNIEnv,
    _class: JClass,
    surface_handle: jlong,
    width: jint,
    height: jint,
) -> jint {
    if width <= 0 || height <= 0 {
        return KinetoStatus::InvalidArgument as jint;
    }
    unsafe {
        kineto_surface_resize(
            surface_handle as *mut std::ffi::c_void,
            width as u32,
            height as u32,
        ) as jint
    }
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeRender(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    surface_handle: jlong,
) -> jint {
    unsafe {
        kineto_render(
            handle_from_jlong(handle),
            surface_handle as *mut std::ffi::c_void,
        ) as jint
    }
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeSurfaceLastError(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let message = unsafe {
        let ptr = kineto_surface_last_error_message();
        if ptr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    };
    new_jstring(&mut env, message)
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeRetain(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    unsafe { kineto_retain(handle_from_jlong(handle)) as jint }
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativePointerMoved(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    x: jdouble,
    y: jdouble,
) -> jint {
    unsafe { kineto_pointer_moved(handle_from_jlong(handle), x, y) as jint }
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeBackgroundColor(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    out_rgba: JIntArray,
) -> jint {
    let mut rgba = [0u8; 4];
    let status = unsafe { kineto_background_color(handle_from_jlong(handle), rgba.as_mut_ptr()) };
    if status == KinetoStatus::Ok
        && env
            .set_int_array_region(&out_rgba, 0, &rgba.map(jint::from))
            .is_err()
    {
        return KinetoStatus::InvalidArgument as jint;
    }
    status as jint
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeDocumentPoint(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    surface_width: jdouble,
    surface_height: jdouble,
    x: jdouble,
    y: jdouble,
    out_point: JDoubleArray,
) -> jint {
    let (mut dx, mut dy) = (0.0, 0.0);
    let status = unsafe {
        kineto_document_point(
            handle_from_jlong(handle),
            surface_width,
            surface_height,
            x,
            y,
            &mut dx,
            &mut dy,
        )
    };
    if status == KinetoStatus::Ok
        && env.set_double_array_region(&out_point, 0, &[dx, dy]).is_err()
    {
        return KinetoStatus::InvalidArgument as jint;
    }
    status as jint
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeOverrideScalar(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    node_id: JString,
    key: JString,
    out_value: JDoubleArray,
) -> jint {
    let mut value = 0.0;
    let status = with_c_strings(&mut env, node_id, key, |node, key| unsafe {
        kineto_override_scalar(handle_from_jlong(handle), node.as_ptr(), key.as_ptr(), &mut value)
            as jint
    })
    .unwrap_or(KinetoStatus::InvalidArgument as jint);
    if status == KinetoStatus::Ok as jint
        && env.set_double_array_region(&out_value, 0, &[value]).is_err()
    {
        return KinetoStatus::InvalidArgument as jint;
    }
    status
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativePlay(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    unsafe { kineto_play(handle_from_jlong(handle)) as jint }
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativePause(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    unsafe { kineto_pause(handle_from_jlong(handle)) as jint }
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeSelectAnimation(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    animation_id: JString,
) -> jint {
    let Ok(id) = c_string(&mut env, animation_id) else {
        return KinetoStatus::InvalidArgument as jint;
    };
    unsafe { kineto_select_animation(handle_from_jlong(handle), id.as_ptr()) as jint }
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeStateMachines(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    let records = unsafe { kineto_state_machines(handle_from_jlong(handle)) };
    records_to_jstring(&mut env, records)
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeMachineInputs(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    machine_id: JString,
) -> jstring {
    let Ok(machine) = c_string(&mut env, machine_id) else {
        return new_jstring(&mut env, String::new());
    };
    let records = unsafe { kineto_machine_inputs(handle_from_jlong(handle), machine.as_ptr()) };
    records_to_jstring(&mut env, records)
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeAnimations(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    let records = unsafe { kineto_animations(handle_from_jlong(handle)) };
    records_to_jstring(&mut env, records)
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeAudioAssets(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    let records = unsafe { kineto_audio_assets(handle_from_jlong(handle)) };
    records_to_jstring(&mut env, records)
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeActiveAudioClips(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    let records = unsafe { kineto_active_audio_clips(handle_from_jlong(handle)) };
    records_to_jstring(&mut env, records)
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeTakeFiredAudioEvents(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    let records =
        unsafe { kineto_take_fired_audio_events(handle_from_jlong(handle), ptr::null_mut()) };
    records_to_jstring(&mut env, records)
}

#[no_mangle]
pub extern "system" fn Java_app_kineto_runtime_KinetoRuntimeJni_nativeAudioEvents(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    let records = unsafe { kineto_audio_events(handle_from_jlong(handle)) };
    records_to_jstring(&mut env, records)
}

fn write_status(env: &mut JNIEnv, out_status: &JIntArray, status: KinetoStatus) {
    // A failed write leaves a Java exception pending, which is what the host
    // should see: the array it passed was unusable.
    let _ = env.set_int_array_region(out_status, 0, &[status as jint]);
}

fn records_to_jstring(env: &mut JNIEnv, records: *const c_char) -> jstring {
    let records = if records.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(records) }
            .to_string_lossy()
            .into_owned()
    };
    new_jstring(env, records)
}

fn handle_from_jlong(handle: jlong) -> *mut KinetoPlayerHandle {
    handle as *mut KinetoPlayerHandle
}

fn c_string(env: &mut JNIEnv, value: JString) -> Result<CString, ()> {
    let value = env.get_string(&value).map_err(|_| ())?;
    CString::new(value.to_string_lossy().into_owned()).map_err(|_| ())
}

fn with_c_strings(
    env: &mut JNIEnv,
    first: JString,
    second: JString,
    body: impl FnOnce(CString, CString) -> jint,
) -> Result<jint, ()> {
    let first = c_string(env, first)?;
    let second = c_string(env, second)?;
    Ok(body(first, second))
}

fn copy_state_machine_string(
    env: &mut JNIEnv,
    handle: jlong,
    state_machine_id: JString,
    copy: unsafe extern "C" fn(
        *const KinetoPlayerHandle,
        *const std::os::raw::c_char,
        *mut c_char,
        usize,
    ) -> usize,
) -> jstring {
    let Ok(machine) = c_string(env, state_machine_id) else {
        return ptr::null_mut();
    };
    let required = unsafe {
        copy(
            handle_from_jlong(handle),
            machine.as_ptr(),
            ptr::null_mut(),
            0,
        )
    };
    if required <= 1 {
        return ptr::null_mut();
    }
    let mut buffer = vec![0 as c_char; required];
    let copied = unsafe {
        copy(
            handle_from_jlong(handle),
            machine.as_ptr(),
            buffer.as_mut_ptr(),
            buffer.len(),
        )
    };
    if copied != required {
        return ptr::null_mut();
    }
    new_jstring(env, c_buffer_to_string(&buffer))
}

fn c_buffer_to_string(buffer: &[c_char]) -> String {
    let bytes: Vec<u8> = buffer
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .map(|byte| byte as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn new_jstring(env: &mut JNIEnv, value: String) -> jstring {
    env.new_string(value)
        .map(|value| value.into_raw())
        .unwrap_or(ptr::null_mut())
}
