//! Bitmap fonts: fixed-size cells of one bit per pixel, kept in flash.
use crate::fonts::{FIXED_6X13, FIXED_6X13_BOLD, FIXED_9X18, FIXED_9X18_BOLD};

/// Consecutive code points `first..=last` whose glyphs start at glyph number
/// `index` of the font's data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphRange {
    first: u32,
    last: u32,
    index: u32,
}

impl GlyphRange {
    pub const fn new(first: u32, last: u32, index: u32) -> Self {
        Self { first, last, index }
    }
}

/// A font whose glyphs all sit in cells of one size, so that text is laid out
/// on a grid. A glyph is `height` rows of `ceil(width / 8)` bytes, the leftmost
/// pixel in the top bit of a row's first byte. The baseline lies `ascent`
/// rows below the top of the cell.
///
/// Make one from a BDF font with `examples/bdf_convert.rs`, or by hand:
///
/// ```
/// use glassmere_embedded::{BitmapFont, GlyphRange};
///
/// // One 4x4 glyph, 'A', a box.
/// static BOX: BitmapFont = BitmapFont::new(
///     4,
///     4,
///     3,
///     &[GlyphRange::new('A' as u32, 'A' as u32, 0)],
///     &[0xF0, 0x90, 0x90, 0xF0],
/// );
/// assert_eq!(BOX.glyph('A').unwrap()[0], 0xF0);
/// assert!(BOX.glyph('B').is_none());
/// ```
#[derive(Debug)]
pub struct BitmapFont {
    width: u8,
    height: u8,
    ascent: u8,
    ranges: &'static [GlyphRange],
    data: &'static [u8],
}

impl BitmapFont {
    /// `ranges` must be sorted, not overlap, and number the glyphs in `data`
    /// one after the other from 0. Anything else is a compile error when the
    /// font is a `static`.
    pub const fn new(
        width: u8,
        height: u8,
        ascent: u8,
        ranges: &'static [GlyphRange],
        data: &'static [u8],
    ) -> Self {
        assert!(width > 0 && height > 0 && ascent <= height);
        let mut glyphs = 0usize;
        let mut at = 0;
        while at < ranges.len() {
            let range = ranges[at];
            assert!(range.first <= range.last);
            assert!(at == 0 || ranges[at - 1].last < range.first);
            assert!(range.index as usize == glyphs);
            glyphs += (range.last - range.first) as usize + 1;
            at += 1;
        }
        assert!(data.len() == glyphs * Self::glyph_bytes(width, height));
        Self {
            width,
            height,
            ascent,
            ranges,
            data,
        }
    }

    const fn glyph_bytes(width: u8, height: u8) -> usize {
        (width as usize).div_ceil(8) * height as usize
    }

    /// Cell width, which is every glyph's advance.
    pub const fn width(&self) -> u32 {
        self.width as u32
    }

    /// Cell height, which is the line height.
    pub const fn height(&self) -> u32 {
        self.height as u32
    }

    /// Rows from the top of the cell to the baseline.
    pub const fn ascent(&self) -> u32 {
        self.ascent as u32
    }

    /// The rows of `ch`, or `None` when the font has no glyph for it.
    pub fn glyph(&self, ch: char) -> Option<&'static [u8]> {
        let code = ch as u32;
        let range = self
            .ranges
            .binary_search_by(|range| {
                if code < range.first {
                    core::cmp::Ordering::Greater
                } else if code > range.last {
                    core::cmp::Ordering::Less
                } else {
                    core::cmp::Ordering::Equal
                }
            })
            .ok()
            .map(|at| self.ranges[at])?;
        let size = Self::glyph_bytes(self.width, self.height);
        let start = (range.index + (code - range.first)) as usize * size;
        self.data.get(start..start + size)
    }

    /// Bytes of font data, for budgeting flash.
    pub fn data_len(&self) -> usize {
        self.data.len()
    }
}

/// A regular face and, when there is one, a bold one of the same cell.
#[derive(Clone, Copy, Debug)]
pub struct FontFace {
    regular: &'static BitmapFont,
    bold: Option<&'static BitmapFont>,
}

impl FontFace {
    /// The bold font must have the regular one's cell and baseline, so that
    /// text lays out the same in either.
    pub const fn new(regular: &'static BitmapFont, bold: Option<&'static BitmapFont>) -> Self {
        if let Some(bold) = bold {
            assert!(
                bold.width == regular.width
                    && bold.height == regular.height
                    && bold.ascent == regular.ascent
            );
        }
        Self { regular, bold }
    }

    pub const fn width(&self) -> u32 {
        self.regular.width()
    }

    pub const fn height(&self) -> u32 {
        self.regular.height()
    }

    pub const fn ascent(&self) -> u32 {
        self.regular.ascent()
    }

    /// The glyph of `ch`, from the bold font when asked and when it has one,
    /// else from the regular font.
    pub fn glyph(&self, ch: char, bold: bool) -> Option<&'static [u8]> {
        if bold && let Some(font) = self.bold {
            font.glyph(ch).or_else(|| self.regular.glyph(ch))
        } else {
            self.regular.glyph(ch)
        }
    }
}

const BUILTIN_FACES: &[FontFace] = &[
    FontFace::new(&FIXED_6X13, Some(&FIXED_6X13_BOLD)),
    FontFace::new(&FIXED_9X18, Some(&FIXED_9X18_BOLD)),
];

/// The faces text can be set in, smallest cell first. A style gets the face
/// whose height is nearest its `font_size`, and the bold glyphs at weight 600
/// and over; family and italic are ignored.
#[derive(Clone, Copy, Debug)]
pub struct FontSet {
    faces: &'static [FontFace],
}

impl FontSet {
    /// The faces shipped with this crate: 6x13 and 9x18, each with a bold
    /// version, covering Latin-1, Latin Extended-A and common punctuation and
    /// symbols. About 35 KiB of flash; a set that names only some of them
    /// leaves the others out of the binary.
    pub const BUILTIN: Self = Self::new(BUILTIN_FACES);

    /// `faces` must not be empty and must grow in cell height.
    pub const fn new(faces: &'static [FontFace]) -> Self {
        assert!(!faces.is_empty());
        let mut at = 1;
        while at < faces.len() {
            assert!(faces[at - 1].height() < faces[at].height());
            at += 1;
        }
        Self { faces }
    }

    pub const fn faces(&self) -> &'static [FontFace] {
        self.faces
    }

    /// The face nearest `font_size` in height; the smaller one on a tie, and
    /// the smallest for a size that is not a number.
    pub fn face_for(&self, font_size: f32) -> &'static FontFace {
        let mut best = &self.faces[0];
        let mut distance = f32::INFINITY;
        for face in self.faces {
            let away = (face.height() as f32 - font_size).abs();
            if away < distance {
                best = face;
                distance = away;
            }
        }
        best
    }
}

impl Default for FontSet {
    fn default() -> Self {
        Self::BUILTIN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_fonts_cover_latin_and_symbols() {
        for face in FontSet::BUILTIN.faces() {
            for code in (0x20..=0x7E).chain(0xA0..=0x17F) {
                let ch = char::from_u32(code).unwrap();
                assert!(face.glyph(ch, false).is_some(), "{ch:?} in regular");
                assert!(face.glyph(ch, true).is_some(), "{ch:?} in bold");
            }
            for ch in "–—‘’“”•…€←↑→↓✓☐☑●○▾▸×‹›".chars()
            {
                assert!(face.glyph(ch, false).is_some(), "{ch:?} in regular");
                // The bold fonts lack some symbols and borrow them.
                assert!(face.glyph(ch, true).is_some(), "{ch:?} in bold");
            }
            assert!(face.glyph('\u{4e00}', false).is_none());
            assert!(face.glyph('\n', false).is_none());
        }
    }

    #[test]
    fn glyphs_are_the_size_of_the_cell_and_draw_something() {
        let face = &FontSet::BUILTIN.faces()[0];
        let rows = face.glyph('A', false).unwrap();
        assert_eq!(rows.len(), 13);
        assert!(rows.iter().any(|row| *row != 0));
        // The space is empty, and a bold letter has more ink than a regular one.
        assert!(face.glyph(' ', false).unwrap().iter().all(|row| *row == 0));
        let ink = |rows: &[u8]| rows.iter().map(|row| row.count_ones()).sum::<u32>();
        assert!(ink(face.glyph('A', true).unwrap()) > ink(rows));
        let big = &FontSet::BUILTIN.faces()[1];
        assert_eq!(big.glyph('A', false).unwrap().len(), 18 * 2);
        assert_eq!((big.width(), big.height(), big.ascent()), (9, 18, 14));
    }

    #[test]
    fn the_nearest_face_wins() {
        let set = FontSet::BUILTIN;
        assert_eq!(set.face_for(8.0).height(), 13);
        assert_eq!(set.face_for(13.0).height(), 13);
        assert_eq!(set.face_for(15.0).height(), 13);
        assert_eq!(set.face_for(15.5).height(), 13, "a tie takes the smaller");
        assert_eq!(set.face_for(16.0).height(), 18);
        assert_eq!(set.face_for(40.0).height(), 18);
        assert_eq!(set.face_for(f32::NAN).height(), 13);
    }
}
