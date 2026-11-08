#[cfg(all(feature = "android-render", target_os = "android"))]
pub mod android_surface;
#[cfg(feature = "metal-render")]
pub mod metal_surface;
pub mod vello_scene;
#[cfg(any(feature = "metal-render", feature = "android-render"))]
pub mod wgpu_surface;

#[cfg(all(feature = "android-render", target_os = "android"))]
pub use android_surface::{AndroidRenderSurface, AndroidSurfaceError};
#[cfg(feature = "metal-render")]
pub use metal_surface::{MetalRenderSurface, MetalSurfaceError};
pub use vello_scene::{
    build_render_scene, camera_affine, component_support, document_point, fit_transform,
    path_segments,
    ComponentSupport,
    ContourSegments,
    CubicSegment, RenderCap, RenderClip, RenderColor, RenderCommand, RenderGradientStop,
    RenderJoin, RenderPaint, RenderPathContour,
    RenderPathPoint, RenderScene, RenderShape, RenderSpread, RenderStroke, RenderStrokeAlign,
    RenderTransform, trim_windows, TrimWindow,
};

#[cfg(feature = "vello-render")]
pub use vello_scene::{build_vello_scene, build_vello_scene_fitted, trim_outline};
#[cfg(any(feature = "metal-render", feature = "android-render"))]
pub use wgpu_surface::{SurfaceSize, WgpuRenderSurface, WgpuSurfaceError};
