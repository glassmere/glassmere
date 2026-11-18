//! The float functions `core` does not have, from `libm`.

pub(crate) trait FloatMath {
    fn floorf(self) -> f32;
    fn ceilf(self) -> f32;
    fn sqrtf(self) -> f32;
    /// Nearest integer, halves up: what snaps a position to the pixel grid.
    fn snapf(self) -> f32;
}

impl FloatMath for f32 {
    #[inline]
    fn floorf(self) -> f32 {
        libm::floorf(self)
    }

    #[inline]
    fn ceilf(self) -> f32 {
        libm::ceilf(self)
    }

    #[inline]
    fn sqrtf(self) -> f32 {
        libm::sqrtf(self)
    }

    #[inline]
    fn snapf(self) -> f32 {
        libm::floorf(self + 0.5)
    }
}
