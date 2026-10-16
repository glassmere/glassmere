//! Immediate-style declarations over a retained, incremental UI kernel.
pub use rust_gui_core::*;
/// Derives `Properties`: property grid rows from a struct's fields, and
/// `tr!` and `style_sheet!`, which check a message key and a stylesheet
/// while the crate compiles.
#[cfg(feature = "derive")]
pub use rust_gui_derive::{Properties, style_sheet, tr};
pub use rust_gui_devtools as devtools;
#[cfg(feature = "fluent")]
pub use rust_gui_fluent as fluent;
#[cfg(feature = "icu")]
pub use rust_gui_icu as icu;
#[cfg(feature = "images")]
pub use rust_gui_image as image;
/// Renders a scene to pixels without a graphics adapter.
pub use rust_gui_render_cpu::{CpuRenderer, CpuStats};
/// The GPU renderer, for applications that draw with wgpu themselves through
/// `AppOptions::gpu`. See `Renderer::set_custom_draw`.
#[cfg(feature = "desktop")]
pub use rust_gui_render_wgpu as render;
pub use rust_gui_scene::{
    Affine, Affinity, Color, Image, Point, Primitive, Rect, Scene, Size, VectorImage,
};
#[cfg(feature = "syntax")]
pub use rust_gui_syntax as syntax;
#[cfg(feature = "desktop")]
pub use rust_gui_winit::hot;
#[cfg(feature = "desktop")]
pub use rust_gui_winit::{
    AppOptions, DirectionChoice, GpuMemoryPolicy, GpuSetup, GraphicsBackend, GraphicsOptions,
    Placement, ThemeChoice, WakeHandle, run, run_with,
};
