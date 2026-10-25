//! Immediate-style declarations over a retained, incremental UI kernel.
pub use glassmere_core::*;
/// Derives `Properties`: property grid rows from a struct's fields, and
/// `tr!` and `style_sheet!`, which check a message key and a stylesheet
/// while the crate compiles.
#[cfg(feature = "derive")]
pub use glassmere_derive::{Properties, style_sheet, tr};
pub use glassmere_devtools as devtools;
#[cfg(feature = "fluent")]
pub use glassmere_fluent as fluent;
#[cfg(feature = "icu")]
pub use glassmere_icu as icu;
#[cfg(feature = "images")]
pub use glassmere_image as image;
/// Renders a scene to pixels without a graphics adapter.
pub use glassmere_render_cpu::{CpuRenderer, CpuStats};
/// The GPU renderer, for applications that draw with wgpu themselves through
/// `AppOptions::gpu`. See `Renderer::set_custom_draw`.
#[cfg(feature = "desktop")]
pub use glassmere_render_wgpu as render;
pub use glassmere_scene::{
    Affine, Affinity, Brush, Cap, Color, Dash, Fill, FillRule, Image, Join, Path, Point, Primitive,
    Rect, Scene, Segment, Size, Stroke, VectorImage,
};
#[cfg(feature = "syntax")]
pub use glassmere_syntax as syntax;
#[cfg(feature = "desktop")]
pub use glassmere_winit::hot;
#[cfg(feature = "desktop")]
pub use glassmere_winit::{
    AppOptions, DirectionChoice, GpuMemoryPolicy, GpuSetup, GraphicsBackend, GraphicsOptions,
    Placement, ThemeChoice, run, run_with,
};
