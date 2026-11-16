//! The float functions `core` lacks, so that layout and paint math reads the
//! same with and without `std`. Imported as `FloatExt as _` or `DoubleExt as _`
//! where it is needed, and only when `std` is off.

/// The `f32` methods the crate calls.
pub(crate) trait FloatExt: Sized {
    fn floor(self) -> Self;
    fn ceil(self) -> Self;
    fn round(self) -> Self;
    fn hypot(self, other: Self) -> Self;
    fn atan2(self, other: Self) -> Self;
    fn sin(self) -> Self;
    fn cos(self) -> Self;
    fn exp(self) -> Self;
    fn rem_euclid(self, rhs: Self) -> Self;
}

impl FloatExt for f32 {
    fn floor(self) -> Self {
        libm::floorf(self)
    }

    fn ceil(self) -> Self {
        libm::ceilf(self)
    }

    fn round(self) -> Self {
        libm::roundf(self)
    }

    fn hypot(self, other: Self) -> Self {
        libm::hypotf(self, other)
    }

    fn atan2(self, other: Self) -> Self {
        libm::atan2f(self, other)
    }

    fn sin(self) -> Self {
        libm::sinf(self)
    }

    fn cos(self) -> Self {
        libm::cosf(self)
    }

    fn exp(self) -> Self {
        libm::expf(self)
    }

    fn rem_euclid(self, rhs: Self) -> Self {
        let remainder = self % rhs;
        if remainder < 0.0 {
            remainder + rhs.abs()
        } else {
            remainder
        }
    }
}

/// The `f64` methods the crate calls.
pub(crate) trait DoubleExt: Sized {
    fn round(self) -> Self;
    fn powi(self, exponent: i32) -> Self;
}

impl DoubleExt for f64 {
    fn round(self) -> Self {
        libm::round(self)
    }

    fn powi(self, exponent: i32) -> Self {
        libm::pow(self, f64::from(exponent))
    }
}
