//! The stopwatch behind the per-frame timings. Without `std` there is no
//! clock to read, so every timing is zero.

#[cfg(feature = "std")]
pub(crate) use std::time::Instant;

#[cfg(not(feature = "std"))]
#[derive(Clone, Copy)]
pub(crate) struct Instant;

#[cfg(not(feature = "std"))]
impl Instant {
    pub(crate) fn now() -> Self {
        Self
    }

    pub(crate) fn elapsed(&self) -> core::time::Duration {
        core::time::Duration::ZERO
    }
}
