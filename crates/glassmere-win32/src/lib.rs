//! What Windows will only tell a window procedure.
//!
//! A window that draws its own title bar has to answer `WM_NCHITTEST` with
//! `HTMAXBUTTON` over its maximize button, or Windows 11 will not offer the
//! snap layouts flyout when the pointer rests there. That message is *sent*,
//! not posted, so no message loop can answer it: only the window procedure
//! can, and reaching it means subclassing.
//!
//! This is the one crate in the workspace that uses unsafe code, and this is
//! all of it: one procedure, a shared rectangle it reads, and a list of what
//! it saw. Everything it learns is handed back as ordinary events for the
//! framework to deal with in its usual way.
#![cfg(windows)]

use std::sync::{Arc, Mutex};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::Gdi::ScreenToClient,
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            HTMAXBUTTON, SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW, WM_NCHITTEST,
            WM_NCMOUSELEAVE, WM_NCMOUSEMOVE,
        },
    },
};

/// What the window procedure saw over the maximize button. The framework
/// does not hear of it otherwise: while the button is part of the caption as
/// far as Windows is concerned, no ordinary mouse message arrives there.
/// Pressing it is left to Windows, which maximizes the window itself, and
/// offers its layouts while the pointer rests there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionEvent {
    /// The pointer is over the button, at this point in client pixels.
    Hover(i32, i32),
    Left,
}

#[derive(Default)]
struct Shared {
    /// The maximize button in client pixels: left, top, right, bottom.
    button: Mutex<Option<(i32, i32, i32, i32)>>,
    seen: Mutex<Vec<CaptionEvent>>,
}

impl Shared {
    fn contains(&self, x: i32, y: i32) -> bool {
        self.button.lock().is_ok_and(|button| {
            button.is_some_and(|(left, top, right, bottom)| {
                x >= left && x < right && y >= top && y < bottom
            })
        })
    }

    fn saw(&self, event: CaptionEvent) {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(event);
        }
    }
}

/// A window whose maximize button Windows knows about. Dropping it puts the
/// window back the way it was.
pub struct Caption {
    window: HWND,
    shared: Arc<Shared>,
}

/// Whether the user has switched off animations in Windows (Settings,
/// Accessibility, Visual effects), `Some(true)` meaning they have. `None`
/// when Windows would not say.
pub fn animations_disabled() -> Option<bool> {
    let mut enabled: i32 = 1;
    // SAFETY: for `SPI_GETCLIENTAREAANIMATION`, `pvParam` points to a `BOOL`
    // that Windows writes; `enabled` is one and outlives the call.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut enabled as *mut i32).cast(),
            0,
        )
    } != 0;
    read.then_some(enabled == 0)
}

/// Which subclass this is, among whatever else has subclassed the window.
const ID: usize = 0x7275_6769;

/// Tells Windows where a window's own maximize button is, from now on.
/// `window` is a `HWND` as `raw-window-handle` reports it. `None` when the
/// window would not take the subclass.
pub fn attach(window: isize) -> Option<Caption> {
    let window = window as HWND;
    let shared = Arc::new(Shared::default());
    // The procedure holds a reference of its own, released by `detach`.
    let reference = Arc::into_raw(Arc::clone(&shared)) as usize;
    // SAFETY: `window` is a live window of this thread, `caption` is a
    // `SUBCLASSPROC`, and `reference` is a pointer the procedure only reads
    // as `&Shared` and that stays alive until `RemoveWindowSubclass`.
    let attached = unsafe { SetWindowSubclass(window, Some(caption), ID, reference) } != 0;
    if !attached {
        // SAFETY: the pointer came from `Arc::into_raw` just above and
        // nothing else took it.
        drop(unsafe { Arc::from_raw(reference as *const Shared) });
        return None;
    }
    Some(Caption { window, shared })
}

impl Caption {
    /// Where the maximize button is, in client pixels, or `None` when the
    /// view has none. Set it after every layout.
    pub fn set_maximize(&self, button: Option<(i32, i32, i32, i32)>) {
        if let Ok(mut held) = self.shared.button.lock() {
            *held = button;
        }
    }

    /// What the window procedure saw since the last call.
    pub fn take(&self) -> Vec<CaptionEvent> {
        self.shared
            .seen
            .lock()
            .map(|mut seen| std::mem::take(&mut *seen))
            .unwrap_or_default()
    }
}

impl Drop for Caption {
    fn drop(&mut self) {
        // SAFETY: the same window and id the subclass was attached with.
        let removed = unsafe { RemoveWindowSubclass(self.window, Some(caption), ID) } != 0;
        if removed {
            // SAFETY: the procedure's reference, which it can no longer use.
            drop(unsafe { Arc::from_raw(Arc::as_ptr(&self.shared)) });
        }
    }
}

/// The window procedure. It answers where the maximize button is and reports
/// what happens over it; everything else goes on to whoever came before.
///
/// # Safety
///
/// Called by Windows for the window this was attached to, with `reference`
/// as it was given to `SetWindowSubclass`.
unsafe extern "system" fn caption(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    reference: usize,
) -> LRESULT {
    // SAFETY: `reference` is the `Arc<Shared>` pointer from `attach`, which
    // outlives the subclass, and this only borrows it for the call.
    let shared = unsafe { &*(reference as *const Shared) };
    let forward = || {
        // SAFETY: forwarding the message this procedure was given.
        unsafe { DefSubclassProc(window, message, wparam, lparam) }
    };
    match message {
        WM_NCHITTEST => {
            // The default answer first, so borders and everything else keep
            // working; only the button's own rectangle is overridden.
            let hit = forward();
            let mut point = POINT {
                x: (lparam & 0xffff) as i16 as i32,
                y: ((lparam >> 16) & 0xffff) as i16 as i32,
            };
            // SAFETY: `point` is a live local, and `window` is this window.
            if unsafe { ScreenToClient(window, &raw mut point) } == 0 {
                return hit;
            }
            if shared.contains(point.x, point.y) {
                HTMAXBUTTON as LRESULT
            } else {
                hit
            }
        }
        WM_NCMOUSEMOVE if wparam == HTMAXBUTTON as WPARAM => {
            let mut point = POINT {
                x: (lparam & 0xffff) as i16 as i32,
                y: ((lparam >> 16) & 0xffff) as i16 as i32,
            };
            // SAFETY: as above.
            if unsafe { ScreenToClient(window, &raw mut point) } != 0 {
                shared.saw(CaptionEvent::Hover(point.x, point.y));
            }
            // Forwarded, because this is the message Windows itself watches
            // to decide when to offer its window layouts.
            forward()
        }
        WM_NCMOUSELEAVE => {
            shared.saw(CaptionEvent::Left);
            forward()
        }
        _ => forward(),
    }
}
