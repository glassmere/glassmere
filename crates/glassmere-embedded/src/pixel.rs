//! Pixel formats. The renderer reads and writes pixels as 8-bit RGB, so a
//! format only says how that is packed.

/// A pixel of a framebuffer.
///
/// To draw to a display with another layout, such as RGB565 with the bytes
/// swapped for a SPI panel, implement this for a type of your own.
pub trait Pixel: Copy {
    fn from_rgb(rgb: [u8; 3]) -> Self;
    fn to_rgb(self) -> [u8; 3];
}

/// 16 bits, five of red, six of green, five of blue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Rgb565(pub u16);

/// Three bytes, red first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Rgb888(pub [u8; 3]);

/// 32 bits, `0xFFRRGGBB`: the top byte is not used by displays and is set to
/// full so that it reads as an opaque ARGB pixel too.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Xrgb8888(pub u32);

/// One byte of grey.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Gray8(pub u8);

/// One bit, as a bool: `true` is lit. Colours at least half as bright as
/// white by luma, and shapes at least half covered, come out lit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Mono(pub bool);

/// ITU-R BT.601 luma, `0..=255`.
fn luma([r, g, b]: [u8; 3]) -> u8 {
    ((u32::from(r) * 77 + u32::from(g) * 150 + u32::from(b) * 29 + 128) >> 8) as u8
}

impl Pixel for Rgb565 {
    #[inline]
    fn from_rgb([r, g, b]: [u8; 3]) -> Self {
        let r = (u16::from(r) * 31 + 127) / 255;
        let g = (u16::from(g) * 63 + 127) / 255;
        let b = (u16::from(b) * 31 + 127) / 255;
        Self(r << 11 | g << 5 | b)
    }

    #[inline]
    fn to_rgb(self) -> [u8; 3] {
        let (r, g, b) = (self.0 >> 11, (self.0 >> 5) & 63, self.0 & 31);
        [
            (r << 3 | r >> 2) as u8,
            (g << 2 | g >> 4) as u8,
            (b << 3 | b >> 2) as u8,
        ]
    }
}

impl Pixel for Rgb888 {
    #[inline]
    fn from_rgb(rgb: [u8; 3]) -> Self {
        Self(rgb)
    }

    #[inline]
    fn to_rgb(self) -> [u8; 3] {
        self.0
    }
}

impl Pixel for Xrgb8888 {
    #[inline]
    fn from_rgb([r, g, b]: [u8; 3]) -> Self {
        Self(0xFF00_0000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
    }

    #[inline]
    fn to_rgb(self) -> [u8; 3] {
        [(self.0 >> 16) as u8, (self.0 >> 8) as u8, self.0 as u8]
    }
}

impl Pixel for Gray8 {
    #[inline]
    fn from_rgb(rgb: [u8; 3]) -> Self {
        Self(luma(rgb))
    }

    #[inline]
    fn to_rgb(self) -> [u8; 3] {
        [self.0; 3]
    }
}

impl Pixel for Mono {
    #[inline]
    fn from_rgb(rgb: [u8; 3]) -> Self {
        Self(luma(rgb) >= 128)
    }

    #[inline]
    fn to_rgb(self) -> [u8; 3] {
        [if self.0 { 255 } else { 0 }; 3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb565_round_trips_every_value_and_keeps_the_extremes() {
        for value in 0..=u16::MAX {
            let pixel = Rgb565(value);
            assert_eq!(Rgb565::from_rgb(pixel.to_rgb()), pixel);
        }
        assert_eq!(Rgb565::from_rgb([255, 255, 255]), Rgb565(0xFFFF));
        assert_eq!(Rgb565::from_rgb([0, 0, 0]), Rgb565(0));
        assert_eq!(Rgb565(0xFFFF).to_rgb(), [255, 255, 255]);
        assert_eq!(Rgb565::from_rgb([255, 0, 0]), Rgb565(0xF800));
        assert_eq!(Rgb565::from_rgb([0, 255, 0]), Rgb565(0x07E0));
        assert_eq!(Rgb565::from_rgb([0, 0, 255]), Rgb565(0x001F));
        // Rounded, not cut: a mid grey lands on the middle step.
        assert_eq!(Rgb565::from_rgb([128, 128, 128]), Rgb565(0x8410));
    }

    #[test]
    fn the_other_formats_hold_their_colours() {
        assert_eq!(Rgb888::from_rgb([1, 2, 3]).to_rgb(), [1, 2, 3]);
        assert_eq!(Xrgb8888::from_rgb([1, 2, 3]), Xrgb8888(0xFF01_0203));
        assert_eq!(Xrgb8888(0x0004_0506).to_rgb(), [4, 5, 6]);
        assert_eq!(Gray8::from_rgb([255, 255, 255]), Gray8(255));
        assert_eq!(Gray8::from_rgb([0, 0, 0]), Gray8(0));
        assert_eq!(Gray8::from_rgb([200, 200, 200]), Gray8(200));
        assert!(Gray8::from_rgb([0, 255, 0]).0 > Gray8::from_rgb([255, 0, 0]).0);
        assert!(Gray8::from_rgb([255, 0, 0]).0 > Gray8::from_rgb([0, 0, 255]).0);
        assert_eq!(Gray8(77).to_rgb(), [77; 3]);
    }

    #[test]
    fn mono_thresholds_at_half_brightness() {
        assert_eq!(Mono::from_rgb([255, 255, 255]), Mono(true));
        assert_eq!(Mono::from_rgb([0, 0, 0]), Mono(false));
        assert_eq!(Mono::from_rgb([127, 127, 127]), Mono(false));
        assert_eq!(Mono::from_rgb([128, 128, 128]), Mono(true));
        assert_eq!(Mono(true).to_rgb(), [255; 3]);
        assert_eq!(Mono(false).to_rgb(), [0; 3]);
    }
}
