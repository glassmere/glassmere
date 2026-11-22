//! Immediate-style declarations over a retained, incremental UI kernel.
pub use glassmere_core::*;
/// Derives `Properties`: property grid rows from a struct's fields, and
/// `PropertyChoice`: an enum as a choice among its variants. Also `tr!` and
/// `style_sheet!`, which check a message key and a stylesheet while the
/// crate compiles.
#[cfg(feature = "derive")]
pub use glassmere_derive::{Properties, PropertyChoice, style_sheet, tr};
pub use glassmere_devtools as devtools;
#[cfg(feature = "embedded")]
pub use glassmere_embedded as embedded;
#[cfg(feature = "fluent")]
pub use glassmere_fluent as fluent;
#[cfg(feature = "icu")]
pub use glassmere_icu as icu;
#[cfg(feature = "images")]
pub use glassmere_image as image;
#[cfg(feature = "markup")]
pub use glassmere_markup as markup;
/// Renders a scene to pixels without a graphics adapter.
pub use glassmere_render_cpu::{CpuRenderer, CpuStats};
/// The GPU renderer, for applications that draw with wgpu themselves through
/// `AppOptions::gpu`. See `Renderer::set_custom_draw`.
#[cfg(feature = "desktop")]
pub use glassmere_render_wgpu as render;
pub use glassmere_scene::{
    Affine, Affinity, Color, Image, ImageFrame, Path, Point, Primitive, Rect, Scene, Segment, Size,
    Stroke, VectorImage,
};
#[cfg(feature = "syntax")]
pub use glassmere_syntax as syntax;
#[cfg(feature = "desktop")]
pub use glassmere_winit::hot;
#[cfg(feature = "desktop")]
pub use glassmere_winit::{
    AppOptions, DirectionChoice, GpuMemoryPolicy, GpuSetup, GraphicsBackend, GraphicsOptions,
    Placement, ThemeChoice, follow_system_reduced_motion, run, run_with,
};
