use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::format;
use crate::player::{EvaluatedFrame, KinetoPlayer};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KinetoStatus {
    Ok = 0,
    NullPointer = 1,
    InvalidArgument = 2,
    LoadFailed = 3,
    AdvanceFailed = 4,
    RenderFailed = 5,
    Unsupported = 6,
}

pub struct KinetoPlayerHandle {
    /// Owners of this handle: `kineto_load` makes one, `kineto_retain` adds one,
    /// `kineto_destroy` gives one up.
    references: AtomicUsize,
    player: KinetoPlayer,
    #[cfg_attr(
        not(any(feature = "metal-render", feature = "android-render")),
        allow(dead_code)
    )]
    bytes: Vec<u8>,
    last_frame: Option<EvaluatedFrame>,
    last_error: CString,
    /// Newline-joined ids of the sounds fired since the host last collected
    /// them, owned here so the returned pointer outlives the call
    /// (issue #111).
    fired_audio: CString,
    /// Newline-joined `id\tname\tsource_name` records for the document's
    /// sounds, built on demand for the same reason.
    audio_events: CString,
    /// Newline-joined introspection records, built on demand: the document's
    /// machines, one machine's inputs, and its clips. One buffer each, so a
    /// host can hold all three at once.
    state_machines: CString,
    machine_inputs: CString,
    animations: CString,
    /// What the timeline's audio should be doing right now, rebuilt on every
    /// call because it changes with the playhead.
    active_audio: CString,
    audio_assets: CString,
}

static SURFACE_LAST_ERROR: OnceLock<Mutex<CString>> = OnceLock::new();
static NULL_PLAYER_ERROR: OnceLock<CString> = OnceLock::new();

impl KinetoPlayerHandle {
    fn new(bytes: Vec<u8>, mut player: KinetoPlayer) -> Result<Self, format::InvalidFlatbuffer> {
        let last_frame = Some(player.play()?);
        Ok(Self {
            references: AtomicUsize::new(1),
            player,
            bytes,
            last_frame,
            last_error: make_empty_c_string(),
            fired_audio: make_empty_c_string(),
            audio_events: make_empty_c_string(),
            state_machines: make_empty_c_string(),
            machine_inputs: make_empty_c_string(),
            animations: make_empty_c_string(),
            active_audio: make_empty_c_string(),
            audio_assets: make_empty_c_string(),
        })
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.last_error = make_c_string(message.into());
    }
}

fn set_surface_error(message: impl Into<String>) {
    let error = SURFACE_LAST_ERROR.get_or_init(|| Mutex::new(make_empty_c_string()));
    if let Ok(mut current) = error.lock() {
        *current = make_c_string(message.into());
    }
}

/// Runs a GPU call so a panic in it becomes an error rather than an abort.
///
/// wgpu and vello panic on validation failures, and a panic cannot cross
/// `extern "C"`: the process dies, taking the host app with it, on a machine whose
/// driver happens to refuse something. A runtime embedded in someone else's app
/// reports that instead. The iOS simulator is such a machine: it lacks the indirect
/// execution vello needs. On Android a hook keeps the panic's location in logcat,
/// since Rust's stderr is discarded there.
#[cfg(any(
    feature = "metal-render",
    all(feature = "android-render", target_os = "android")
))]
fn guard_gpu_call<T>(body: impl FnOnce() -> T) -> Result<T, String> {
    #[cfg(target_os = "android")]
    {
        static HOOK: OnceLock<()> = OnceLock::new();
        HOOK.get_or_init(|| {
            std::panic::set_hook(Box::new(|info| {
                crate::render::android_surface::log_line(&format!("[kineto] panic: {info}"));
            }));
        });
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).map_err(|payload| {
        payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "the renderer panicked".to_owned())
    })
}

/// Load a `.kvk` FlatBuffer into an opaque runtime player.
///
/// Returns null on failure. When `out_status` is non-null, it receives a
/// `KinetoStatus` value.
///
/// # Safety
///
/// `bytes` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn kineto_load(
    bytes: *const u8,
    len: usize,
    out_status: *mut KinetoStatus,
) -> *mut KinetoPlayerHandle {
    if bytes.is_null() || len == 0 {
        set_status(out_status, KinetoStatus::NullPointer);
        return ptr::null_mut();
    }

    let bytes = unsafe { std::slice::from_raw_parts(bytes, len) }.to_vec();
    match KinetoPlayer::load(bytes.clone())
        .and_then(|player| KinetoPlayerHandle::new(bytes, player))
    {
        Ok(handle) => {
            set_status(out_status, KinetoStatus::Ok);
            Box::into_raw(Box::new(handle))
        }
        Err(_) => {
            set_status(out_status, KinetoStatus::LoadFailed);
            ptr::null_mut()
        }
    }
}

/// Advance the player by `dt` seconds and cache the evaluated frame.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`.
#[no_mangle]
pub unsafe extern "C" fn kineto_advance(player: *mut KinetoPlayerHandle, dt: f64) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };

    match handle.player.advance(dt) {
        Ok(frame) => {
            handle.last_frame = Some(frame);
            handle.set_error("");
            KinetoStatus::Ok
        }
        Err(error) => {
            handle.set_error(error.to_string());
            KinetoStatus::AdvanceFailed
        }
    }
}

/// Render the last evaluated frame to an opaque surface.
///
/// In M7.10 the surface remains opaque for Swift integration. With the
/// `metal-render` feature enabled, `surface` is expected to point to a
/// `MetalRenderSurface`.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. `surface` must be
/// either null or a valid platform surface pointer for the enabled renderer.
#[no_mangle]
pub unsafe extern "C" fn kineto_render(
    player: *mut KinetoPlayerHandle,
    surface: *mut c_void,
) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };
    if surface.is_null() {
        handle.set_error("surface pointer is null");
        return KinetoStatus::NullPointer;
    }
    let Some(frame) = handle.last_frame.as_ref() else {
        handle.set_error("no evaluated frame available");
        return KinetoStatus::InvalidArgument;
    };

    #[cfg(all(feature = "android-render", target_os = "android"))]
    {
        let document = match format::load(&handle.bytes) {
            Ok(document) => document,
            Err(error) => {
                handle.set_error(error.to_string());
                return KinetoStatus::RenderFailed;
            }
        };
        let render_scene = crate::render::build_render_scene(document, frame);
        let surface = unsafe { &mut *(surface as *mut crate::render::AndroidRenderSurface) };
        return match guard_gpu_call(|| surface.render_scene(&render_scene)) {
            Ok(Ok(())) => {
                handle.set_error("");
                KinetoStatus::Ok
            }
            Ok(Err(error)) => {
                handle.set_error(format!("{error:?}"));
                KinetoStatus::RenderFailed
            }
            Err(panic) => {
                handle.set_error(panic);
                KinetoStatus::RenderFailed
            }
        };
    }

    #[cfg(all(
        feature = "metal-render",
        not(all(feature = "android-render", target_os = "android"))
    ))]
    {
        let document = match format::load(&handle.bytes) {
            Ok(document) => document,
            Err(error) => {
                handle.set_error(error.to_string());
                return KinetoStatus::RenderFailed;
            }
        };
        let render_scene = crate::render::build_render_scene(document, frame);
        let surface = unsafe { &mut *(surface as *mut crate::render::MetalRenderSurface<'static>) };
        return match guard_gpu_call(|| surface.render_scene(&render_scene)) {
            Ok(Ok(())) => {
                handle.set_error("");
                KinetoStatus::Ok
            }
            Ok(Err(error)) => {
                handle.set_error(format!("{error:?}"));
                KinetoStatus::RenderFailed
            }
            Err(panic) => {
                handle.set_error(panic);
                KinetoStatus::RenderFailed
            }
        };
    }

    #[cfg(not(any(
        feature = "metal-render",
        all(feature = "android-render", target_os = "android")
    )))]
    {
        let _ = frame;
        handle.set_error("kineto_render requires a platform render feature");
        KinetoStatus::Unsupported
    }
}

/// Create an opaque render surface from a `CAMetalLayer`.
///
/// Returns null on failure. The returned pointer is passed to `kineto_render`
/// and must be released with `kineto_surface_destroy`.
///
/// # Safety
///
/// `layer` must be a valid `CAMetalLayer` pointer that outlives the returned
/// surface. This must be called on the Apple main thread.
#[no_mangle]
pub unsafe extern "C" fn kineto_surface_create_from_ca_metal_layer(
    layer: *mut c_void,
    width: u32,
    height: u32,
    out_status: *mut KinetoStatus,
) -> *mut c_void {
    if layer.is_null() {
        set_surface_error("layer pointer is null");
        set_status(out_status, KinetoStatus::NullPointer);
        return ptr::null_mut();
    }
    if width == 0 || height == 0 {
        set_surface_error("surface dimensions must be non-zero");
        set_status(out_status, KinetoStatus::InvalidArgument);
        return ptr::null_mut();
    }

    #[cfg(all(feature = "metal-render", target_vendor = "apple"))]
    {
        let size = crate::render::SurfaceSize::new(width, height)
            .expect("surface dimensions were validated");
        let created = guard_gpu_call(|| unsafe {
            crate::render::MetalRenderSurface::from_ca_metal_layer(layer, size)
        });
        match created {
            Ok(Ok(surface)) => {
                set_surface_error("");
                set_status(out_status, KinetoStatus::Ok);
                Box::into_raw(Box::new(surface)).cast::<c_void>()
            }
            Ok(Err(error)) => {
                set_surface_error(format!("{error:?}"));
                set_status(out_status, KinetoStatus::RenderFailed);
                ptr::null_mut()
            }
            Err(panic) => {
                set_surface_error(panic);
                set_status(out_status, KinetoStatus::RenderFailed);
                ptr::null_mut()
            }
        }
    }

    #[cfg(not(all(feature = "metal-render", target_vendor = "apple")))]
    {
        set_surface_error("kineto_surface_create requires the metal-render feature on Apple");
        set_status(out_status, KinetoStatus::Unsupported);
        ptr::null_mut()
    }
}

/// Create an opaque render surface from an Android `ANativeWindow`.
///
/// Returns null on failure. The returned pointer is passed to `kineto_render`
/// and must be released with `kineto_surface_destroy`.
///
/// # Safety
///
/// `window` must be a valid `ANativeWindow` pointer. The runtime acquires its
/// own reference, so callers may release their temporary reference after this
/// function returns.
#[no_mangle]
pub unsafe extern "C" fn kineto_surface_create_from_android_native_window(
    window: *mut c_void,
    width: u32,
    height: u32,
    out_status: *mut KinetoStatus,
) -> *mut c_void {
    if window.is_null() {
        set_surface_error("ANativeWindow pointer is null");
        set_status(out_status, KinetoStatus::NullPointer);
        return ptr::null_mut();
    }
    if width == 0 || height == 0 {
        set_surface_error("surface dimensions must be non-zero");
        set_status(out_status, KinetoStatus::InvalidArgument);
        return ptr::null_mut();
    }

    #[cfg(all(feature = "android-render", target_os = "android"))]
    {
        let size = crate::render::SurfaceSize::new(width, height)
            .expect("surface dimensions were validated");
        let created = guard_gpu_call(|| unsafe {
            crate::render::AndroidRenderSurface::from_anative_window(window, size)
        });
        match created {
            Ok(Ok(surface)) => {
                set_surface_error("");
                set_status(out_status, KinetoStatus::Ok);
                Box::into_raw(Box::new(surface)).cast::<c_void>()
            }
            Ok(Err(error)) => {
                set_surface_error(format!("{error:?}"));
                set_status(out_status, KinetoStatus::RenderFailed);
                ptr::null_mut()
            }
            Err(panic) => {
                set_surface_error(panic);
                set_status(out_status, KinetoStatus::RenderFailed);
                ptr::null_mut()
            }
        }
    }

    #[cfg(not(all(feature = "android-render", target_os = "android")))]
    {
        set_surface_error("kineto_surface_create requires android-render on Android");
        set_status(out_status, KinetoStatus::Unsupported);
        ptr::null_mut()
    }
}

/// Return the last render-surface error message, if any.
#[no_mangle]
pub extern "C" fn kineto_surface_last_error_message() -> *const c_char {
    let error = SURFACE_LAST_ERROR.get_or_init(|| Mutex::new(make_empty_c_string()));
    match error.lock() {
        Ok(message) => message.as_ptr(),
        Err(_) => ptr::null(),
    }
}

/// Resize an opaque render surface.
///
/// # Safety
///
/// `surface` must be null or a pointer returned by
/// `kineto_surface_create_from_ca_metal_layer`.
#[no_mangle]
pub unsafe extern "C" fn kineto_surface_resize(
    surface: *mut c_void,
    width: u32,
    height: u32,
) -> KinetoStatus {
    if surface.is_null() {
        return KinetoStatus::NullPointer;
    }
    if width == 0 || height == 0 {
        return KinetoStatus::InvalidArgument;
    }

    #[cfg(all(feature = "android-render", target_os = "android"))]
    {
        let size = crate::render::SurfaceSize::new(width, height)
            .expect("surface dimensions were validated");
        let surface = unsafe { &mut *(surface as *mut crate::render::AndroidRenderSurface) };
        return match guard_gpu_call(|| surface.resize(size)) {
            Ok(()) => KinetoStatus::Ok,
            Err(panic) => {
                set_surface_error(panic);
                KinetoStatus::RenderFailed
            }
        };
    }

    #[cfg(all(
        feature = "metal-render",
        not(all(feature = "android-render", target_os = "android"))
    ))]
    {
        let size = crate::render::SurfaceSize::new(width, height)
            .expect("surface dimensions were validated");
        let surface = unsafe { &mut *(surface as *mut crate::render::MetalRenderSurface<'static>) };
        return match guard_gpu_call(|| surface.resize(size)) {
            Ok(()) => KinetoStatus::Ok,
            Err(panic) => {
                set_surface_error(panic);
                KinetoStatus::RenderFailed
            }
        };
    }

    #[cfg(not(any(
        feature = "metal-render",
        all(feature = "android-render", target_os = "android")
    )))]
    {
        KinetoStatus::Unsupported
    }
}

/// Destroy an opaque render surface.
///
/// # Safety
///
/// `surface` must be null or a pointer returned by
/// `kineto_surface_create_from_ca_metal_layer` that has not already been
/// destroyed.
#[no_mangle]
pub unsafe extern "C" fn kineto_surface_destroy(surface: *mut c_void) {
    if surface.is_null() {
        return;
    }

    #[cfg(all(feature = "android-render", target_os = "android"))]
    {
        let surface = unsafe { Box::from_raw(surface as *mut crate::render::AndroidRenderSurface) };
        // A failed teardown leaks at worst; it must not take the host down.
        let _ = guard_gpu_call(move || drop(surface));
        return;
    }

    #[cfg(all(
        feature = "metal-render",
        not(all(feature = "android-render", target_os = "android"))
    ))]
    {
        let surface =
            unsafe { Box::from_raw(surface as *mut crate::render::MetalRenderSurface<'static>) };
        let _ = guard_gpu_call(move || drop(surface));
    }
}

/// Set a boolean state-machine input.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. `state_machine_id`
/// and `input_name` must be valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn kineto_set_bool(
    player: *mut KinetoPlayerHandle,
    state_machine_id: *const c_char,
    input_name: *const c_char,
    value: bool,
) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };
    let (state_machine_id, input_name) =
        match unsafe { c_string_pair(state_machine_id, input_name) } {
            Ok(values) => values,
            Err(status) => {
                handle.set_error("invalid state machine input identifiers");
                return status;
            }
        };

    match handle
        .player
        .set_bool(&state_machine_id, &input_name, value)
    {
        Ok(()) => {
            handle.set_error("");
            KinetoStatus::Ok
        }
        Err(error) => {
            handle.set_error(error);
            KinetoStatus::InvalidArgument
        }
    }
}

/// Pointer event kinds a host can deliver to the runtime.
///
/// `repr(C)`, like [`KinetoStatus`], so cbindgen emits one tagged enum
/// (`typedef enum KinetoPointerEvent { … } KinetoPointerEvent;`). Under
/// `repr(i32)` it emitted an untagged enum *plus* a same-named
/// `typedef int32_t`, which is legal C and ambiguous in Swift — importing the
/// header then failed with "'KinetoPointerEvent' is ambiguous for type
/// lookup". The values are unchanged: a C enum is a 32-bit int on every
/// Apple platform, so this is not an ABI change.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KinetoPointerEvent {
    Down = 0,
    Up = 1,
    Enter = 2,
    Exit = 3,
}

/// Deliver a pointer event at a document point.
///
/// Whatever listens there drives its input — the host does not need to know
/// which input, or that there is a state machine at all (issue #74).
/// `fired_count`, when non-null, receives the number of listeners that fired.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. `fired_count` must be
/// null or a valid, writable pointer.
#[no_mangle]
pub unsafe extern "C" fn kineto_pointer_event(
    player: *mut KinetoPlayerHandle,
    event: KinetoPointerEvent,
    x: f64,
    y: f64,
    fired_count: *mut u32,
) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };

    let kind = match event {
        KinetoPointerEvent::Down => format::ListenerEventKind::PointerDown,
        KinetoPointerEvent::Up => format::ListenerEventKind::PointerUp,
        KinetoPointerEvent::Enter => format::ListenerEventKind::PointerEnter,
        KinetoPointerEvent::Exit => format::ListenerEventKind::PointerExit,
    };

    match handle.player.pointer_event(kind, x, y) {
        Ok(fired) => {
            if !fired_count.is_null() {
                unsafe { *fired_count = fired.len() as u32 };
            }
            handle.set_error("");
            KinetoStatus::Ok
        }
        Err(error) => {
            handle.set_error(error.to_string());
            KinetoStatus::InvalidArgument
        }
    }
}

/// Deliver a pointer *move* — a drag rather than a tap.
///
/// Fires no listeners: the editor has no Move listener, so a move is
/// continuous input, not a discrete event. It drives whatever reads the
/// pointer continuously — following targets, extent-dragged bindings,
/// pointer-sourced inputs. Without it a following target only updates on tap,
/// so "look at my finger" means "look where I last tapped" (issue #111).
///
/// # Safety
///
/// `player` must be a handle from `kineto_load`.
#[no_mangle]
pub unsafe extern "C" fn kineto_pointer_moved(
    player: *mut KinetoPlayerHandle,
    x: f64,
    y: f64,
) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };
    match handle.player.pointer_moved(x, y) {
        Ok(()) => {
            handle.set_error("");
            KinetoStatus::Ok
        }
        Err(error) => {
            handle.set_error(error.to_string());
            KinetoStatus::InvalidArgument
        }
    }
}

/// Convert a point on the drawable into document space.
///
/// The renderer fits the document into the drawable and draws it through the
/// camera; a host that hit tests has to undo exactly that. Doing the
/// arithmetic host-side is how the two drift — which is what happened when the
/// surface started fitting and the reference app kept multiplying touches by
/// the layer's contents scale, putting every tap where the artwork was not.
///
/// # Safety
///
/// `player` must be a handle from `kineto_load`. `out_x` and `out_y` must be
/// writable, or null to skip that axis.
#[no_mangle]
pub unsafe extern "C" fn kineto_document_point(
    player: *mut KinetoPlayerHandle,
    surface_width: f64,
    surface_height: f64,
    x: f64,
    y: f64,
    out_x: *mut f64,
    out_y: *mut f64,
) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };
    match handle
        .player
        .document_point(surface_width, surface_height, x, y)
    {
        Ok((dx, dy)) => {
            if !out_x.is_null() {
                unsafe { *out_x = dx };
            }
            if !out_y.is_null() {
                unsafe { *out_y = dy };
            }
            handle.set_error("");
            KinetoStatus::Ok
        }
        Err(error) => {
            handle.set_error(error.to_string());
            KinetoStatus::InvalidArgument
        }
    }
}

/// Writes the artboard background the surface clears to as four straight
/// (not premultiplied) RGBA bytes (issue #233). `0, 0, 0, 0` means
/// transparent: no background, or a fully clear one.
///
/// # Safety
///
/// `player` must be a handle from `kineto_load`. `out_rgba` must point to four
/// writable bytes.
#[no_mangle]
pub unsafe extern "C" fn kineto_background_color(
    player: *mut KinetoPlayerHandle,
    out_rgba: *mut u8,
) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };
    if out_rgba.is_null() {
        return KinetoStatus::NullPointer;
    }
    match handle.player.background_color() {
        Ok(rgba) => {
            unsafe { std::ptr::copy_nonoverlapping(rgba.as_ptr(), out_rgba, 4) };
            handle.set_error("");
            KinetoStatus::Ok
        }
        Err(error) => {
            handle.set_error(error.to_string());
            KinetoStatus::InvalidArgument
        }
    }
}

/// Set a number state-machine input to a normalised `0..1` value.
///
/// This is the pointer entry point: a host that tracks a mouse or finger
/// normalises the coordinate against its own viewport and pushes it here, once
/// per axis. Values outside `0..1` are clamped where they are read.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. `state_machine_id`
/// and `input_name` must be valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn kineto_set_number(
    player: *mut KinetoPlayerHandle,
    state_machine_id: *const c_char,
    input_name: *const c_char,
    value: f64,
) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };
    let (state_machine_id, input_name) =
        match unsafe { c_string_pair(state_machine_id, input_name) } {
            Ok(values) => values,
            Err(status) => {
                handle.set_error("invalid state machine input identifiers");
                return status;
            }
        };

    match handle
        .player
        .set_number(&state_machine_id, &input_name, value)
    {
        Ok(()) => {
            handle.set_error("");
            KinetoStatus::Ok
        }
        Err(error) => {
            handle.set_error(error);
            KinetoStatus::InvalidArgument
        }
    }
}

/// Queue a trigger state-machine input.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. `state_machine_id`
/// and `input_name` must be valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn kineto_set_trigger(
    player: *mut KinetoPlayerHandle,
    state_machine_id: *const c_char,
    input_name: *const c_char,
) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };
    let (state_machine_id, input_name) =
        match unsafe { c_string_pair(state_machine_id, input_name) } {
            Ok(values) => values,
            Err(status) => {
                handle.set_error("invalid state machine input identifiers");
                return status;
            }
        };

    match handle.player.set_trigger(&state_machine_id, &input_name) {
        Ok(()) => {
            handle.set_error("");
            KinetoStatus::Ok
        }
        Err(error) => {
            handle.set_error(error);
            KinetoStatus::InvalidArgument
        }
    }
}

/// Copy the active state id for a state machine into `out_buffer`.
///
/// Returns the number of bytes required including the trailing null byte. A
/// return value of 0 means no active state exists or the arguments are invalid.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. `state_machine_id`
/// must be a valid null-terminated UTF-8 string. When `out_buffer` is non-null,
/// it must point to `out_len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn kineto_get_active_state(
    player: *const KinetoPlayerHandle,
    state_machine_id: *const c_char,
    out_buffer: *mut c_char,
    out_len: usize,
) -> usize {
    let Some(handle) = (unsafe { player.as_ref() }) else {
        return 0;
    };
    let Ok(state_machine_id) = (unsafe { c_string_from_ptr(state_machine_id) }) else {
        return 0;
    };
    let Some(active_state) = handle.player.active_state(&state_machine_id) else {
        return 0;
    };
    copy_string_to_buffer(active_state, out_buffer, out_len)
}

/// Copy the active transition id for a state machine into `out_buffer`.
///
/// Returns the number of bytes required including the trailing null byte. A
/// return value of 0 means no active transition exists or the arguments are
/// invalid.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. `state_machine_id`
/// must be a valid null-terminated UTF-8 string. When `out_buffer` is non-null,
/// it must point to `out_len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn kineto_get_active_transition(
    player: *const KinetoPlayerHandle,
    state_machine_id: *const c_char,
    out_buffer: *mut c_char,
    out_len: usize,
) -> usize {
    let Some(handle) = (unsafe { player.as_ref() }) else {
        return 0;
    };
    let Ok(state_machine_id) = (unsafe { c_string_from_ptr(state_machine_id) }) else {
        return 0;
    };
    let Some(active_transition) = handle.player.active_transition(&state_machine_id) else {
        return 0;
    };
    copy_string_to_buffer(active_transition, out_buffer, out_len)
}

/// Return transition progress in `[0, 1]` for a state machine.
///
/// Returns 0 when no transition is active. `out_status` receives
/// `InvalidArgument` for invalid arguments or unknown state machines.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. `state_machine_id`
/// must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn kineto_get_transition_progress(
    player: *const KinetoPlayerHandle,
    state_machine_id: *const c_char,
    out_status: *mut KinetoStatus,
) -> f64 {
    let Some(handle) = (unsafe { player.as_ref() }) else {
        set_status(out_status, KinetoStatus::NullPointer);
        return 0.0;
    };
    let Ok(state_machine_id) = (unsafe { c_string_from_ptr(state_machine_id) }) else {
        set_status(out_status, KinetoStatus::InvalidArgument);
        return 0.0;
    };
    if handle
        .player
        .active_state(&state_machine_id)
        .or_else(|| handle.player.active_transition(&state_machine_id))
        .is_none()
    {
        set_status(out_status, KinetoStatus::InvalidArgument);
        return 0.0;
    }
    let progress = handle
        .player
        .transition_progress(&state_machine_id)
        .unwrap_or(0.0);
    set_status(out_status, KinetoStatus::Ok);
    progress
}

/// Hit-test a document-space point and copy the topmost node id.
///
/// Returns the number of bytes required including the trailing null byte. A
/// return value of 0 means no hit or invalid arguments.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. When `out_buffer` is
/// non-null, it must point to `out_len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn kineto_hit_test_document_point(
    player: *const KinetoPlayerHandle,
    x: f64,
    y: f64,
    out_buffer: *mut c_char,
    out_len: usize,
) -> usize {
    let Some(handle) = (unsafe { player.as_ref() }) else {
        return 0;
    };
    match handle.player.hit_test_document_point(x, y) {
        Ok(Some(target)) => copy_string_to_buffer(&target.node_id, out_buffer, out_len),
        Ok(None) | Err(_) => 0,
    }
}

/// Return the last error message owned by `player`.
///
/// The pointer remains valid until the next mutating FFI call for the same
/// player, or until `kineto_destroy`.
///
/// # Safety
///
/// `player` must be null or a pointer returned by `kineto_load`.
#[no_mangle]
pub unsafe extern "C" fn kineto_last_error_message(
    player: *const KinetoPlayerHandle,
) -> *const c_char {
    let Some(handle) = (unsafe { player.as_ref() }) else {
        return NULL_PLAYER_ERROR
            .get_or_init(|| make_c_string("null player"))
            .as_ptr();
    };
    handle.last_error.as_ptr()
}

/// Take the sounds fired since the last call, as newline-joined event ids.
///
/// A `.kvk` carries no audio bytes: the runtime reports *which* sound fired
/// and the host plays whatever it ships for that event (issue #111). Draining,
/// so a host that polls every frame is never handed the same firing twice, and
/// never de-duplicated — the same sound twice is two firings.
///
/// Returns an empty string when nothing fired. `count`, when non-null,
/// receives the number of firings. The pointer remains valid until the next
/// call to this function for the same player, or until `kineto_destroy`.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. `count` must be null
/// or a valid, writable pointer.
#[no_mangle]
pub unsafe extern "C" fn kineto_take_fired_audio_events(
    player: *mut KinetoPlayerHandle,
    count: *mut u32,
) -> *const c_char {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return NULL_PLAYER_ERROR
            .get_or_init(|| make_c_string("null player"))
            .as_ptr();
    };

    let fired = handle.player.take_fired_audio_events();
    if !count.is_null() {
        unsafe { *count = fired.len() as u32 };
    }
    handle.fired_audio = make_c_string(fired.join("\n"));
    handle.fired_audio.as_ptr()
}

/// The document's sounds, as newline-joined `id\tname\tsource_name` records.
///
/// How a host maps a reported firing onto its own audio. `source_name` is the
/// file the sound was authored against and may be empty.
///
/// The pointer remains valid until the next call to this function for the same
/// player, or until `kineto_destroy`.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`.
#[no_mangle]
pub unsafe extern "C" fn kineto_audio_events(
    player: *mut KinetoPlayerHandle,
) -> *const c_char {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return NULL_PLAYER_ERROR
            .get_or_init(|| make_c_string("null player"))
            .as_ptr();
    };

    let records = match handle.player.audio_events() {
        Ok(events) => events
            .into_iter()
            .map(|(id, name, source)| {
                format!("{id}\t{name}\t{}", source.unwrap_or_default())
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Err(error) => {
            handle.set_error(error.to_string());
            String::new()
        }
    };
    handle.audio_events = make_c_string(records);
    handle.audio_events.as_ptr()
}

/// The document's state machines, as newline-joined `id\tname` records.
///
/// Introspection for a host that shows a document rather than driving a known
/// one (issue #111). The pointer remains valid until the next call to this
/// function for the same player, or until `kineto_destroy`.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`.
#[no_mangle]
pub unsafe extern "C" fn kineto_state_machines(
    player: *mut KinetoPlayerHandle,
) -> *const c_char {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return NULL_PLAYER_ERROR
            .get_or_init(|| make_c_string("null player"))
            .as_ptr();
    };

    let records = match handle.player.state_machines() {
        Ok(machines) => machines
            .into_iter()
            .map(|(id, name)| format!("{id}\t{name}"))
            .collect::<Vec<_>>()
            .join("\n"),
        Err(error) => {
            handle.set_error(error.to_string());
            String::new()
        }
    };
    handle.state_machines = make_c_string(records);
    handle.state_machines.as_ptr()
}

/// One machine's inputs, as newline-joined
/// `id\tname\tkind\tdefault_bool\tdefault_number` records.
///
/// `kind` is `0` bool, `1` trigger, `2` number; `default_bool` is `0` or `1`.
/// Empty when nothing carries `machine_id`.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. `machine_id` must be
/// a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn kineto_machine_inputs(
    player: *mut KinetoPlayerHandle,
    machine_id: *const c_char,
) -> *const c_char {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return NULL_PLAYER_ERROR
            .get_or_init(|| make_c_string("null player"))
            .as_ptr();
    };
    let Ok(machine_id) = (unsafe { c_string_from_ptr(machine_id) }) else {
        handle.set_error("machine_id is not valid UTF-8");
        handle.machine_inputs = make_empty_c_string();
        return handle.machine_inputs.as_ptr();
    };

    let records = match handle.player.machine_inputs(&machine_id) {
        Ok(inputs) => inputs
            .into_iter()
            .map(|(id, name, kind, default_bool, default_number)| {
                format!(
                    "{id}\t{name}\t{kind}\t{}\t{default_number}",
                    u8::from(default_bool)
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Err(error) => {
            handle.set_error(error.to_string());
            String::new()
        }
    };
    handle.machine_inputs = make_c_string(records);
    handle.machine_inputs.as_ptr()
}

/// The document's animation clips, as newline-joined `id\tname\tduration`
/// records, the duration in seconds.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`.
#[no_mangle]
pub unsafe extern "C" fn kineto_animations(
    player: *mut KinetoPlayerHandle,
) -> *const c_char {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return NULL_PLAYER_ERROR
            .get_or_init(|| make_c_string("null player"))
            .as_ptr();
    };

    let records = match handle.player.animations() {
        Ok(clips) => clips
            .into_iter()
            .map(|(id, name, duration)| format!("{id}\t{name}\t{duration}"))
            .collect::<Vec<_>>()
            .join("\n"),
        Err(error) => {
            handle.set_error(error.to_string());
            String::new()
        }
    };
    handle.animations = make_c_string(records);
    handle.animations.as_ptr()
}

/// The timeline audio that should be sounding right now, as newline-joined
/// `track_id\tasset_id\tfile_name\tsource_position\tvolume` records
/// (issue #111).
///
/// Poll it after each `kineto_advance`: a host starts a player for a track
/// that appears, stops one that disappears, and seeks when the position drifts
/// from where its own playback has got to. Empty means silence — which is also
/// the answer while paused at a point no clip covers.
///
/// The pointer remains valid until the next call to this function for the same
/// player, or until `kineto_destroy`.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`.
#[no_mangle]
pub unsafe extern "C" fn kineto_active_audio_clips(
    player: *mut KinetoPlayerHandle,
) -> *const c_char {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return NULL_PLAYER_ERROR
            .get_or_init(|| make_c_string("null player"))
            .as_ptr();
    };

    let records = match handle.player.active_audio_clips() {
        Ok(clips) => clips
            .into_iter()
            .map(|(track_id, asset_id, name, position, volume)| {
                format!("{track_id}\t{asset_id}\t{name}\t{position}\t{volume}")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Err(error) => {
            handle.set_error(error.to_string());
            String::new()
        }
    };
    handle.active_audio = make_c_string(records);
    handle.active_audio.as_ptr()
}

/// Start playback of the active animation clip (issue #111).
///
/// Without this the timeline never advances, so a clip's audio has no playhead
/// to follow — `kineto_advance` only moves time while the player is playing.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`.
#[no_mangle]
pub unsafe extern "C" fn kineto_play(player: *mut KinetoPlayerHandle) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };
    match handle.player.play() {
        Ok(_) => {
            handle.set_error("");
            KinetoStatus::Ok
        }
        Err(error) => {
            handle.set_error(error.to_string());
            KinetoStatus::InvalidArgument
        }
    }
}

/// Pause playback, leaving the playhead where it is.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`.
#[no_mangle]
pub unsafe extern "C" fn kineto_pause(player: *mut KinetoPlayerHandle) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };
    handle.player.pause();
    handle.set_error("");
    KinetoStatus::Ok
}

/// Make `animation_id` the active clip, rewinding to its start.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. `animation_id` must
/// be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn kineto_select_animation(
    player: *mut KinetoPlayerHandle,
    animation_id: *const c_char,
) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };
    let animation_id = match unsafe { c_string_from_ptr(animation_id) } {
        Ok(id) => id,
        Err(status) => {
            handle.set_error("animation_id is not valid UTF-8");
            return status;
        }
    };
    // The player ignores an id it does not know. A host passing one has a bug
    // the status has to carry: reporting Ok here leaves it playing the old clip.
    let known = handle
        .player
        .animations()
        .map(|clips| clips.iter().any(|(id, _, _)| *id == animation_id))
        .unwrap_or(false);
    if !known {
        handle.set_error(format!("Unknown animation id \"{animation_id}\""));
        return KinetoStatus::InvalidArgument;
    }
    match handle.player.select_animation(&animation_id) {
        Ok(()) => {
            handle.set_error("");
            KinetoStatus::Ok
        }
        Err(error) => {
            handle.set_error(error.to_string());
            KinetoStatus::InvalidArgument
        }
    }
}

/// Every audio file the document refers to, as newline-joined
/// `asset_id\tfile_name\tduration` records (issue #111).
///
/// A `.kvk` names its audio and carries none of it, so this is how a host
/// knows what to ask the user for — before anything is bound and before
/// anything plays.
///
/// The pointer remains valid until the next call to this function for the same
/// player, or until `kineto_destroy`.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`.
#[no_mangle]
pub unsafe extern "C" fn kineto_audio_assets(
    player: *mut KinetoPlayerHandle,
) -> *const c_char {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return NULL_PLAYER_ERROR
            .get_or_init(|| make_c_string("null player"))
            .as_ptr();
    };

    let records = match handle.player.audio_assets() {
        Ok(assets) => assets
            .into_iter()
            .map(|(id, name, duration)| format!("{id}\t{name}\t{duration}"))
            .collect::<Vec<_>>()
            .join("\n"),
        Err(error) => {
            handle.set_error(error.to_string());
            String::new()
        }
    };
    handle.audio_assets = make_c_string(records);
    handle.audio_assets.as_ptr()
}

/// One evaluated scalar of the current frame, e.g. `target-hand` /
/// `ikTargetPositionX`.
///
/// Exists for cross-runtime parity checks: hosts normally consume a frame by
/// rendering it, but a fixture has to be able to state where the rig ended up.
/// Returns `InvalidArgument` when the frame does not carry that scalar.
///
/// # Safety
///
/// `player` must be a pointer returned by `kineto_load`. `node_id` and `key`
/// must be valid NUL-terminated C strings. `out_value` must be writable.
#[no_mangle]
pub unsafe extern "C" fn kineto_override_scalar(
    player: *mut KinetoPlayerHandle,
    node_id: *const c_char,
    key: *const c_char,
    out_value: *mut f64,
) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_mut() }) else {
        return KinetoStatus::NullPointer;
    };
    if out_value.is_null() {
        return KinetoStatus::NullPointer;
    }
    let (node_id, key) = match (unsafe { c_string_from_ptr(node_id) }, unsafe {
        c_string_from_ptr(key)
    }) {
        (Ok(node_id), Ok(key)) => (node_id, key),
        (Err(status), _) | (_, Err(status)) => return status,
    };
    let value = handle
        .player
        .current_frame()
        .ok()
        .and_then(|frame| frame.node_overrides.get(node_id.as_str())?.scalar(&key));
    match value {
        Some(value) => {
            unsafe { *out_value = value };
            KinetoStatus::Ok
        }
        None => KinetoStatus::InvalidArgument,
    }
}

/// Add an owner to a player. Each call needs its own `kineto_destroy`.
///
/// For a host whose player and view have separate lifetimes, as with a platform
/// view: the view is torn down some time after the framework is told, so the
/// player's owner can release it while the view is still drawing.
///
/// Sharing a handle is not sharing it across threads. Calls on one handle must
/// still not overlap.
///
/// # Safety
///
/// `player` must be null or a live pointer returned by `kineto_load`.
#[no_mangle]
pub unsafe extern "C" fn kineto_retain(player: *mut KinetoPlayerHandle) -> KinetoStatus {
    let Some(handle) = (unsafe { player.as_ref() }) else {
        return KinetoStatus::NullPointer;
    };
    handle.references.fetch_add(1, Ordering::Relaxed);
    KinetoStatus::Ok
}

/// Give up one owner of a player, freeing it when that was the last.
///
/// # Safety
///
/// `player` must be null or a pointer returned by `kineto_load`, and the caller
/// must own a reference to it that it has not already given up.
#[no_mangle]
pub unsafe extern "C" fn kineto_destroy(player: *mut KinetoPlayerHandle) {
    let Some(handle) = (unsafe { player.as_ref() }) else {
        return;
    };
    if handle.references.fetch_sub(1, Ordering::AcqRel) == 1 {
        let _ = unsafe { Box::from_raw(player) };
    }
}

unsafe fn c_string_from_ptr(ptr: *const c_char) -> Result<String, KinetoStatus> {
    if ptr.is_null() {
        return Err(KinetoStatus::NullPointer);
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(str::to_owned)
        .map_err(|_| KinetoStatus::InvalidArgument)
}

unsafe fn c_string_pair(
    first: *const c_char,
    second: *const c_char,
) -> Result<(String, String), KinetoStatus> {
    Ok((unsafe { c_string_from_ptr(first) }?, unsafe {
        c_string_from_ptr(second)
    }?))
}

fn copy_string_to_buffer(value: &str, out_buffer: *mut c_char, out_len: usize) -> usize {
    let required = value.len() + 1;
    if out_buffer.is_null() || out_len == 0 {
        return required;
    }

    let bytes = value.as_bytes();
    let copy_len = bytes.len().min(out_len.saturating_sub(1));
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out_buffer.cast::<u8>(), copy_len);
        *out_buffer.add(copy_len) = 0;
    }
    required
}

fn set_status(out_status: *mut KinetoStatus, status: KinetoStatus) {
    if !out_status.is_null() {
        unsafe { *out_status = status };
    }
}

fn make_empty_c_string() -> CString {
    make_c_string("")
}

fn make_c_string(message: impl Into<String>) -> CString {
    let message = message.into().replace('\0', " ");
    CString::new(message).expect("interior null bytes replaced")
}
