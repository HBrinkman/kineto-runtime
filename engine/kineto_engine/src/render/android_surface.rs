use std::ffi::{c_char, c_void, CString};
use std::ptr::NonNull;

use raw_window_handle::{
    AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use vello::util::RenderContext;

use super::{RenderScene, SurfaceSize, WgpuRenderSurface, WgpuSurfaceError};

#[link(name = "log")]
extern "C" {
    fn __android_log_write(priority: i32, tag: *const c_char, text: *const c_char) -> i32;
}

/// Writes to logcat. Rust's stderr goes nowhere on Android, so anything meant for
/// a QA session has to come through here.
pub(crate) fn log_line(line: &str) {
    const ANDROID_LOG_ERROR: i32 = 6;
    let text = CString::new(line.replace('\0', " ")).unwrap_or_default();
    unsafe { __android_log_write(ANDROID_LOG_ERROR, c"kineto".as_ptr(), text.as_ptr()) };
}

#[derive(Debug)]
pub enum AndroidSurfaceError {
    InvalidWindow,
    CreateSurface(vello::wgpu::CreateSurfaceError),
    Wgpu(WgpuSurfaceError),
}

impl From<vello::wgpu::CreateSurfaceError> for AndroidSurfaceError {
    fn from(error: vello::wgpu::CreateSurfaceError) -> Self {
        Self::CreateSurface(error)
    }
}

impl From<WgpuSurfaceError> for AndroidSurfaceError {
    fn from(error: WgpuSurfaceError) -> Self {
        Self::Wgpu(error)
    }
}

pub struct AndroidRenderSurface {
    surface: WgpuRenderSurface<'static>,
    window: NonNull<ndk_sys::ANativeWindow>,
}

impl AndroidRenderSurface {
    /// Creates a renderer from an Android `ANativeWindow` pointer.
    ///
    /// # Safety
    ///
    /// `window` must point to a live `ANativeWindow`. The returned surface
    /// acquires its own reference and releases it on drop.
    pub unsafe fn from_anative_window(
        window: *mut c_void,
        size: SurfaceSize,
    ) -> Result<Self, AndroidSurfaceError> {
        let window = NonNull::new(window.cast::<ndk_sys::ANativeWindow>())
            .ok_or(AndroidSurfaceError::InvalidWindow)?;
        unsafe { ndk_sys::ANativeWindow_acquire(window.as_ptr()) };

        let context = RenderContext {
            instance: vello::wgpu::Instance::new(vello::wgpu::InstanceDescriptor {
                display: None,
                backends: vello::wgpu::Backends::VULKAN | vello::wgpu::Backends::GL,
                flags: vello::wgpu::InstanceFlags::from_build_config().with_env(),
                memory_budget_thresholds: vello::wgpu::MemoryBudgetThresholds::default(),
                backend_options: vello::wgpu::BackendOptions::from_env_or_default(),
            }),
            devices: Vec::new(),
        };
        let target = vello::wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(RawDisplayHandle::Android(AndroidDisplayHandle::new())),
            raw_window_handle: RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(
                window.cast::<c_void>(),
            )),
        };
        let wgpu_surface = unsafe { context.instance.create_surface_unsafe(target) }?;
        let surface = match pollster::block_on(WgpuRenderSurface::from_context_and_wgpu_surface(
            context,
            wgpu_surface,
            size,
        )) {
            Ok(surface) => surface,
            Err(error) => {
                unsafe { ndk_sys::ANativeWindow_release(window.as_ptr()) };
                return Err(error.into());
            }
        };

        Ok(Self { surface, window })
    }

    pub fn resize(&mut self, size: SurfaceSize) {
        self.surface.resize(size);
    }

    pub fn render_scene(&mut self, render_scene: &RenderScene) -> Result<(), WgpuSurfaceError> {
        self.surface.render_scene(render_scene)
    }
}

impl Drop for AndroidRenderSurface {
    fn drop(&mut self) {
        unsafe { ndk_sys::ANativeWindow_release(self.window.as_ptr()) };
    }
}
