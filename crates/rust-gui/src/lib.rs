//! Immediate-style declarations over a retained, incremental UI kernel.
pub use rust_gui_core::*;
pub use rust_gui_devtools as devtools;
pub use rust_gui_scene::{Color, Point, Primitive, Rect, Scene, Size};
#[cfg(feature = "desktop")]
pub use rust_gui_winit::{
    AppOptions, DesktopClosed, DesktopHandle, GpuMemoryPolicy, GraphicsBackend, GraphicsOptions,
    Mailbox, MessageSender, WakeHandle, WindowKey, run, run_windows, run_with_wakeup,
};
