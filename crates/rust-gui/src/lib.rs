//! Immediate-style declarations over a retained, incremental UI kernel.
pub use rust_gui_core::*;
/// Derives `Properties`: property grid rows from a struct's fields.
#[cfg(feature = "derive")]
pub use rust_gui_derive::{Properties, tr};
pub use rust_gui_devtools as devtools;
#[cfg(feature = "fluent")]
pub use rust_gui_fluent as fluent;
#[cfg(feature = "images")]
pub use rust_gui_image as image;
pub use rust_gui_scene::{
    Affine, Affinity, Color, Image, Point, Primitive, Rect, Scene, Size, VectorImage,
};
#[cfg(feature = "syntax")]
pub use rust_gui_syntax as syntax;
#[cfg(feature = "desktop")]
pub use rust_gui_winit::hot;
#[cfg(feature = "desktop")]
pub use rust_gui_winit::{
    AppOptions, DesktopClosed, DesktopHandle, DirectionChoice, DockWindowView, DockWindows,
    GpuMemoryPolicy, GraphicsBackend, GraphicsOptions, Mailbox, MessageSender, ThemeChoice,
    WakeHandle, WindowKey, run, run_windows, run_with_wakeup,
};
