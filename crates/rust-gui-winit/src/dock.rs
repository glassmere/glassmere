use crate::{AppOptions, DesktopHandle, WindowKey};
use rust_gui_core::{DockLayout, DockWindow, DockWindowId, Ui};
use std::collections::HashMap;

/// What to open for one detached dock window.
pub struct DockWindowView {
    pub options: AppOptions,
    /// Usually locks the shared app state and calls `Ui::dock_window`.
    pub view: Box<dyn FnMut(&mut Ui<'_>) + Send>,
    /// Runs once when the OS window is gone, whether the user closed it, the
    /// layout dropped it or the desktop shut down. Typically
    /// `layout.redock(id)`, which does nothing if the layout dropped it first.
    pub on_close: Box<dyn FnOnce() + Send>,
}

struct OnDrop(Option<Box<dyn FnOnce() + Send>>);
impl Drop for OnDrop {
    fn drop(&mut self) {
        if let Some(run) = self.0.take() {
            run();
        }
    }
}

/// Keeps one OS window per `DockLayout::windows` entry. Call `sync` from the
/// main window's view after `Ui::dock`; it only sends requests to the desktop,
/// so holding the app state lock while calling it is fine.
#[derive(Default)]
pub struct DockWindows {
    open: HashMap<DockWindowId, WindowKey>,
}

impl DockWindows {
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens windows for new layout entries and closes windows whose entry left.
    pub fn sync(
        &mut self,
        desktop: &DesktopHandle,
        layout: &DockLayout,
        mut open: impl FnMut(&DockWindow) -> DockWindowView,
    ) {
        self.open.retain(|id, key| {
            let keep = layout.window(*id).is_some();
            if !keep {
                desktop.close_window(*key);
            }
            keep
        });
        for window in &layout.windows {
            if self.open.contains_key(&window.id) {
                continue;
            }
            let DockWindowView {
                options,
                mut view,
                on_close,
            } = open(window);
            // The desktop drops a window's view when the window closes.
            let closed = OnDrop(Some(on_close));
            if let Ok(key) = desktop.open_window(options, move |ui| {
                let _ = &closed;
                view(ui);
            }) {
                self.open.insert(window.id, key);
            }
        }
    }

    /// Requests a redraw of every detached window, for changes made elsewhere.
    pub fn redraw(&self, desktop: &DesktopHandle) {
        for key in self.open.values() {
            desktop.redraw(*key);
        }
    }
}
