//! Hands a drag that left the window to the operating system.
//!
//! The core's side is payload-agnostic (`Context::dragging`, `export_drag`,
//! `finish_exported_drag`), so what can leave the app is decided here alone:
//! files, through the `drag` crate, on Windows and macOS. Text needs a data
//! object of our own and would be one more arm in `start`.
use rust_gui_core::{DragOutcome, DragPayload};
use std::sync::mpsc::{Receiver, channel};
use winit::window::Window;

/// Starts a system drag. `None` means this payload cannot leave the app here,
/// and the in-app drag simply goes on. On Windows this returns only when the
/// drag is over; on macOS the outcome arrives later.
#[cfg(any(windows, target_os = "macos"))]
pub(crate) fn start(window: &Window, payload: &DragPayload) -> Option<Receiver<DragOutcome>> {
    let DragPayload::Files(files) = payload else {
        return None;
    };
    let (sender, receiver) = channel();
    drag::start_drag(
        window,
        drag::DragItem::Files(files.clone()),
        drag::Image::Raw(include_bytes!("drag-icon.png").to_vec()),
        move |result, _| {
            let _ = sender.send(match result {
                drag::DragResult::Dropped => DragOutcome::Dropped,
                drag::DragResult::Cancel => DragOutcome::Cancelled,
            });
        },
        // ponytail: always a copy. A move makes the receiving app delete the
        // source file; add a flag on the payload when an app needs that.
        drag::Options::default(),
    )
    // A file that does not exist is the usual reason.
    .inspect_err(|error| tracing::warn!("system drag not started: {error}"))
    .ok()?;
    Some(receiver)
}

// ponytail: the `drag` crate needs a GTK window on Linux, which winit does not
// give us. Upgrade path: XDND and wl_data_device of our own.
#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn start(_: &Window, _: &DragPayload) -> Option<Receiver<DragOutcome>> {
    None
}
