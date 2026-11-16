//! The float functions `core` lacks, so that scene math reads the same with and without `std`.

pub(crate) trait FloatExt {
    fn floor(self) -> Self;
    fn powf(self, exponent: Self) -> Self;
    fn sin_cos(self) -> (Self, Self)
    where
        Self: Sized;
}

impl FloatExt for f32 {
    fn floor(self) -> Self {
        libm::floorf(self)
    }

    fn powf(self, exponent: Self) -> Self {
        libm::powf(self, exponent)
    }

    fn sin_cos(self) -> (Self, Self) {
        libm::sincosf(self)
    }
}
