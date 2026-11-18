//! Text layout on a character grid, and the measurer that agrees with the
//! renderer about it. Both go through `place`, so a line that is measured as
//! one line is painted as one line.
use crate::font::{FontFace, FontSet};
use crate::math::FloatMath;
use alloc::vec::Vec;
use core::iter::Peekable;
use glassmere_scene::{
    CaretStop, Rect, Size, TextCluster, TextGeometry, TextLine, TextMeasurer, TextSpan, TextStyle,
    TextWrap,
};

/// A character that goes with the one before it: combining marks, joiners and
/// variation selectors. It takes no cell of its own and is not drawn.
fn is_extend(ch: char) -> bool {
    matches!(ch as u32,
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x200C | 0x200D
        | 0x20D0..=0x20FF | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F | 0xE0100..=0xE01EF)
}

/// What one cell of text holds: a character and the marks that follow it, or
/// a line break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cluster {
    pub start: usize,
    pub end: usize,
    /// The character that is drawn, `\n` for a line break.
    pub ch: char,
    pub newline: bool,
}

/// `(byte, char)` pairs grouped into clusters. This is a simpler rule than
/// grapheme segmentation, and the same as it for text in Latin, Greek and
/// Cyrillic with combining marks.
#[derive(Clone)]
pub(crate) struct Clusters<I: Iterator<Item = (usize, char)>> {
    chars: Peekable<I>,
}

impl<I: Iterator<Item = (usize, char)>> Clusters<I> {
    pub(crate) fn new(chars: I) -> Self {
        Self {
            chars: chars.peekable(),
        }
    }
}

impl<I: Iterator<Item = (usize, char)>> Iterator for Clusters<I> {
    type Item = Cluster;

    fn next(&mut self) -> Option<Cluster> {
        let (start, ch) = self.chars.next()?;
        let mut end = start + ch.len_utf8();
        let mut newline = ch == '\n';
        if ch == '\r'
            && let Some(&(at, '\n')) = self.chars.peek()
        {
            self.chars.next();
            end = at + 1;
            newline = true;
        }
        if !newline {
            while let Some(&(at, next)) = self.chars.peek()
                && is_extend(next)
            {
                end = at + next.len_utf8();
                self.chars.next();
            }
        }
        Some(Cluster {
            start,
            end,
            ch: if newline { '\n' } else { ch },
            newline,
        })
    }
}

/// One visual line, in bytes and cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LineSpan {
    pub start: usize,
    /// Without the line break that ends it. A wrapped line ends where the
    /// next one starts.
    pub end: usize,
    /// Cells, the spaces that hang off a wrapped line included.
    pub cols: u32,
    /// Cells up to the last character that is not a hanging space: what the
    /// line measures.
    pub shown: u32,
}

/// Breaks text into lines of at most `max_cols` cells: at line breaks, after
/// a space when a word would not fit, and inside a word that is longer than a
/// line. Spaces that end a wrapped line hang off its end rather than start
/// the next one.
pub(crate) fn break_lines(
    clusters: impl Iterator<Item = Cluster>,
    max_cols: u32,
    mut emit: impl FnMut(LineSpan),
) {
    let mut line_start = 0;
    let mut last_end = 0;
    let mut cols = 0u32;
    // Cells up to the last character that is not a space.
    let mut ink = 0u32;
    // Where the line can end: the byte after a run of spaces, the cells to
    // there, and the ink before it.
    let mut opening: Option<(usize, u32, u32)> = None;
    for cluster in clusters {
        if cluster.newline {
            emit(LineSpan {
                start: line_start,
                end: cluster.start,
                cols,
                shown: cols,
            });
            line_start = cluster.end;
            last_end = cluster.end;
            (cols, ink, opening) = (0, 0, None);
            continue;
        }
        last_end = cluster.end;
        if cluster.ch == ' ' {
            cols += 1;
            if ink > 0 {
                opening = Some((cluster.end, cols, ink));
            }
            continue;
        }
        if cols >= max_cols {
            if let Some((at, at_cols, at_ink)) = opening.take() {
                emit(LineSpan {
                    start: line_start,
                    end: at,
                    cols: at_cols,
                    shown: at_ink,
                });
                line_start = at;
                cols -= at_cols;
            } else if cols > 0 {
                emit(LineSpan {
                    start: line_start,
                    end: cluster.start,
                    cols,
                    shown: cols,
                });
                line_start = cluster.start;
                cols = 0;
            }
        }
        cols += 1;
        ink = cols;
    }
    emit(LineSpan {
        start: line_start,
        end: last_end,
        cols,
        shown: cols,
    });
}

/// A cluster and the cell it sits in.
pub(crate) struct Placed {
    pub line: usize,
    pub col: u32,
    pub cluster: Cluster,
}

/// Lays `chars` out and calls `glyph` for each cluster that takes a cell, in
/// text order. `lines` is left holding the lines. This is the one place text
/// is placed, for measuring and for painting alike.
pub(crate) fn place<I>(
    chars: I,
    max_cols: u32,
    lines: &mut Vec<LineSpan>,
    mut glyph: impl FnMut(Placed),
) where
    I: Iterator<Item = (usize, char)> + Clone,
{
    lines.clear();
    break_lines(Clusters::new(chars.clone()), max_cols, |line| {
        lines.push(line)
    });
    let (mut line, mut col) = (0, 0);
    for cluster in Clusters::new(chars) {
        while line + 1 < lines.len() && cluster.start >= lines[line + 1].start {
            line += 1;
            col = 0;
        }
        // What lies at the end of a line is the break itself.
        if cluster.start >= lines[line].end {
            continue;
        }
        glyph(Placed { line, col, cluster });
        col += 1;
    }
}

/// The cells a line may have: unlimited unless the style wraps.
pub(crate) fn max_cols(style: &TextStyle, width: Option<f32>, cell: u32) -> u32 {
    match (style.wrap, width) {
        (TextWrap::Word, Some(width)) if width.is_finite() => {
            (width / cell as f32 + 0.001).floorf().max(1.0) as u32
        }
        _ => u32::MAX,
    }
}

/// The characters of spans as if they were one string: byte offsets run
/// through all of them.
pub(crate) fn span_chars(spans: &[TextSpan]) -> impl Iterator<Item = (usize, char)> + Clone + '_ {
    let mut base = 0;
    spans.iter().flat_map(move |span| {
        let start = base;
        base += span.text.len();
        span.text
            .char_indices()
            .map(move |(at, ch)| (start + at, ch))
    })
}

/// Measures text in the fonts the renderer draws it in, on the same grid, so
/// that layout and pixels agree.
///
/// Every character takes one cell of the face nearest the style's font size,
/// and a line is as tall as a cell. Lines break at `\n` and, for
/// [`TextWrap::Word`], after spaces. Text is left to right, whatever the
/// style says; marks that combine with a character share its cell. The
/// family, weight and italic of a style and of spans do not change a
/// measurement.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmbeddedMeasurer {
    fonts: FontSet,
}

impl EmbeddedMeasurer {
    /// A measurer for `fonts`, which should be the renderer's.
    pub const fn new(fonts: FontSet) -> Self {
        Self { fonts }
    }

    fn geometry_of<I>(
        face: &FontFace,
        chars: I,
        len: usize,
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextGeometry
    where
        I: Iterator<Item = (usize, char)> + Clone,
    {
        let (cell, line_height) = (face.width() as f32, face.height() as f32);
        let mut lines = Vec::new();
        let mut clusters: Vec<TextCluster> = Vec::new();
        place(
            chars,
            max_cols(style, width, face.width()),
            &mut lines,
            |placed| {
                clusters.push(TextCluster {
                    bytes: placed.cluster.start..placed.cluster.end,
                    rect: Rect {
                        x: placed.col as f32 * cell,
                        y: placed.line as f32 * line_height,
                        width: cell,
                        height: line_height,
                    },
                    rtl: false,
                });
            },
        );
        let caret = |byte, x, y| CaretStop {
            byte,
            rect: Rect {
                x,
                y,
                width: 1.0,
                height: line_height,
            },
        };
        let mut carets = Vec::with_capacity(clusters.len() + 1);
        for (at, line) in lines.iter().enumerate() {
            if line.cols == 0 {
                carets.push(caret(line.start, 0.0, at as f32 * line_height));
            }
        }
        carets.extend(
            clusters
                .iter()
                .map(|cluster| caret(cluster.bytes.start, cluster.rect.x, cluster.rect.y)),
        );
        if let Some(last) = clusters.last()
            && last.bytes.end == len
        {
            carets.push(caret(len, last.rect.x + cell, last.rect.y));
        }
        carets.sort_by_key(|stop| stop.byte);
        let widest = lines.iter().map(|line| line.shown).max().unwrap_or(0);
        TextGeometry {
            size: Size::new(widest as f32 * cell, lines.len() as f32 * line_height),
            carets,
            clusters,
            lines: lines
                .iter()
                .enumerate()
                .map(|(at, line)| TextLine {
                    y: at as f32 * line_height,
                    height: line_height,
                    rtl: false,
                    bytes: line.start..line.end,
                })
                .collect(),
        }
    }
}

impl TextMeasurer for EmbeddedMeasurer {
    fn measure_styled(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> Size {
        let face = self.fonts.face_for(style.font_size);
        let (mut count, mut widest) = (0u32, 0u32);
        break_lines(
            Clusters::new(text.char_indices()),
            max_cols(style, width, face.width()),
            |line| {
                count += 1;
                widest = widest.max(line.shown);
            },
        );
        Size::new(
            (widest * face.width()) as f32,
            (count * face.height()) as f32,
        )
    }

    fn geometry(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> TextGeometry {
        let face = self.fonts.face_for(style.font_size);
        Self::geometry_of(face, text.char_indices(), text.len(), style, width)
    }

    fn geometry_rich(
        &mut self,
        spans: &[TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextGeometry {
        let face = self.fonts.face_for(style.font_size);
        let len = spans.iter().map(|span| span.text.len()).sum();
        Self::geometry_of(face, span_chars(spans), len, style, width)
    }

    fn ascent(&mut self, style: &TextStyle) -> f32 {
        self.fonts.face_for(style.font_size).ascent() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;
    use glassmere_scene::MonospaceMeasurer;

    fn wrapped() -> TextStyle {
        TextStyle {
            font_size: 13.0,
            wrap: TextWrap::Word,
            ..TextStyle::default()
        }
    }

    /// The lines of `text` at `cols` cells, as strings.
    fn lines(text: &str, cols: u32) -> Vec<String> {
        let mut found = Vec::new();
        break_lines(Clusters::new(text.char_indices()), cols, |line| {
            found.push(String::from(&text[line.start..line.end]));
        });
        found
    }

    #[test]
    fn words_wrap_at_spaces_and_long_words_break() {
        assert_eq!(lines("hello world foo", 11), ["hello world ", "foo"]);
        assert_eq!(lines("hello world foo", 5), ["hello ", "world ", "foo"]);
        assert_eq!(lines("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(lines("ab cdefghij", 4), ["ab ", "cdef", "ghij"]);
        assert_eq!(lines("abcd", 4), ["abcd"]);
        assert_eq!(lines("", 4), [""]);
        assert_eq!(lines("anything goes", u32::MAX), ["anything goes"]);
    }

    #[test]
    fn hard_breaks_keep_empty_lines_and_crlf_is_one_break() {
        assert_eq!(lines("a\n\nb", 9), ["a", "", "b"]);
        assert_eq!(lines("a\r\nb", 9), ["a", "b"]);
        assert_eq!(lines("a\n", 9), ["a", ""]);
        assert_eq!(lines("\nb", 9), ["", "b"]);
    }

    #[test]
    fn spaces_that_end_a_line_hang_instead_of_starting_the_next() {
        assert_eq!(lines("aaaa    bb", 4), ["aaaa    ", "bb"]);
        let mut shown = Vec::new();
        break_lines(Clusters::new("aaaa    bb".char_indices()), 4, |line| {
            shown.push((line.cols, line.shown));
        });
        assert_eq!(shown, [(8, 4), (2, 2)]);
        // Leading spaces of a line are text, and stay.
        assert_eq!(lines("  ab cd", 4), ["  ab ", "cd"]);
    }

    #[test]
    fn marks_share_a_cell_with_their_letter() {
        let text = "e\u{301}x";
        let clusters: Vec<_> = Clusters::new(text.char_indices()).collect();
        assert_eq!(clusters.len(), 2);
        assert_eq!((clusters[0].start, clusters[0].end), (0, 3));
        assert_eq!(clusters[0].ch, 'e');
        assert_eq!(lines(text, 9), [text]);
        let mut cols = 0;
        break_lines(Clusters::new(text.char_indices()), 9, |line| {
            cols = line.cols;
        });
        assert_eq!(cols, 2);
    }

    #[test]
    fn measure_and_geometry_agree_on_size() {
        let mut measurer = EmbeddedMeasurer::default();
        for (text, width) in [
            ("hello world foo bar", Some(66.0)),
            ("a\n\nlonger line\nz", None),
            ("", Some(30.0)),
            ("ďalší riadok s diakritikou", Some(100.0)),
        ] {
            for wrap in [TextWrap::None, TextWrap::Word] {
                for font_size in [12.0, 16.0] {
                    let style = TextStyle {
                        font_size,
                        wrap,
                        ..TextStyle::default()
                    };
                    let size = measurer.measure_styled(text, &style, width);
                    let geometry = measurer.geometry(text, &style, width);
                    assert_eq!(size, geometry.size, "{text:?} {wrap:?} {font_size}");
                }
            }
        }
    }

    #[test]
    fn geometry_has_the_structure_the_monospace_measurer_gives() {
        let mut ours = EmbeddedMeasurer::default();
        let mut reference = MonospaceMeasurer;
        let style = TextStyle {
            font_size: 13.0,
            ..TextStyle::default()
        };
        for text in [
            "",
            "abc",
            "ab\ncd",
            "a\n\nb",
            "x\r\ny",
            "trailing\n",
            "žlť ľud",
        ] {
            let (a, b) = (
                ours.geometry(text, &style, None),
                reference.geometry(text, &style, None),
            );
            let bytes = |g: &TextGeometry| {
                (
                    g.clusters
                        .iter()
                        .map(|c| c.bytes.clone())
                        .collect::<Vec<_>>(),
                    g.carets.iter().map(|c| c.byte).collect::<Vec<_>>(),
                    g.lines.iter().map(|l| l.bytes.clone()).collect::<Vec<_>>(),
                )
            };
            assert_eq!(bytes(&a), bytes(&b), "{text:?}");
        }
    }

    #[test]
    fn cells_are_the_font_cell_and_the_ascent_is_the_fonts() {
        let mut measurer = EmbeddedMeasurer::default();
        let small = TextStyle {
            font_size: 13.0,
            ..TextStyle::default()
        };
        let big = TextStyle::default();
        let geometry = measurer.geometry("ab\ncd", &small, None);
        assert_eq!(geometry.size, Size::new(12.0, 26.0));
        // The line break takes no cell: the clusters are a, b, c and d.
        assert_eq!(geometry.clusters.len(), 4);
        assert_eq!(geometry.clusters[3].rect.x, 6.0);
        assert_eq!(geometry.clusters[3].rect.y, 13.0);
        assert_eq!(geometry.caret(5).x, 12.0);
        assert_eq!(measurer.measure("ab", 16.0), Size::new(18.0, 18.0));
        assert_eq!(measurer.ascent(&small), 11.0);
        assert_eq!(measurer.ascent(&big), 14.0);
    }

    #[test]
    fn rich_text_is_measured_as_its_concatenation() {
        let mut measurer = EmbeddedMeasurer::default();
        let style = wrapped();
        let spans = vec![
            TextSpan::new("hello "),
            TextSpan::new("bold ").weight(700),
            TextSpan::new("world"),
        ];
        let rich = measurer.geometry_rich(&spans, &style, Some(60.0));
        let plain = measurer.geometry("hello bold world", &style, Some(60.0));
        assert_eq!(rich, plain);
        assert_eq!(rich.lines.len(), 2);
    }

    #[test]
    fn a_wrapped_text_reports_the_lines_it_breaks_into() {
        let mut measurer = EmbeddedMeasurer::default();
        let text = "one two three";
        let geometry = measurer.geometry(text, &wrapped(), Some(48.0));
        let shown: Vec<_> = geometry
            .lines
            .iter()
            .map(|line| &text[line.bytes.clone()])
            .collect();
        assert_eq!(shown, ["one two ", "three"]);
        assert_eq!(geometry.size.width, 7.0 * 6.0);
        assert_eq!(geometry.size.height, 26.0);
    }
}
