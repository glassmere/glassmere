//! Where pixels go: a slice, a size and a place on the screen.
use crate::Pixel;
use crate::raster::IRect;
use glassmere_scene::Rect;

/// A rectangle of pixels in a slice, and where its top left pixel sits on the
/// screen.
///
/// The slice need not be the whole screen. A microcontroller that cannot hold
/// a frame renders it in bands, one buffer moved down the screen with
/// [`set_origin`](Self::set_origin); one that redraws what changed gives the
/// changed rectangle's own buffer and origin. The renderer draws what falls
/// inside the buffer and nothing else, in screen coordinates, so a band is
/// exactly the pixels the whole frame would have there.
///
/// Pixels are stored by rows from the top, `stride` pixels apart.
pub struct Framebuffer<'a, P: Pixel> {
    pixels: &'a mut [P],
    width: u32,
    height: u32,
    stride: u32,
    origin: [i32; 2],
}

impl<'a, P: Pixel> Framebuffer<'a, P> {
    /// A buffer of `width` by `height` pixels at the screen's top left corner,
    /// its rows following each other.
    ///
    /// # Panics
    ///
    /// When `pixels` is shorter than `width * height`.
    pub fn new(pixels: &'a mut [P], width: u32, height: u32) -> Self {
        Self::with_stride(pixels, width, height, width)
    }

    /// Like `new`, with rows `stride` pixels apart, for a buffer that is a
    /// window into a larger one.
    ///
    /// # Panics
    ///
    /// When `stride` is under `width` or `pixels` cannot hold the last row.
    pub fn with_stride(pixels: &'a mut [P], width: u32, height: u32, stride: u32) -> Self {
        assert!(stride >= width, "stride {stride} is under width {width}");
        let needed = if height == 0 {
            0
        } else {
            stride as usize * (height as usize - 1) + width as usize
        };
        assert!(
            pixels.len() >= needed,
            "{} pixels cannot hold {width}x{height} with stride {stride}",
            pixels.len()
        );
        Self {
            pixels,
            width,
            height,
            stride,
            origin: [0, 0],
        }
    }

    /// Places the buffer's top left pixel at screen position `(x, y)`.
    pub fn at(mut self, x: i32, y: i32) -> Self {
        self.origin = [x, y];
        self
    }

    /// Moves the buffer on the screen, to render the next band with it.
    pub fn set_origin(&mut self, x: i32, y: i32) {
        self.origin = [x, y];
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn stride(&self) -> u32 {
        self.stride
    }

    /// The screen position of the top left pixel.
    pub fn origin(&self) -> [i32; 2] {
        self.origin
    }

    /// The part of the screen the buffer covers.
    pub fn bounds(&self) -> Rect {
        Rect {
            x: self.origin[0] as f32,
            y: self.origin[1] as f32,
            width: self.width as f32,
            height: self.height as f32,
        }
    }

    /// The pixels the buffer covers.
    pub(crate) fn screen(&self) -> IRect {
        IRect {
            x0: self.origin[0],
            y0: self.origin[1],
            x1: self.origin[0].saturating_add(self.width as i32),
            y1: self.origin[1].saturating_add(self.height as i32),
        }
    }

    /// The pixel at screen position `(x, y)`, if the buffer covers it.
    pub fn pixel(&self, x: i32, y: i32) -> Option<P> {
        let (column, row) = (
            u32::try_from(x.checked_sub(self.origin[0])?).ok()?,
            u32::try_from(y.checked_sub(self.origin[1])?).ok()?,
        );
        (column < self.width && row < self.height)
            .then(|| self.pixels[row as usize * self.stride as usize + column as usize])
    }

    /// Row `row` of the buffer, `width` pixels, counted from its top.
    ///
    /// # Panics
    ///
    /// When `row` is not under `height`.
    pub fn row(&self, row: u32) -> &[P] {
        assert!(row < self.height);
        let start = row as usize * self.stride as usize;
        &self.pixels[start..start + self.width as usize]
    }

    /// The slice the buffer lives in, `stride` pixels to a row.
    pub fn pixels(&self) -> &[P] {
        self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [P] {
        self.pixels
    }

    /// The pixels of screen row `y` from column `x0` up to `x1`. The caller
    /// has clipped the range to the buffer.
    #[inline]
    pub(crate) fn span_mut(&mut self, y: i32, x0: i32, x1: i32) -> &mut [P] {
        let start =
            (y - self.origin[1]) as usize * self.stride as usize + (x0 - self.origin[0]) as usize;
        &mut self.pixels[start..start + (x1 - x0) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Gray8;
    use alloc::vec;

    #[test]
    fn a_buffer_knows_its_place_on_the_screen() {
        let mut pixels = vec![Gray8(0); 4 * 3];
        let mut buffer = Framebuffer::new(&mut pixels, 4, 3).at(10, 20);
        assert_eq!(buffer.origin(), [10, 20]);
        assert_eq!(buffer.bounds().x, 10.0);
        assert_eq!(buffer.bounds().height, 3.0);
        buffer.span_mut(21, 11, 13).fill(Gray8(7));
        assert_eq!(buffer.pixel(11, 21), Some(Gray8(7)));
        assert_eq!(buffer.pixel(12, 21), Some(Gray8(7)));
        assert_eq!(buffer.pixel(13, 21), Some(Gray8(0)));
        assert_eq!(buffer.pixel(9, 21), None);
        assert_eq!(buffer.pixel(11, 23), None);
        assert_eq!(buffer.pixel(14, 20), None);
        assert_eq!(buffer.row(1), [Gray8(0), Gray8(7), Gray8(7), Gray8(0)]);
        buffer.set_origin(0, 0);
        assert_eq!(buffer.pixel(1, 1), Some(Gray8(7)));
    }

    #[test]
    fn stride_leaves_the_pixels_between_rows_alone() {
        let mut pixels = vec![Gray8(1); 6 * 2];
        let mut buffer = Framebuffer::with_stride(&mut pixels, 4, 2, 6);
        buffer.span_mut(1, 0, 4).fill(Gray8(9));
        assert_eq!(buffer.row(1), [Gray8(9); 4]);
        assert_eq!(buffer.pixels()[10], Gray8(1));
        assert_eq!(buffer.pixels()[6], Gray8(9));
    }

    #[test]
    #[should_panic(expected = "cannot hold")]
    fn a_slice_that_is_too_short_is_refused() {
        let mut pixels = vec![Gray8(0); 11];
        let _ = Framebuffer::new(&mut pixels, 4, 3);
    }
}
