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
//! framework to deal with in its usual way. With the `menu` feature it also
//! puts a menu in a window's menu bar, which takes a raw window handle.
#![cfg(windows)]

#[cfg(feature = "menu")]
mod menu;
#[cfg(feature = "menu")]
pub use menu::{MenuBar, attach_menu};

use std::sync::{Arc, Mutex};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::{Dwm::DwmExtendFrameIntoClientArea, Gdi::ScreenToClient},
    UI::Controls::MARGINS,
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            HTCLOSE, HTMAXBUTTON, HTMINBUTTON, SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
            WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE,
            WM_NCMOUSEMOVE,
        },
    },
};

/// Where the pointer went over the caption buttons: [`CaptionInput`] without
/// the presses, as [`Caption::take`] reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionEvent {
    /// The pointer is over a button, at this point in client pixels.
    Hover(i32, i32),
    Left,
}

/// What the window procedure saw over the caption buttons. The framework
/// does not hear of it otherwise: while the buttons are part of the caption
/// as far as Windows is concerned, no ordinary mouse message arrives there.
/// Windows shows their tooltips and offers its layouts over the maximize
/// button, but does not press the buttons of a window without its own
/// caption, so presses are handed on for the framework's buttons to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaptionInput {
    /// The pointer is over a button, at this point in client pixels.
    Hover(i32, i32),
    Left,
    /// A button was pressed or released, at this point in client pixels.
    Down(i32, i32),
    Up(i32, i32),
}

/// A button in client pixels: left, top, right, bottom.
type Button = (i32, i32, i32, i32);

/// What Windows calls each caption button, in the order `Shared` keeps them.
const HITS: [u32; 3] = [HTMINBUTTON, HTMAXBUTTON, HTCLOSE];

#[derive(Default)]
struct Shared {
    /// Minimize, maximize and close.
    buttons: Mutex<[Option<Button>; 3]>,
    seen: Mutex<Vec<CaptionInput>>,
}

impl Shared {
    /// The caption button at a point, as Windows names it.
    fn hit(&self, x: i32, y: i32) -> Option<u32> {
        let buttons = self.buttons.lock().ok()?;
        buttons.iter().zip(HITS).find_map(|(button, hit)| {
            button
                .is_some_and(|(left, top, right, bottom)| {
                    x >= left && x < right && y >= top && y < bottom
                })
                .then_some(hit)
        })
    }

    fn set(&self, index: usize, button: Option<Button>) {
        if let Ok(mut held) = self.buttons.lock() {
            held[index] = button;
        }
    }

    fn saw(&self, event: CaptionInput) {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(event);
        }
    }
}

/// A window whose caption buttons Windows knows about. Dropping it puts the
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

/// Gives a window without the system's frame the system's shadow, by
/// extending the frame one pixel into the bottom of the client area. Only the
/// bottom, because a frame reaching into the top is where Windows looks for a
/// caption and stops offering its layouts over the maximize button.
pub fn frame_shadow(window: isize) {
    let margins = MARGINS {
        cxLeftWidth: 0,
        cxRightWidth: 0,
        cyTopHeight: 0,
        cyBottomHeight: 1,
    };
    // SAFETY: `margins` is a live local, and a window handle that is no
    // longer valid only makes the call fail.
    unsafe { DwmExtendFrameIntoClientArea(window as HWND, &raw const margins) };
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
        self.shared.set(1, button);
    }

    /// Where the minimize button is, as `set_maximize`.
    pub fn set_minimize(&self, button: Option<(i32, i32, i32, i32)>) {
        self.shared.set(0, button);
    }

    /// Where the close button is, as `set_maximize`.
    pub fn set_close(&self, button: Option<(i32, i32, i32, i32)>) {
        self.shared.set(2, button);
    }

    /// Where the pointer went since the last call, leaving out presses.
    pub fn take(&self) -> Vec<CaptionEvent> {
        self.take_input()
            .into_iter()
            .filter_map(|input| match input {
                CaptionInput::Hover(x, y) => Some(CaptionEvent::Hover(x, y)),
                CaptionInput::Left => Some(CaptionEvent::Left),
                _ => None,
            })
            .collect()
    }

    /// What the window procedure saw since the last call, in order.
    pub fn take_input(&self) -> Vec<CaptionInput> {
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

/// The window procedure. It answers where the caption buttons are and reports
/// what happens over them; everything else goes on to whoever came before.
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
            client_point(window, lparam)
                .and_then(|(x, y)| shared.hit(x, y))
                .map_or(hit, |button| button as LRESULT)
        }
        WM_NCMOUSEMOVE if is_button(wparam) => {
            if let Some((x, y)) = client_point(window, lparam) {
                shared.saw(CaptionInput::Hover(x, y));
            }
            // Forwarded, because this is the message Windows itself watches
            // to decide when to offer its window layouts.
            forward()
        }
        // Kept from Windows, which would otherwise start a move or track a
        // caption button the window does not have.
        WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if is_button(wparam) => {
            if let Some((x, y)) = client_point(window, lparam) {
                shared.saw(CaptionInput::Down(x, y));
            }
            0
        }
        WM_NCLBUTTONUP if is_button(wparam) => {
            if let Some((x, y)) = client_point(window, lparam) {
                shared.saw(CaptionInput::Up(x, y));
            }
            0
        }
        WM_NCMOUSELEAVE => {
            shared.saw(CaptionInput::Left);
            forward()
        }
        _ => forward(),
    }
}

/// The screen point a non-client message carries, in client pixels.
fn client_point(window: HWND, lparam: LPARAM) -> Option<(i32, i32)> {
    let mut point = POINT {
        x: (lparam & 0xffff) as i16 as i32,
        y: ((lparam >> 16) & 0xffff) as i16 as i32,
    };
    // SAFETY: `point` is a live local, and `window` is the window the
    // message was sent to.
    (unsafe { ScreenToClient(window, &raw mut point) } != 0).then_some((point.x, point.y))
}

/// Whether a hit-test code a message carries is one of the caption buttons.
fn is_button(wparam: WPARAM) -> bool {
    HITS.iter().any(|&hit| wparam == hit as WPARAM)
}
