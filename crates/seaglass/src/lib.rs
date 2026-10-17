//! Immediate-style declarations over a retained, incremental UI kernel.
pub use seaglass_core::*;
/// Derives `Properties`: property grid rows from a struct's fields, and
/// `tr!` and `style_sheet!`, which check a message key and a stylesheet
/// while the crate compiles.
#[cfg(feature = "derive")]
pub use seaglass_derive::{Properties, style_sheet, tr};
pub use seaglass_devtools as devtools;
#[cfg(feature = "fluent")]
pub use seaglass_fluent as fluent;
#[cfg(feature = "icu")]
pub use seaglass_icu as icu;
#[cfg(feature = "images")]
pub use seaglass_image as image;
/// Renders a scene to pixels without a graphics adapter.
pub use seaglass_render_cpu::{CpuRenderer, CpuStats};
/// The GPU renderer, for applications that draw with wgpu themselves through
/// `AppOptions::gpu`. See `Renderer::set_custom_draw`.
#[cfg(feature = "desktop")]
pub use seaglass_render_wgpu as render;
pub use seaglass_scene::{
    Affine, Affinity, Color, Image, Point, Primitive, Rect, Scene, Size, VectorImage,
};
#[cfg(feature = "syntax")]
pub use seaglass_syntax as syntax;
#[cfg(feature = "desktop")]
pub use seaglass_winit::hot;
#[cfg(feature = "desktop")]
pub use seaglass_winit::{
    AppOptions, DirectionChoice, GpuMemoryPolicy, GpuSetup, GraphicsBackend, GraphicsOptions,
    Placement, ThemeChoice, run, run_with,
};
