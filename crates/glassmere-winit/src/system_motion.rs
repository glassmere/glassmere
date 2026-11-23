//! The operating system's reduced-motion preference, and how a window finds
//! out that the answer changed. Windows asks the system on the spot. Linux and
//! the BSDs ask `gsettings` on a background thread and keep the answer; its
//! generation changes whenever the answer does, and the thread then wakes the
//! event loop. Elsewhere there is no preference.

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
mod gsettings;
#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
pub(crate) use gsettings::{generation, refresh, snapshot};

#[cfg(not(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
)))]
mod elsewhere {
    /// The generation of the answer and the answer itself.
    #[cfg(windows)]
    pub(crate) fn snapshot() -> (usize, Option<bool>) {
        (0, glassmere_win32::animations_disabled())
    }

    /// No platform query exists here: no preference.
    #[cfg(not(windows))]
    pub(crate) fn snapshot() -> (usize, Option<bool>) {
        (0, None)
    }

    /// The answer is asked for on the spot, so it never turns out newer.
    pub(crate) fn generation() -> usize {
        0
    }

    pub(crate) fn refresh(_wake: &crate::wakeup::WakeHandle) {}

    #[cfg(test)]
    mod tests {
        use super::{generation, snapshot};

        #[test]
        fn a_platform_without_a_probe_never_reports_a_newer_answer() {
            assert_eq!(generation(), 0);
            assert_eq!(snapshot().0, 0);
            #[cfg(not(windows))]
            assert_eq!(snapshot().1, None);
        }
    }
}
#[cfg(not(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
)))]
pub(crate) use elsewhere::{generation, refresh, snapshot};
