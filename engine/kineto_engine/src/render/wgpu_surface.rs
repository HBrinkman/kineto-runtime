use std::num::NonZeroUsize;

use vello::peniko::color::palette;
use vello::util::RenderContext;
use vello::wgpu::util::TextureBlitter;
use vello::wgpu::{SurfaceConfiguration, Texture, TextureFormat, TextureUsages, TextureView};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions};

use super::{build_vello_scene_fitted, RenderScene};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

impl SurfaceSize {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        Some(Self { width, height })
    }
}

#[derive(Debug)]
pub enum WgpuSurfaceError {
    InvalidSize,
    FrameUnavailable(&'static str),
    /// The GPU is reachable but cannot run vello. Reported before any vello
    /// resource exists, so the host gets an error instead of an abort.
    Unsupported(String),
    Vello(vello::Error),
}

/// Vello dispatches and draws indirectly: its first frame creates buffers with
/// `BufferUsages::INDIRECT`. Without `INDIRECT_EXECUTION` wgpu reports a
/// validation error at that point, and the default error handler turns it into
/// a panic inside an `extern "C"` frame — an abort that takes the host app with
/// it (issue #237, the iOS Simulator's Metal device).
///
/// Checked up front, against what the adapter reports, rather than caught
/// afterwards: a panic across the FFI boundary cannot be recovered, and
/// catching it would also hide genuine bugs.
fn require_vello_downlevel_flags(
    flags: vello::wgpu::DownlevelFlags,
) -> Result<(), WgpuSurfaceError> {
    use vello::wgpu::DownlevelFlags;
    let required = DownlevelFlags::COMPUTE_SHADERS | DownlevelFlags::INDIRECT_EXECUTION;
    let missing = required - flags;
    if missing.is_empty() {
        return Ok(());
    }
    Err(WgpuSurfaceError::Unsupported(format!(
        "this GPU lacks downlevel flags vello requires: {missing:?}"
    )))
}

/// Writes diagnostics where a device QA session can read them: stderr, which
/// `xcrun devicectl` and Console.app both surface, and logcat on Android.
fn diagnostic(line: &str) {
    #[cfg(target_os = "android")]
    super::android_surface::log_line(line);
    #[cfg(not(target_os = "android"))]
    eprintln!("{line}");
}

async fn log_adapter_state(headline: &str, instance: &vello::wgpu::Instance) {
    diagnostic(&format!(
        "[kineto] surface: {headline}. {}",
        describe_adapters(instance).await
    ));
}

impl From<vello::Error> for WgpuSurfaceError {
    fn from(error: vello::Error) -> Self {
        Self::Vello(error)
    }
}

/// A surface Kineto owns end to end: its own device and queue, its own target
/// texture, its own blitter.
///
/// Deliberately *not* `vello::util::RenderSurface` plus `RenderContext`
/// (issue #111). Vello's `RenderContext::device()` requests a device with
/// `wgpu::Limits::default()`, which an iPad cannot satisfy — on an M1 iPad
/// Pro it fails with:
///
/// ```text
/// Limit 'max_inter_stage_shader_variables' value 16 is better than allowed 15
/// ```
///
/// and every failure comes back as a bare `None`, which vello reports as
/// `NoCompatibleDevice` — indistinguishable from having no GPU at all. The
/// device is fine; the ask was too high. So the device is requested here,
/// against the adapter's own limits, and the rest of the surface state comes
/// with it.
pub struct WgpuRenderSurface<'window> {
    device: vello::wgpu::Device,
    queue: vello::wgpu::Queue,
    surface: vello::wgpu::Surface<'window>,
    config: SurfaceConfiguration,
    target_texture: Texture,
    target_view: TextureView,
    blitter: TextureBlitter,
    renderer: Renderer,
}

/// An instance over whatever backends this build carries.
fn default_instance() -> vello::wgpu::Instance {
    vello::wgpu::Instance::new(vello::wgpu::InstanceDescriptor {
        display: None,
        backends: vello::wgpu::Backends::from_env().unwrap_or_default(),
        flags: vello::wgpu::InstanceFlags::from_build_config().with_env(),
        memory_budget_thresholds: vello::wgpu::MemoryBudgetThresholds::default(),
        backend_options: vello::wgpu::BackendOptions::from_env_or_default(),
    })
}

/// Requests a device the way vello would, but asking only for what this
/// adapter actually offers.
///
/// The features mirror vello's own optional set; the limits are the adapter's,
/// which is the whole fix. An adapter never reports limits it cannot meet, so
/// this cannot ask for too much.
async fn request_device(
    adapter: &vello::wgpu::Adapter,
) -> Result<(vello::wgpu::Device, vello::wgpu::Queue), vello::Error> {
    let optional = vello::wgpu::Features::CLEAR_TEXTURE | vello::wgpu::Features::PIPELINE_CACHE;
    adapter
        .request_device(&vello::wgpu::DeviceDescriptor {
            label: Some("kineto_engine.device"),
            required_features: adapter.features() & optional,
            required_limits: adapter.limits(),
            ..Default::default()
        })
        .await
        .map_err(|error| {
            diagnostic(&format!("[kineto] surface: request_device failed: {error}"));
            vello::Error::NoCompatibleDevice
        })
}

/// Picks an adapter for `surface`, falling back to any adapter at all.
async fn request_adapter(
    instance: &vello::wgpu::Instance,
    surface: Option<&vello::wgpu::Surface<'_>>,
) -> Result<vello::wgpu::Adapter, vello::Error> {
    if let Ok(adapter) =
        vello::wgpu::util::initialize_adapter_from_env_or_default(instance, surface).await
    {
        return Ok(adapter);
    }
    // A surface that matches nothing is worth saying out loud: it means the
    // layer, not the GPU, is the problem.
    if surface.is_some() {
        diagnostic("[kineto] surface: no adapter matched the surface; trying without it");
        if let Ok(adapter) =
            vello::wgpu::util::initialize_adapter_from_env_or_default(instance, None).await
        {
            return Ok(adapter);
        }
    }
    Err(vello::Error::NoCompatibleDevice)
}

/// The first compositing mode that keeps the surface's alpha (ADR-0011).
///
/// `Auto` resolves to `Opaque` where offered, and on Metal that overrides the
/// host's `isOpaque = false`: a translucent background clear would show as a
/// darkened opaque colour and a transparent one as black. Metal's non-opaque
/// mode is named `PostMultiplied`, but it only clears the layer's opaque flag;
/// the pixels stay as vello wrote them (premultiplied), which is what Core
/// Animation composites.
fn preferred_alpha_mode(
    available: &[vello::wgpu::CompositeAlphaMode],
) -> vello::wgpu::CompositeAlphaMode {
    use vello::wgpu::CompositeAlphaMode::{Auto, Inherit, PostMultiplied, PreMultiplied};
    [PreMultiplied, PostMultiplied, Inherit]
        .into_iter()
        .find(|mode| available.contains(mode))
        .unwrap_or(Auto)
}

impl<'window> WgpuRenderSurface<'window> {
    pub async fn new(
        target: impl Into<vello::wgpu::SurfaceTarget<'window>>,
        size: SurfaceSize,
    ) -> Result<Self, WgpuSurfaceError> {
        let instance = default_instance();
        let surface = instance
            .create_surface(target)
            .map_err(vello::Error::from)?;
        Self::from_instance_and_surface(&instance, surface, size).await
    }

    pub async fn from_wgpu_surface(
        surface: vello::wgpu::Surface<'window>,
        size: SurfaceSize,
    ) -> Result<Self, WgpuSurfaceError> {
        let instance = default_instance();
        Self::from_instance_and_surface(&instance, surface, size).await
    }

    pub(crate) async fn from_context_and_wgpu_surface(
        context: RenderContext,
        surface: vello::wgpu::Surface<'window>,
        size: SurfaceSize,
    ) -> Result<Self, WgpuSurfaceError> {
        // The context is only here for its instance: the surface was created
        // from it, so the adapter has to come from the same one.
        Self::from_instance_and_surface(&context.instance, surface, size).await
    }

    pub(crate) async fn from_instance_and_surface(
        instance: &vello::wgpu::Instance,
        surface: vello::wgpu::Surface<'window>,
        size: SurfaceSize,
    ) -> Result<Self, WgpuSurfaceError> {
        let adapter = match request_adapter(instance, Some(&surface)).await {
            Ok(adapter) => adapter,
            Err(error) => {
                log_adapter_state("no adapter", instance).await;
                return Err(error.into());
            }
        };

        let info = adapter.get_info();
        let downlevel = adapter.get_downlevel_capabilities();
        if let Err(error) = require_vello_downlevel_flags(downlevel.flags) {
            diagnostic(&format!(
                "[kineto] surface: {:?}/{} cannot run vello (downlevel flags {:?})",
                info.backend, info.name, downlevel.flags
            ));
            return Err(error);
        }
        let (device, queue) = match request_device(&adapter).await {
            Ok(pair) => pair,
            Err(error) => {
                diagnostic(&format!(
                    "[kineto] surface: {:?}/{} rejected the device request",
                    info.backend, info.name
                ));
                return Err(error.into());
            }
        };

        let capabilities = surface.get_capabilities(&adapter);
        let alpha_mode = preferred_alpha_mode(&capabilities.alpha_modes);
        let format = capabilities
            .formats
            .into_iter()
            .find(|it| matches!(it, TextureFormat::Rgba8Unorm | TextureFormat::Bgra8Unorm))
            .ok_or(vello::Error::UnsupportedSurfaceFormat)?;
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: vello::wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let (target_texture, target_view) = create_targets(size.width, size.height, &device);
        let blitter = TextureBlitter::new(&device, format);
        let renderer = Renderer::new(
            &device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: AaSupport::area_only(),
                num_init_threads: NonZeroUsize::new(1),
                pipeline_cache: None,
            },
        )?;

        Ok(Self {
            device,
            queue,
            surface,
            config,
            target_texture,
            target_view,
            blitter,
            renderer,
        })
    }

    #[cfg(target_vendor = "apple")]
    /// Creates a renderer from a `CAMetalLayer` pointer.
    ///
    /// # Safety
    ///
    /// `layer` must be a valid `CAMetalLayer`, must outlive the returned
    /// surface, and this must be called on the Apple main thread.
    pub unsafe fn from_ca_metal_layer(
        layer: *mut std::ffi::c_void,
        size: SurfaceSize,
    ) -> Result<WgpuRenderSurface<'static>, WgpuSurfaceError> {
        let instance = vello::wgpu::Instance::new(vello::wgpu::InstanceDescriptor {
            display: None,
            backends: vello::wgpu::Backends::METAL,
            flags: vello::wgpu::InstanceFlags::from_build_config().with_env(),
            memory_budget_thresholds: vello::wgpu::MemoryBudgetThresholds::default(),
            backend_options: vello::wgpu::BackendOptions::from_env_or_default(),
        });
        let surface = unsafe {
            instance
                .create_surface_unsafe(vello::wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer))
        }
        .map_err(vello::Error::from)?;
        pollster::block_on(WgpuRenderSurface::from_instance_and_surface(
            &instance, surface, size,
        ))
    }

    pub fn size(&self) -> SurfaceSize {
        SurfaceSize {
            width: self.config.width,
            height: self.config.height,
        }
    }

    pub fn resize(&mut self, size: SurfaceSize) {
        self.config.width = size.width;
        self.config.height = size.height;
        let (texture, view) = create_targets(size.width, size.height, &self.device);
        self.target_texture = texture;
        self.target_view = view;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render_scene(&mut self, render_scene: &RenderScene) -> Result<(), WgpuSurfaceError> {
        let size = self.size();
        if SurfaceSize::new(size.width, size.height).is_none() {
            return Err(WgpuSurfaceError::InvalidSize);
        }

        // Fit the document to the drawable instead of drawing it 1:1 in the
        // corner (issue #111 parity audit §5.10).
        let scene = build_vello_scene_fitted(
            render_scene,
            Some((size.width as f64, size.height as f64)),
        );
        self.renderer.render_to_texture(
            &self.device,
            &self.queue,
            &scene,
            &self.target_view,
            &RenderParams {
                // Straight-alpha clear (ADR-0011); transparent when the
                // document has no background, as before.
                base_color: match render_scene.background {
                    Some([r, g, b, a]) => vello::peniko::Color::from_rgba8(r, g, b, a),
                    None => palette::css::TRANSPARENT,
                },
                width: size.width,
                height: size.height,
                antialiasing_method: AaConfig::Area,
            },
        )?;

        let frame = match self.surface.get_current_texture() {
            vello::wgpu::CurrentSurfaceTexture::Success(frame)
            | vello::wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            vello::wgpu::CurrentSurfaceTexture::Timeout => {
                return Err(WgpuSurfaceError::FrameUnavailable("surface timeout"));
            }
            vello::wgpu::CurrentSurfaceTexture::Occluded => {
                return Err(WgpuSurfaceError::FrameUnavailable("surface occluded"));
            }
            vello::wgpu::CurrentSurfaceTexture::Outdated => {
                return Err(WgpuSurfaceError::FrameUnavailable("surface outdated"));
            }
            vello::wgpu::CurrentSurfaceTexture::Lost => {
                return Err(WgpuSurfaceError::FrameUnavailable("surface lost"));
            }
            vello::wgpu::CurrentSurfaceTexture::Validation => {
                return Err(WgpuSurfaceError::FrameUnavailable("surface validation"));
            }
        };
        let frame_view = frame
            .texture
            .create_view(&vello::wgpu::TextureViewDescriptor::default());
        let mut encoder =
            self.device
                .create_command_encoder(&vello::wgpu::CommandEncoderDescriptor {
                    label: Some("kineto_engine.render_surface.blit"),
                });
        self.blitter
            .copy(&self.device, &mut encoder, &self.target_view, &frame_view);
        self.queue.submit([encoder.finish()]);
        frame.present();
        Ok(())
    }
}

/// What the instance could see when adapter selection failed.
///
/// `NoCompatibleDevice` on its own says nothing a host can act on — it reads
/// the same whether the backend was compiled out, the surface is bogus, or
/// the device genuinely has no GPU. On an iPad the answer matters, so the
/// adapters are enumerated and described (issue #111 device QA).
async fn describe_adapters(instance: &vello::wgpu::Instance) -> String {
    let adapters = instance
        .enumerate_adapters(vello::wgpu::Backends::all())
        .await;
    if adapters.is_empty() {
        return "no adapters at all — the Metal backend is missing from this \
build, or no GPU is reachable from this process"
            .to_owned();
    }
    let described: Vec<String> = adapters
        .iter()
        .map(|adapter| {
            let info = adapter.get_info();
            format!(
                "{:?}/{} ({:?})",
                info.backend, info.name, info.device_type
            )
        })
        .collect();
    format!("{} adapter(s): {}", described.len(), described.join(", "))
}

fn create_targets(width: u32, height: u32, device: &vello::wgpu::Device) -> (Texture, TextureView) {
    let target_texture = device.create_texture(&vello::wgpu::TextureDescriptor {
        label: Some("kineto_engine.render_surface.target"),
        size: vello::wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: vello::wgpu::TextureDimension::D2,
        usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
        format: TextureFormat::Rgba8Unorm,
        view_formats: &[],
    });
    let target_view = target_texture.create_view(&vello::wgpu::TextureViewDescriptor::default());
    (target_texture, target_view)
}

#[cfg(target_vendor = "apple")]
/// Creates a wgpu surface from a retained `CAMetalLayer` pointer.
///
/// # Safety
///
/// `layer` must be a valid `CAMetalLayer` and must outlive the returned
/// `Surface`. The call must run on the Apple main thread, matching wgpu's
/// Metal surface requirements.
pub unsafe fn surface_from_ca_metal_layer(
    context: &RenderContext,
    layer: *mut std::ffi::c_void,
) -> Result<vello::wgpu::Surface<'static>, vello::wgpu::CreateSurfaceError> {
    // Safety: Caller guarantees the pointer is a live CAMetalLayer with a
    // lifetime at least as long as the returned surface.
    unsafe {
        context
            .instance
            .create_surface_unsafe(vello::wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vello::wgpu::{CompositeAlphaMode, DownlevelFlags};

    #[test]
    fn should_keep_alpha_on_metal_where_auto_would_pick_opaque() {
        // Arrange: what wgpu-hal's Metal surface reports.
        let metal = [
            CompositeAlphaMode::Opaque,
            CompositeAlphaMode::PostMultiplied,
        ];

        // Act
        let mode = preferred_alpha_mode(&metal);

        // Assert
        assert_eq!(mode, CompositeAlphaMode::PostMultiplied);
    }

    #[test]
    fn should_prefer_premultiplied_and_fall_back_to_auto() {
        assert_eq!(
            preferred_alpha_mode(&[
                CompositeAlphaMode::Inherit,
                CompositeAlphaMode::PostMultiplied,
                CompositeAlphaMode::PreMultiplied,
            ]),
            CompositeAlphaMode::PreMultiplied,
        );
        assert_eq!(
            preferred_alpha_mode(&[CompositeAlphaMode::Opaque]),
            CompositeAlphaMode::Auto,
        );
    }

    #[test]
    fn should_reject_adapter_without_indirect_execution() {
        // Arrange: the iOS Simulator's Metal device, which vello cannot drive.
        let flags = DownlevelFlags::COMPUTE_SHADERS | DownlevelFlags::FRAGMENT_WRITABLE_STORAGE;

        // Act
        let result = require_vello_downlevel_flags(flags);

        // Assert
        match result {
            Err(WgpuSurfaceError::Unsupported(message)) => {
                assert!(message.contains("INDIRECT_EXECUTION"), "{message}");
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn should_accept_adapter_with_everything_vello_needs() {
        // Arrange
        let flags = DownlevelFlags::all();

        // Act / Assert
        assert!(require_vello_downlevel_flags(flags).is_ok());
    }
}
