//! Fonts, shaping and measuring, shared by every renderer so that layout and
//! pixels agree whichever one draws. Built on cosmic-text.
pub use cosmic_text;
use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, LineEnding, LineIter, Metrics, Shaping, Wrap,
};
use glassmere_scene::{
    CaretStop, FontFamily, Rect, Size, TextAlign, TextCluster, TextGeometry, TextLine,
    TextMeasurer, TextSpan, TextStyle, TextWrap,
};
use std::collections::hash_map::RandomState;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::OnceLock;
use unicode_segmentation::UnicodeSegmentation;

/// Identifies one shaped block: its text and everything that shapes it.
///
/// A key feeds a hasher one word, computed when the key is built, so a lookup
/// does not hash the text again. Only `TextSystem::key` builds one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextKey {
    text: Hashed,
    size: Skip<u32>,
    family: Skip<FontFamily>,
    weight: Skip<u16>,
    italic: Skip<bool>,
    wrap: Skip<TextWrap>,
    rtl: Skip<bool>,
    align: Skip<TextAlign>,
    base: Skip<Option<bool>>,
    width: Skip<Option<u32>>,
    /// Byte length and overrides of each run of a rich block; empty for plain text.
    spans: Skip<Vec<SpanKey>>,
}

/// The text of a key with the hash of the whole key.
#[derive(Clone)]
struct Hashed {
    hash: u64,
    text: String,
}

impl Hash for Hashed {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

// Equal keys have equal fields and so equal hashes; the hash only lets
// unequal keys differ early, the fields decide.
impl PartialEq for Hashed {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.text == other.text
    }
}

impl Eq for Hashed {}

impl fmt::Debug for Hashed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.text, f)
    }
}

/// A field that is compared but is already part of the key's cached hash.
#[derive(Clone)]
struct Skip<T>(T);

impl<T> Hash for Skip<T> {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}

impl<T: PartialEq> PartialEq for Skip<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Eq> Eq for Skip<T> {}

impl<T: fmt::Debug> fmt::Debug for Skip<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

/// The seed and odd multiplier of the key hash, drawn once per process so
/// that every text system and thread hashes a key alike and the hash cannot
/// be predicted by whoever supplies the text. They must stay random and
/// secret: with a constant seed or an even multiplier, text can be crafted
/// that collides in every bucket of a map.
fn seed() -> (u64, u64) {
    static SEED: OnceLock<(u64, u64)> = OnceLock::new();
    *SEED.get_or_init(|| {
        let random = RandomState::new();
        (
            random.hash_one(0x6b65_795f_7365_6564_u64),
            random.hash_one(0x6b65_795f_6d75_6c74_u64) | 1,
        )
    })
}

/// Hashes a key's fields a word at a time, folding each product of the
/// running state and a word back on itself.
struct Fold {
    state: u64,
    mul: u64,
}

impl Fold {
    fn new() -> Self {
        let (state, mul) = seed();
        Self { state, mul }
    }

    fn add(&mut self, word: u64) {
        let product = u128::from(self.state ^ word) * u128::from(self.mul);
        self.state = (product as u64) ^ ((product >> 64) as u64);
    }
}

impl Hasher for Fold {
    fn write(&mut self, bytes: &[u8]) {
        let (words, tail) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        if !tail.is_empty() {
            let mut word = [0; 8];
            word[..tail.len()].copy_from_slice(tail);
            self.add(u64::from_le_bytes(word));
        }
        // Without the length, "a" and "a\0" would give the same words.
        self.add(bytes.len() as u64);
    }

    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.add(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

type SpanKey = (
    usize,
    Option<[u8; 4]>,
    Option<u16>,
    Option<bool>,
    Option<FontFamily>,
);

fn family(family: &FontFamily) -> Family<'_> {
    match family {
        FontFamily::SansSerif => Family::SansSerif,
        FontFamily::Serif => Family::Serif,
        FontFamily::Monospace => Family::Monospace,
        FontFamily::Named(name) => Family::Name(name),
    }
}

fn attrs(font: &FontFamily, weight: u16, italic: bool) -> Attrs<'_> {
    Attrs::new()
        .family(family(font))
        .weight(cosmic_text::Weight(weight.clamp(100, 900)))
        .style(if italic {
            cosmic_text::Style::Italic
        } else {
            cosmic_text::Style::Normal
        })
}

/// Bytes of the mark that starts each line of a text whose direction is
/// forced. The left-to-right and right-to-left marks are three bytes each.
const MARK: usize = '\u{200f}'.len_utf8();

/// A strong character of the forced direction, without width. It is the
/// first strong character of its line, so the bidi algorithm takes the
/// paragraph direction from it.
fn mark(rtl: bool) -> &'static str {
    if rtl { "\u{200f}" } else { "\u{200e}" }
}

/// Where each line of `text` starts, as cosmic-text splits it: after each
/// line ending, so a text that ends with one has an empty last line.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut ended = true;
    for (range, ending) in LineIter::new(text) {
        starts.push(range.start);
        ended = ending != LineEnding::None;
    }
    if ended {
        starts.push(text.len());
    }
    starts
}

/// System fonts and shared shaping buffers for measurement, painting and editing.
pub struct TextSystem {
    pub fonts: FontSystem,
    /// Shaped blocks, kept between frames; see `ensure` and `retain`.
    pub buffers: HashMap<TextKey, Buffer>,
}

impl Default for TextSystem {
    fn default() -> Self {
        let mut fonts = FontSystem::new();
        // cosmic-text's generic defaults name Linux fonts that may be absent.
        // Select installed native families before shaping any cached buffers.
        let db = fonts.db_mut();
        let choose = |names: &[&str]| {
            names
                .iter()
                .find(|name| {
                    db.faces()
                        .any(|face| face.families.iter().any(|(family, _)| family == **name))
                })
                .map(|name| (*name).to_owned())
        };
        let serif = choose(&[
            "Times New Roman",
            "Times",
            "Noto Serif",
            "DejaVu Serif",
            "Liberation Serif",
        ]);
        let sans = choose(&[
            "Segoe UI",
            "Helvetica Neue",
            "Noto Sans",
            "DejaVu Sans",
            "Liberation Sans",
        ]);
        let mono = choose(&[
            "Consolas",
            "Menlo",
            "Noto Sans Mono",
            "DejaVu Sans Mono",
            "Liberation Mono",
        ]);
        if let Some(family) = serif {
            db.set_serif_family(family);
        }
        if let Some(family) = sans {
            db.set_sans_serif_family(family);
        }
        if let Some(family) = mono {
            db.set_monospace_family(family);
        }
        Self {
            fonts,
            buffers: HashMap::new(),
        }
    }
}

impl TextSystem {
    /// Shaping over exactly these font files and no system fonts; every generic
    /// family resolves to the first font. Intended for deterministic snapshots.
    pub fn with_fonts(fonts: impl IntoIterator<Item = Vec<u8>>) -> Self {
        let mut db = cosmic_text::fontdb::Database::new();
        for font in fonts {
            db.load_font_data(font);
        }
        let family = db
            .faces()
            .next()
            .and_then(|face| face.families.first())
            .map(|(name, _)| name.clone())
            .expect("with_fonts requires at least one parseable font");
        db.set_sans_serif_family(family.clone());
        db.set_serif_family(family.clone());
        db.set_monospace_family(family);
        Self {
            fonts: FontSystem::new_with_locale_and_db("en-US".into(), db),
            buffers: HashMap::new(),
        }
    }

    /// Whether a named family is installed. A style that asks for one that
    /// is not gets the default font instead, silently, which is what makes
    /// it worth reporting.
    pub fn has_family(&self, name: &str) -> bool {
        self.fonts.db().faces().any(|face| {
            face.families
                .iter()
                .any(|(family, _)| family.eq_ignore_ascii_case(name))
        })
    }

    pub fn ensure(&mut self, text: &str, font_size: f32) -> TextKey {
        self.ensure_styled(
            text,
            &TextStyle {
                font_size,
                ..TextStyle::default()
            },
            None,
        )
    }

    pub fn ensure_styled(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> TextKey {
        self.ensure_buffer(text, &[], style, width)
    }

    /// A rich block: per-span family, weight, slant and colour over the block style.
    pub fn ensure_rich(
        &mut self,
        spans: &[TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextKey {
        let text: String = spans.iter().map(|span| span.text.as_str()).collect();
        self.ensure_buffer(&text, spans, style, width)
    }

    /// The cache key of a block, with the font size and width it is shaped at.
    ///
    /// Every field of the key is folded into its hash from the same value
    /// that is stored, so keys that are equal have equal hashes. A field added
    /// to `TextKey` is folded in here and wrapped in `Skip`.
    fn key(
        text: &str,
        spans: &[TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> (TextKey, f32, Option<f32>) {
        let font_size = if style.font_size.is_finite() {
            style.font_size.max(1.0)
        } else {
            16.0
        };
        let width = if style.wrap == TextWrap::None {
            None
        } else {
            width
                .filter(|width| width.is_finite())
                .map(|width| width.max(1.0))
        };
        let size = font_size.to_bits();
        let weight = style.weight.clamp(100, 900);
        // Alignment needs a width, which only wrapped blocks have.
        let rtl = style.rtl && width.is_some();
        let align = if width.is_some() {
            style.align
        } else {
            TextAlign::Start
        };
        let width_bits = width.map(f32::to_bits);
        // Most blocks are plain, and there is nothing to collect for them.
        let spans: Vec<SpanKey> = if spans.is_empty() {
            Vec::new()
        } else {
            spans
                .iter()
                .map(|span| {
                    (
                        span.text.len(),
                        span.color.map(|color| [color.0, color.1, color.2, color.3]),
                        span.weight,
                        span.italic,
                        span.family.clone(),
                    )
                })
                .collect()
        };
        let mut fold = Fold::new();
        (
            size,
            &style.family,
            weight,
            style.italic,
            style.wrap,
            rtl,
            align,
            style.base,
            width_bits,
            &spans,
        )
            .hash(&mut fold);
        text.hash(&mut fold);
        let key = TextKey {
            text: Hashed {
                hash: fold.finish(),
                text: text.into(),
            },
            size: Skip(size),
            family: Skip(style.family.clone()),
            weight: Skip(weight),
            italic: Skip(style.italic),
            wrap: Skip(style.wrap),
            rtl: Skip(rtl),
            align: Skip(align),
            base: Skip(style.base),
            width: Skip(width_bits),
            spans: Skip(spans),
        };
        (key, font_size, width)
    }

    fn shape(
        fonts: &mut FontSystem,
        text: &str,
        spans: &[TextSpan],
        style: &TextStyle,
        font_size: f32,
        width: Option<f32>,
    ) -> Buffer {
        let metrics = Metrics::new(font_size, font_size * 1.4);
        // Buffer::new shapes and lays out an empty line that set_text then
        // drops. set_rich_text reuses that line instead, so a rich block
        // starts from it.
        let mut buffer = if spans.is_empty() {
            Buffer::new_empty(metrics)
        } else {
            Buffer::new(fonts, metrics)
        };
        buffer.set_wrap(
            fonts,
            if style.wrap == TextWrap::None {
                Wrap::None
            } else {
                Wrap::WordOrGlyph
            },
        );
        buffer.set_size(fonts, width, None);
        let base = attrs(&style.family, style.weight, style.italic);
        // A forced paragraph direction is a mark that starts each line.
        let forced = style.base.map(|rtl| (mark(rtl), line_starts(text)));
        if spans.is_empty() {
            let marked = forced.as_ref().map(|(mark, starts)| {
                let mut marked = String::with_capacity(text.len() + MARK * starts.len());
                let mut at = 0;
                for &start in starts {
                    marked.push_str(&text[at..start]);
                    marked.push_str(mark);
                    at = start;
                }
                marked.push_str(&text[at..]);
                marked
            });
            let text = marked.as_deref().unwrap_or(text);
            buffer.set_text(fonts, text, &base, Shaping::Advanced, None);
        } else {
            // A span colour overrides the text area's default colour per glyph.
            // A text that ends with a line ending has no empty last line here.
            let mut starts = forced
                .as_ref()
                .map_or(&[][..], |(_, starts)| starts)
                .iter()
                .filter(|&&start| start < text.len() || text.is_empty())
                .peekable();
            let mark = forced.as_ref().map_or("", |(mark, _)| mark);
            let mut runs = Vec::new();
            let mut at = 0;
            for span in spans {
                let run = attrs(
                    span.family.as_ref().unwrap_or(&style.family),
                    span.weight.unwrap_or(style.weight),
                    span.italic.unwrap_or(style.italic),
                );
                let run = match span.color {
                    Some(color) => {
                        run.color(cosmic_text::Color::rgba(color.0, color.1, color.2, color.3))
                    }
                    None => run,
                };
                // The mark of a line goes before its first run, which may
                // begin inside a span.
                let mut rest = span.text.as_str();
                loop {
                    while starts.next_if_eq(&&at).is_some() {
                        runs.push((mark, base.clone()));
                    }
                    match starts.peek() {
                        Some(&&next) if next < at + rest.len() => {
                            let (head, tail) = rest.split_at(next - at);
                            runs.push((head, run.clone()));
                            (at, rest) = (next, tail);
                        }
                        _ => break,
                    }
                }
                runs.push((rest, run));
                at += rest.len();
            }
            while starts.next_if_eq(&&at).is_some() {
                runs.push((mark, base.clone()));
            }
            // cosmic-text shapes an ASCII word in one run when every explicit
            // span overlapping it is compatible, and text with the default
            // attributes is not an explicit span: "bold:" would be all bold.
            // Defaults that no run uses make every run explicit.
            let unused = base
                .clone()
                .weight(cosmic_text::Weight(style.weight.clamp(100, 900) ^ 1));
            buffer.set_rich_text(fonts, runs, &unused, Shaping::Advanced, None);
        }
        if width.is_some() {
            let align = match style.align {
                TextAlign::Center => Some(cosmic_text::Align::Center),
                // The end of right-to-left UI is the left edge, whatever the
                // text in it says.
                TextAlign::End if style.rtl => Some(cosmic_text::Align::Left),
                TextAlign::End => Some(cosmic_text::Align::End),
                TextAlign::Justify => Some(cosmic_text::Align::Justified),
                // Right-to-left UI starts at the right edge.
                TextAlign::Start if style.rtl => Some(cosmic_text::Align::Right),
                TextAlign::Start => None,
            };
            if align.is_some() {
                for line in &mut buffer.lines {
                    line.set_align(align);
                }
            }
        }
        buffer.shape_until_scroll(fonts, false);
        buffer
    }

    fn buffer(
        &mut self,
        text: &str,
        spans: &[TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> &Buffer {
        let (key, font_size, width) = Self::key(text, spans, style, width);
        self.buffers
            .entry(key)
            .or_insert_with(|| Self::shape(&mut self.fonts, text, spans, style, font_size, width))
    }

    fn ensure_buffer(
        &mut self,
        text: &str,
        spans: &[TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextKey {
        let (key, font_size, width) = Self::key(text, spans, style, width);
        if self.buffers.contains_key(&key) {
            return key;
        }
        let buffer = Self::shape(&mut self.fonts, text, spans, style, font_size, width);
        self.buffers.insert(key.clone(), buffer);
        key
    }

    pub fn retain(&mut self, active: &HashSet<TextKey>) {
        self.buffers.retain(|key, _| active.contains(key));
    }

    fn size(buffer: &Buffer) -> Size {
        let mut size = Size::new(0.0, buffer.metrics().line_height);
        for run in buffer.layout_runs() {
            size.width = size.width.max(run.line_w);
            size.height = size.height.max(run.line_top + run.line_height);
        }
        size
    }
}

impl TextMeasurer for TextSystem {
    fn measure_styled(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> Size {
        Self::size(self.buffer(text, &[], style, width))
    }

    fn geometry(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> TextGeometry {
        Self::extract(
            self.buffer(text, &[], style, width),
            text,
            style.base.is_some(),
        )
    }

    /// The font's own ascent for this size, from a line shaped with it.
    fn ascent(&mut self, style: &TextStyle) -> f32 {
        self.buffer("M", &[], style, None)
            .layout_runs()
            .next()
            .map_or(style.font_size * 0.8, |run| run.line_y)
    }

    fn geometry_rich(
        &mut self,
        spans: &[TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextGeometry {
        let text: String = spans.iter().map(|span| span.text.as_str()).collect();
        Self::extract(
            self.buffer(&text, spans, style, width),
            &text,
            style.base.is_some(),
        )
    }
}

impl TextSystem {
    /// Grapheme clusters and caret stops of a shaped buffer over `text`. A
    /// buffer shaped with a forced direction has a mark at the start of each
    /// line that `text` does not; offsets are those of `text`.
    fn extract(buffer: &Buffer, text: &str, forced: bool) -> TextGeometry {
        let mut result = TextGeometry {
            size: Self::size(buffer),
            ..TextGeometry::default()
        };
        let bases = line_starts(text);
        for run in buffer.layout_runs() {
            let base = bases.get(run.line_i).copied().unwrap_or(0);
            let lead = if forced && run.line_i < bases.len() {
                MARK
            } else {
                0
            };
            let start = run
                .glyphs
                .iter()
                .map(|glyph| glyph.start)
                .min()
                .unwrap_or(0)
                .saturating_sub(lead);
            let end = run
                .glyphs
                .iter()
                .map(|glyph| glyph.end)
                .max()
                .unwrap_or(0)
                .saturating_sub(lead);
            result.lines.push(TextLine {
                y: run.line_top,
                height: run.line_height,
                rtl: run.rtl,
                bytes: base + start..base + end,
            });
            for (inner, grapheme) in run.text[lead..].grapheme_indices(true) {
                let byte = lead + inner;
                let Some(glyph) = run
                    .glyphs
                    .iter()
                    .find(|glyph| glyph.start <= byte && byte < glyph.end)
                else {
                    continue;
                };
                let mut left = glyph.x;
                let mut right = glyph.x + glyph.w;
                for part in run
                    .glyphs
                    .iter()
                    .filter(|part| part.start == glyph.start && part.end == glyph.end)
                {
                    left = left.min(part.x);
                    right = right.max(part.x + part.w);
                }
                let first = glyph.start.max(lead);
                let cluster_text = &run.text[first..glyph.end];
                let count = cluster_text.graphemes(true).count().max(1);
                let offset = run.text[first..byte].graphemes(true).count();
                let advance = (right - left) / count as f32;
                let rtl = glyph.level.is_rtl();
                let x = if rtl {
                    right - advance * (offset + 1) as f32
                } else {
                    left + advance * offset as f32
                };
                let rect = Rect {
                    x,
                    y: run.line_top,
                    width: advance,
                    height: run.line_height,
                };
                result.clusters.push(TextCluster {
                    bytes: base + inner..base + inner + grapheme.len(),
                    rect,
                    rtl,
                });
                result.carets.push(CaretStop {
                    byte: base + inner,
                    rect: Rect {
                        x: if rtl { x + advance } else { x },
                        width: 1.0,
                        ..rect
                    },
                });
            }
            if run.glyphs.is_empty() || run.text.len() == lead {
                result.carets.push(CaretStop {
                    byte: base,
                    rect: Rect {
                        x: 0.0,
                        y: run.line_top,
                        width: 1.0,
                        height: run.line_height,
                    },
                });
            }
        }
        result.clusters.sort_by_key(|cluster| cluster.bytes.start);
        result.carets.sort_by_key(|caret| caret.byte);
        if let Some(last) = result.clusters.last() {
            result.carets.push(CaretStop {
                byte: text.len(),
                rect: Rect {
                    x: if last.rtl {
                        last.rect.x
                    } else {
                        last.rect.x + last.rect.width
                    },
                    width: 1.0,
                    ..last.rect
                },
            });
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glassmere_scene::{Affinity, Point};
    use std::ops::Range;

    #[test]
    fn shaped_bidirectional_text_steps_monotonically_and_reports_lines() {
        let mut system = TextSystem::default();
        let style = TextStyle::default();
        let text = "abc \u{5d0}\u{5d1}\u{5d2} def";
        let geometry = system.geometry(text, &style, None);
        if !geometry.clusters.iter().any(|cluster| cluster.rtl) {
            // No installed font shaped the Hebrew letters right to left.
            return;
        }
        assert_eq!(geometry.lines.len(), 1);
        assert!(!geometry.lines[0].rtl);
        assert_eq!(geometry.lines[0].bytes, 0..text.len());
        let mut at = (0, Affinity::Downstream);
        let mut xs = vec![geometry.caret_at(at.0, at.1).x];
        while let Some(next) = geometry.visual_step(at.0, at.1, true) {
            at = next;
            xs.push(geometry.caret_at(at.0, at.1).x);
        }
        // Eleven characters: twelve caret places, strictly left to right.
        assert_eq!(xs.len(), 12);
        assert!(xs.windows(2).all(|pair| pair[1] > pair[0]), "{xs:?}");
        assert_eq!(at.0, text.len());
        // The boundary offset has two places.
        let upstream = geometry.caret_at(4, Affinity::Upstream).x;
        let downstream = geometry.caret_at(4, Affinity::Downstream).x;
        assert!(downstream > upstream + 1.0);

        let wrapped = system.geometry(
            "one two three four five six seven eight nine ten",
            &TextStyle {
                wrap: TextWrap::Word,
                ..TextStyle::default()
            },
            Some(120.0),
        );
        assert!(wrapped.lines.len() > 2);
        let end = wrapped.lines[0].bytes.end;
        assert_eq!(
            wrapped.line_edge(1, Affinity::Downstream, true),
            (end, Affinity::Upstream)
        );
        assert_eq!(
            wrapped.caret_at(end, Affinity::Upstream).y,
            wrapped.lines[0].y
        );
    }

    #[test]
    fn rich_geometry_indexes_the_concatenated_spans_and_matches_plain_shaping() {
        let mut system = TextSystem::default();
        let style = TextStyle::default();
        let plain = system.geometry("abcd", &style, None);
        let ranges = |geometry: &TextGeometry| -> Vec<_> {
            geometry
                .clusters
                .iter()
                .map(|cluster| cluster.bytes.clone())
                .collect()
        };
        let rich = system.geometry_rich(&[TextSpan::new("ab"), TextSpan::new("cd")], &style, None);
        assert_eq!(ranges(&rich), [0..1, 1..2, 2..3, 3..4]);
        assert_eq!(rich.size, plain.size);
        assert_eq!(rich.carets.last().map(|caret| caret.byte), Some(4));
        let bold = system.geometry_rich(
            &[TextSpan::new("ab").weight(700), TextSpan::new("c\nd")],
            &style,
            None,
        );
        assert_eq!(ranges(&bold), [0..1, 1..2, 2..3, 4..5]);
        assert!(bold.size.height > plain.size.height);
        // Span attributes are part of the cache key; plain text keeps its own entry.
        let red = [TextSpan::new("abcd").color(glassmere_scene::Color(255, 0, 0, 255))];
        assert_ne!(
            system.ensure_rich(&red, &style, None),
            system.ensure_styled("abcd", &style, None)
        );
    }

    #[test]
    fn shaping_wraps_and_geometry_hits_only_grapheme_boundaries() {
        let mut system = TextSystem::default();
        let style = TextStyle {
            family: FontFamily::Serif,
            weight: 700,
            italic: true,
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let paragraph = "Text wrapping uses the same shaped buffer for layout and painting.";
        let narrow = system.measure_styled(paragraph, &style, Some(120.0));
        let wide = system.measure_styled(paragraph, &style, Some(360.0));
        assert!(narrow.height > wide.height && narrow.width <= 121.0);
        for text in ["office a👩‍💻e\u{301}", "مرحبا بالعالم", "日本語"] {
            let geometry = system.geometry(text, &TextStyle::default(), None);
            let boundaries: Vec<_> = text
                .grapheme_indices(true)
                .map(|(byte, _)| byte)
                .chain(std::iter::once(text.len()))
                .collect();
            assert!(geometry.size.width > 0.0 && !geometry.clusters.is_empty());
            assert!(
                geometry
                    .carets
                    .iter()
                    .all(|caret| boundaries.contains(&caret.byte))
            );
            for x in 0..geometry.size.width.ceil() as usize {
                assert!(boundaries.contains(&geometry.hit(Point {
                    x: x as f32,
                    y: 8.0
                })));
            }
            let selection = geometry.selection(0..text.len());
            assert!(
                selection
                    .iter()
                    .all(|rect| rect.width >= 0.0 && rect.height > 0.0)
            );
        }
    }

    /// Latin, Hebrew, Latin; the Hebrew run reads right to left.
    const MIXED: &str = "abc \u{5d0}\u{5d1}\u{5d2} def";

    fn forced(rtl: bool) -> TextStyle {
        TextStyle {
            base: Some(rtl),
            ..TextStyle::default()
        }
    }

    fn bytes(geometry: &TextGeometry) -> Vec<Range<usize>> {
        geometry
            .clusters
            .iter()
            .map(|cluster| cluster.bytes.clone())
            .collect()
    }

    #[test]
    fn a_forced_direction_orders_the_line_and_leaves_the_offsets_alone() {
        let mut system = TextSystem::default();
        let plain = system.geometry(MIXED, &TextStyle::default(), None);
        if !plain.clusters.iter().any(|cluster| cluster.rtl) {
            // No installed font shaped the Hebrew letters right to left.
            return;
        }
        assert!(!plain.lines[0].rtl);
        let rich = [
            TextSpan::new("abc ").color(glassmere_scene::Color(255, 0, 0, 255)),
            TextSpan::new(&MIXED[4..]),
        ];
        for geometry in [
            system.geometry(MIXED, &forced(true), None),
            system.geometry_rich(&rich, &forced(true), None),
        ] {
            // The marks that force the direction are not in the offsets.
            assert_eq!(bytes(&geometry), bytes(&plain));
            assert_eq!(
                geometry
                    .carets
                    .iter()
                    .map(|caret| caret.byte)
                    .collect::<Vec<_>>(),
                plain
                    .carets
                    .iter()
                    .map(|caret| caret.byte)
                    .collect::<Vec<_>>()
            );
            assert_eq!(geometry.lines.len(), 1);
            assert_eq!(geometry.lines[0].bytes, 0..MIXED.len());
            assert!(geometry.lines[0].rtl);
            assert!((geometry.size.width - plain.size.width).abs() < 0.5);
            // The first logical run is the rightmost one.
            assert!(geometry.clusters[0].rect.x > geometry.clusters[4].rect.x);
            assert!(plain.clusters[0].rect.x < plain.clusters[4].rect.x);
            let start = geometry.caret_at(0, Affinity::Downstream).x;
            let end = geometry.caret_at(MIXED.len(), Affinity::Downstream).x;
            assert!(start > end + 10.0, "{start} {end}");
        }
        // Left to right over a line that starts with Hebrew.
        let hebrew = "\u{5d0}\u{5d1}\u{5d2} abc";
        let free = system.geometry(hebrew, &TextStyle::default(), None);
        let ltr = system.geometry(hebrew, &forced(false), None);
        assert!(free.lines[0].rtl && !ltr.lines[0].rtl);
        assert_eq!(bytes(&ltr), bytes(&free));
        assert_eq!(ltr.lines[0].bytes, 0..hebrew.len());
        // Forced and free text do not share a shaped buffer.
        assert_ne!(
            system.ensure_styled(MIXED, &forced(true), None),
            system.ensure_styled(MIXED, &TextStyle::default(), None)
        );
    }

    #[test]
    fn every_line_of_a_forced_text_takes_the_direction() {
        let mut system = TextSystem::default();
        for text in ["ab\ncd", "ab\n\ncd", "ab\r\ncd", "ab\n", ""] {
            let free = system.geometry(text, &TextStyle::default(), None);
            let geometry = system.geometry(text, &forced(true), None);
            assert_eq!(bytes(&geometry), bytes(&free), "{text:?}");
            assert_eq!(
                geometry
                    .carets
                    .iter()
                    .map(|caret| caret.byte)
                    .collect::<Vec<_>>(),
                free.carets
                    .iter()
                    .map(|caret| caret.byte)
                    .collect::<Vec<_>>(),
                "{text:?}"
            );
            assert_eq!(geometry.lines.len(), free.lines.len(), "{text:?}");
            for (line, plain) in geometry.lines.iter().zip(&free.lines) {
                assert!(line.rtl && !plain.rtl, "{text:?}");
                assert_eq!(line.bytes, plain.bytes, "{text:?}");
            }
        }
        let spans = [TextSpan::new("ab\nc").weight(700), TextSpan::new("d")];
        let free = system.geometry_rich(&spans, &TextStyle::default(), None);
        let geometry = system.geometry_rich(&spans, &forced(true), None);
        assert_eq!(bytes(&geometry), bytes(&free));
        assert_eq!(geometry.lines.len(), 2);
        assert!(geometry.lines.iter().all(|line| line.rtl));
        assert_eq!(geometry.lines[1].bytes, 3..5);
        // A wrapped line covers the same bytes.
        let wrapped = TextStyle {
            wrap: TextWrap::Word,
            ..forced(true)
        };
        let text = "one two three four five six seven eight nine ten";
        let geometry = system.geometry(text, &wrapped, Some(120.0));
        assert!(geometry.lines.len() > 2);
        assert_eq!(geometry.lines[0].bytes.start, 0);
        assert_eq!(geometry.lines.last().unwrap().bytes.end, text.len());
        assert_eq!(bytes(&geometry).last(), Some(&(text.len() - 1..text.len())));
    }

    #[test]
    fn generic_serif_resolves_to_an_installed_face_instead_of_sans_fallback() {
        let mut system = TextSystem::default();
        let expected = system
            .fonts
            .db()
            .query(&cosmic_text::fontdb::Query {
                families: &[Family::Serif],
                ..Default::default()
            })
            .expect("install system serif fonts to run shaping tests");
        let key = system.ensure_styled(
            "Serif",
            &TextStyle {
                family: FontFamily::Serif,
                ..TextStyle::default()
            },
            None,
        );
        let buffer = &system.buffers[&key];
        assert!(
            buffer
                .layout_runs()
                .flat_map(|run| run.glyphs)
                .all(|glyph| glyph.font_id == expected)
        );
    }

    fn cases() -> Vec<(&'static str, TextStyle, Option<f32>)> {
        let wrapped = TextStyle {
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let style = |font_size| TextStyle {
            font_size,
            ..wrapped.clone()
        };
        vec![
            ("alpha beta gamma delta epsilon", TextStyle::default(), None),
            (
                "alpha beta gamma delta epsilon",
                wrapped.clone(),
                Some(120.0),
            ),
            (
                "alpha beta gamma delta epsilon",
                TextStyle::default(),
                Some(50.0),
            ),
            (
                "alpha beta gamma delta epsilon",
                wrapped.clone(),
                Some(f32::NAN),
            ),
            (
                "alpha beta gamma delta epsilon",
                wrapped.clone(),
                Some(f32::INFINITY),
            ),
            (
                "alpha beta gamma delta epsilon",
                wrapped.clone(),
                Some(0.25),
            ),
            (
                "alpha beta gamma delta epsilon",
                style(f32::NAN),
                Some(90.0),
            ),
            ("alpha beta gamma delta epsilon", style(0.2), Some(90.0)),
            ("abc \u{5d0}\u{5d1}\u{5d2}\ndef", forced(true), None),
            (
                "abc \u{5d0}\u{5d1}\u{5d2} def",
                TextStyle {
                    rtl: true,
                    base: Some(false),
                    ..wrapped
                },
                Some(80.0),
            ),
        ]
    }

    #[test]
    fn measuring_and_ensuring_share_one_buffer() {
        for (text, style, width) in cases() {
            let mut system = TextSystem::default();
            let measured = system.measure_styled(text, &style, width);
            assert_eq!(system.buffers.len(), 1);
            let key = system.ensure_styled(text, &style, width);
            assert!(system.buffers.contains_key(&key));
            assert_eq!(system.buffers.len(), 1);
            system.geometry(text, &style, width);
            assert_eq!(system.buffers.len(), 1);
            // The buffer the measure path shaped is the one the ensure path
            // would have shaped.
            let mut fresh = TextSystem::default();
            let fresh_key = fresh.ensure_styled(text, &style, width);
            assert_eq!(fresh_key, key);
            assert_eq!(TextSystem::size(&fresh.buffers[&fresh_key]), measured);
            assert_eq!(
                system.buffers[&key].metrics().font_size,
                fresh.buffers[&fresh_key].metrics().font_size
            );
            assert_eq!(
                system.buffers[&key].size(),
                fresh.buffers[&fresh_key].size()
            );

            let mut system = TextSystem::default();
            let key = system.ensure_styled(text, &style, width);
            system.measure_styled(text, &style, width);
            system.geometry(text, &style, width);
            assert_eq!(system.buffers.len(), 1);
            assert!(system.buffers.contains_key(&key));
        }
    }

    #[test]
    fn rich_geometry_and_ascent_share_the_buffer_of_their_ensure() {
        let style = TextStyle {
            base: Some(true),
            ..TextStyle::default()
        };
        let spans = [TextSpan::new("ab").weight(700), TextSpan::new("cd")];
        let mut system = TextSystem::default();
        system.geometry_rich(&spans, &style, Some(60.0));
        assert_eq!(system.buffers.len(), 1);
        let key = system.ensure_rich(&spans, &style, Some(60.0));
        assert!(system.buffers.contains_key(&key));
        assert_eq!(system.buffers.len(), 1);

        let mut system = TextSystem::default();
        system.ascent(&style);
        assert_eq!(system.buffers.len(), 1);
        let key = system.ensure_styled("M", &style, None);
        assert!(system.buffers.contains_key(&key));
        assert_eq!(system.buffers.len(), 1);
    }

    #[test]
    fn a_hit_reads_the_stored_buffer_and_does_not_shape_again() {
        let style = TextStyle::default();
        let mut system = TextSystem::default();
        let key = system.ensure_styled("hello", &style, None);
        let before = system.measure_styled("hello", &style, None);
        {
            let TextSystem { fonts, buffers } = &mut system;
            buffers
                .get_mut(&key)
                .unwrap()
                .set_metrics(fonts, Metrics::new(40.0, 80.0));
        }
        let after = system.measure_styled("hello", &style, None);
        assert_ne!(after.height, before.height);
        assert_eq!(after.height, 80.0);
        assert_eq!(system.geometry("hello", &style, None).size.height, 80.0);
        assert_eq!(system.ensure_styled("hello", &style, None), key);
        assert_eq!(system.buffers[&key].metrics().line_height, 80.0);
        assert_eq!(system.buffers.len(), 1);
    }

    /// A text system with the font of the wgpu renderer, read when the test
    /// runs: the font is not part of this crate's package, so a test that
    /// needs it ends without failing when the repository is not there.
    fn fira() -> Option<TextSystem> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../glassmere-render-wgpu/fonts/FiraMono-Regular.ttf"
        );
        match std::fs::read(path) {
            Ok(bytes) => Some(TextSystem::with_fonts([bytes])),
            Err(_) => {
                eprintln!("skipped: {path} is missing; this test needs the repository");
                None
            }
        }
    }

    /// A block shaped from a buffer that starts with an empty shaped line,
    /// the plain way: wrap, size, text, alignment, then layout.
    fn reference(
        fonts: &mut FontSystem,
        text: &str,
        style: &TextStyle,
        font_size: f32,
        width: Option<f32>,
    ) -> Buffer {
        let mut buffer = Buffer::new(fonts, Metrics::new(font_size, font_size * 1.4));
        buffer.set_wrap(
            fonts,
            if style.wrap == TextWrap::None {
                Wrap::None
            } else {
                Wrap::WordOrGlyph
            },
        );
        buffer.set_size(fonts, width, None);
        let base = attrs(&style.family, style.weight, style.italic);
        buffer.set_text(fonts, text, &base, Shaping::Advanced, None);
        if style.rtl && width.is_some() {
            for line in &mut buffer.lines {
                line.set_align(Some(cosmic_text::Align::Right));
            }
        }
        buffer.shape_until_scroll(fonts, false);
        buffer
    }

    fn same(left: &Buffer, right: &Buffer) {
        assert_eq!(format!("{:?}", left.lines), format!("{:?}", right.lines));
        assert_eq!(left.metrics(), right.metrics());
        assert_eq!(left.size(), right.size());
        assert_eq!(left.wrap(), right.wrap());
        assert_eq!(left.scroll(), right.scroll());
        assert_eq!(left.redraw(), right.redraw());
    }

    #[test]
    fn a_plain_block_equals_one_built_from_a_shaped_empty_line() {
        let texts = [
            "",
            "a",
            "a\nb",
            "a\n",
            "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda",
            MIXED,
        ];
        for text in texts {
            for wrap in [TextWrap::None, TextWrap::Word] {
                for width in [None, Some(80.0)] {
                    for rtl in [false, true] {
                        let style = TextStyle {
                            wrap,
                            rtl,
                            ..TextStyle::default()
                        };
                        let Some(mut system) = fira() else { return };
                        let key = system.ensure_styled(text, &style, width);
                        let (_, font_size, width) = TextSystem::key(text, &[], &style, width);
                        let mut fonts = fira().unwrap().fonts;
                        let expected = reference(&mut fonts, text, &style, font_size, width);
                        same(&system.buffers[&key], &expected);
                    }
                }
            }
        }
    }

    #[test]
    fn a_rich_block_is_built_as_before() {
        let wrapped = TextStyle {
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let base = attrs(&wrapped.family, wrapped.weight, wrapped.italic);
        let red = base.clone().color(cosmic_text::Color::rgba(255, 0, 0, 255));
        let bold = base.clone().weight(cosmic_text::Weight(700));
        let left = mark(false);
        let cases = [
            (
                vec![TextSpan::new("alpha beta").weight(700)],
                wrapped.clone(),
                vec![("alpha beta", bold.clone())],
            ),
            (
                vec![
                    TextSpan::new("alpha ").color(glassmere_scene::Color(255, 0, 0, 255)),
                    TextSpan::new("beta gamma").weight(700),
                ],
                wrapped.clone(),
                vec![("alpha ", red), ("beta gamma", bold.clone())],
            ),
            (
                vec![TextSpan::new("")],
                wrapped.clone(),
                vec![("", base.clone())],
            ),
            (
                vec![TextSpan::new("alpha").weight(700)],
                TextStyle {
                    base: Some(false),
                    ..wrapped.clone()
                },
                vec![(left, base.clone()), ("alpha", bold)],
            ),
        ];
        for (spans, style, runs) in cases {
            let Some(mut system) = fira() else { return };
            let key = system.ensure_rich(&spans, &style, Some(80.0));
            let mut fonts = fira().unwrap().fonts;
            let mut expected = Buffer::new(&mut fonts, Metrics::new(16.0, 16.0 * 1.4));
            expected.set_wrap(&mut fonts, Wrap::WordOrGlyph);
            expected.set_size(&mut fonts, Some(80.0), None);
            let unused = base
                .clone()
                .weight(cosmic_text::Weight(style.weight.clamp(100, 900) ^ 1));
            expected.set_rich_text(&mut fonts, runs, &unused, Shaping::Advanced, None);
            expected.shape_until_scroll(&mut fonts, false);
            same(&system.buffers[&key], &expected);
        }
    }

    #[test]
    fn the_first_block_shaped_matches_the_same_block_shaped_later() {
        let style = TextStyle::default();
        let bold = TextStyle {
            weight: 700,
            family: FontFamily::Monospace,
            ..TextStyle::default()
        };
        let Some(mut first) = fira() else { return };
        let key = first.ensure_styled("Hello", &style, None);
        let mut later = fira().unwrap();
        for text in ["other", "\u{5d0}\u{5d1}", "x y z"] {
            later.ensure_styled(text, &bold, None);
            later.ensure_styled(text, &style, Some(40.0));
        }
        assert_eq!(later.ensure_styled("Hello", &style, None), key);
        same(&first.buffers[&key], &later.buffers[&key]);
    }

    #[test]
    fn empty_and_newline_terminated_text_keep_their_lines() {
        let Some(mut system) = fira() else { return };
        let style = TextStyle::default();
        assert_eq!(system.measure_styled("", &style, None).height, 16.0 * 1.4);
        assert_eq!(system.geometry("a\n", &style, None).lines.len(), 2);
        let wrapped = TextStyle {
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let text = "alpha beta gamma delta epsilon";
        let one = system.measure_styled("alpha", &wrapped, Some(60.0)).height;
        assert!(system.measure_styled(text, &wrapped, Some(60.0)).height > one);
    }

    #[test]
    fn forced_and_rtl_blocks_are_laid_out() {
        let Some(mut system) = fira() else { return };
        let rtl = TextStyle {
            wrap: TextWrap::Word,
            rtl: true,
            ..TextStyle::default()
        };
        for (style, width) in [
            (rtl, Some(100.0)),
            (forced(true), None),
            (forced(false), None),
        ] {
            let key = system.ensure_styled(MIXED, &style, width);
            assert!(system.buffers[&key].layout_runs().next().is_some());
            assert!(!system.geometry(MIXED, &style, width).lines.is_empty());
        }
    }

    #[test]
    fn a_miss_inserts_one_entry_under_the_returned_key() {
        let style = TextStyle::default();
        let spans = [TextSpan::new("ab"), TextSpan::new("cd").weight(700)];
        let mut system = TextSystem::default();
        let plain = system.ensure("one", 16.0);
        assert_eq!(system.buffers.len(), 1);
        let styled = system.ensure_styled("two", &style, Some(30.0));
        assert_eq!(system.buffers.len(), 2);
        let rich = system.ensure_rich(&spans, &style, None);
        assert_eq!(system.buffers.len(), 3);
        for key in [&plain, &styled, &rich] {
            assert!(system.buffers.contains_key(key));
        }
        assert_eq!(system.ensure("one", 16.0), plain);
        assert_eq!(system.ensure_rich(&spans, &style, None), rich);
        assert_eq!(system.buffers.len(), 3);
        system.retain(&HashSet::from([rich.clone()]));
        assert_eq!(system.buffers.len(), 1);
        assert!(system.buffers.contains_key(&rich));
        system.retain(&HashSet::new());
        assert!(system.buffers.is_empty());
    }

    /// Position, font and colour of every glyph of a buffer; a colour is
    /// the bits of its channels over a set bit, no colour is zero.
    fn glyphs(buffer: &Buffer) -> Vec<[u64; 6]> {
        buffer
            .layout_runs()
            .flat_map(|run| {
                run.glyphs.iter().map(move |glyph| {
                    let color = glyph.color_opt.map_or(0, |color| {
                        1 << 32 | u64::from(u32::from_be_bytes(color.as_rgba()))
                    });
                    [
                        glyph.start as u64,
                        glyph.end as u64,
                        u64::from(glyph.glyph_id) << 16 | u64::from(glyph.font_weight.0),
                        color,
                        u64::from(glyph.x.to_bits()),
                        u64::from(run.line_top.to_bits()),
                    ]
                })
            })
            .collect()
    }

    #[test]
    fn the_ensure_path_shapes_the_spans_like_the_measure_path() {
        let red = glassmere_scene::Color(255, 0, 0, 255);
        let spans = [
            TextSpan::new("alpha ").color(red),
            TextSpan::new("beta ").weight(700).italic(true),
            TextSpan::new("gamma delta"),
        ];
        let wrapped = TextStyle {
            wrap: TextWrap::Word,
            base: Some(false),
            ..TextStyle::default()
        };
        for (style, width) in [
            (TextStyle::default(), None),
            (wrapped.clone(), Some(60.0)),
            (wrapped, None),
        ] {
            let mut ensured = TextSystem::default();
            let key = ensured.ensure_rich(&spans, &style, width);
            let mut measured = TextSystem::default();
            measured.geometry_rich(&spans, &style, width);
            assert_eq!(measured.buffers.len(), 1);
            let expected = glyphs(&measured.buffers[&key]);
            let shaped = glyphs(&ensured.buffers[&key]);
            assert!(
                expected
                    .iter()
                    .any(|glyph| glyph[3] == 1 << 32 | 0xff00_00ff)
            );
            assert!(expected.iter().any(|glyph| glyph[2] & 0xffff == 700));
            assert_eq!(shaped, expected);
            assert_eq!(ensured.buffers[&key].size(), measured.buffers[&key].size());
        }
    }

    #[test]
    fn measuring_agrees_with_the_buffer_of_the_ensure() {
        let paragraph = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda";
        let style = TextStyle {
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        for width in [Some(120.0), Some(360.0), None] {
            let mut system = TextSystem::default();
            let measured = system.measure_styled(paragraph, &style, width);
            let key = system.ensure_styled(paragraph, &style, width);
            let buffer = &system.buffers[&key];
            let mut expected = Size::new(0.0, buffer.metrics().line_height);
            for run in buffer.layout_runs() {
                expected.width = expected.width.max(run.line_w);
                expected.height = expected.height.max(run.line_top + run.line_height);
            }
            assert_eq!(measured, expected);
            assert_eq!(system.geometry(paragraph, &style, width).size, expected);
        }
    }

    #[test]
    fn keys_use_the_normalised_width_and_size() {
        let mut system = TextSystem::default();
        let text = "alpha beta gamma";
        let none = TextStyle::default();
        let wrapped = TextStyle {
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        assert_eq!(
            system.ensure_styled(text, &none, Some(50.0)),
            system.ensure_styled(text, &none, None)
        );
        assert_eq!(
            system.ensure_styled(text, &wrapped, Some(0.25)),
            system.ensure_styled(text, &wrapped, Some(1.0))
        );
        assert_eq!(
            system.ensure_styled(text, &wrapped, Some(f32::NAN)),
            system.ensure_styled(text, &wrapped, None)
        );
        let nan_size = TextStyle {
            font_size: f32::NAN,
            ..wrapped.clone()
        };
        assert_eq!(
            system.ensure_styled(text, &nan_size, Some(80.0)),
            system.ensure_styled(text, &wrapped, Some(80.0))
        );
        // Alignment needs a width, which a wrap-less block does not have.
        let rtl_none = TextStyle {
            rtl: true,
            ..none.clone()
        };
        assert_eq!(
            system.ensure_styled(text, &rtl_none, Some(50.0)),
            system.ensure_styled(text, &rtl_none, None)
        );
        assert_eq!(
            system.ensure_styled(text, &rtl_none, None),
            system.ensure_styled(text, &none, None)
        );
        let rtl_wrapped = TextStyle {
            rtl: true,
            ..wrapped.clone()
        };
        assert_eq!(
            system.ensure_styled(text, &rtl_wrapped, Some(f32::NAN)),
            system.ensure_styled(text, &rtl_wrapped, None)
        );
        assert!(!system.ensure_styled(text, &rtl_wrapped, None).rtl.0);
        assert!(system.ensure_styled(text, &rtl_wrapped, Some(50.0)).rtl.0);
        // A wrap-less block shaped through the measure path is laid out
        // unwrapped whatever width is offered.
        let narrow = system.measure_styled(text, &none, Some(20.0));
        assert_eq!(narrow, system.measure_styled(text, &none, None));
        assert!(narrow.width > 20.0);
    }

    fn plain_key(text: &str, style: &TextStyle, width: Option<f32>) -> TextKey {
        TextSystem::key(text, &[], style, width).0
    }

    fn map_hash(state: &RandomState, key: &TextKey) -> u64 {
        state.hash_one(key)
    }

    #[test]
    fn inputs_that_normalise_alike_give_equal_keys_and_one_buffer() {
        let state = RandomState::new();
        let style = |edit: fn(&mut TextStyle)| {
            let mut style = TextStyle::default();
            edit(&mut style);
            style
        };
        let wrapped = |edit: fn(&mut TextStyle)| TextStyle {
            wrap: TextWrap::Word,
            ..style(edit)
        };
        let cases: [(TextStyle, Option<f32>, TextStyle, Option<f32>); 8] = [
            (
                style(|s| s.weight = 950),
                None,
                style(|s| s.weight = 900),
                None,
            ),
            (
                style(|s| s.weight = 20),
                None,
                style(|s| s.weight = 100),
                None,
            ),
            (
                style(|s| s.font_size = f32::NAN),
                None,
                TextStyle::default(),
                None,
            ),
            (
                style(|s| s.font_size = 0.25),
                None,
                style(|s| s.font_size = 1.0),
                None,
            ),
            (
                style(|s| s.font_size = f32::INFINITY),
                None,
                TextStyle::default(),
                None,
            ),
            (
                style(|s| s.rtl = true),
                None,
                style(|s| s.rtl = false),
                None,
            ),
            (TextStyle::default(), Some(80.0), TextStyle::default(), None),
            (
                wrapped(|s| s.rtl = true),
                Some(f32::NAN),
                wrapped(|s| s.rtl = false),
                None,
            ),
        ];
        for (index, (left, left_width, right, right_width)) in cases.into_iter().enumerate() {
            let a = plain_key("same", &left, left_width);
            let b = plain_key("same", &right, right_width);
            assert_eq!(a, b, "case {index}");
            assert_eq!(map_hash(&state, &a), map_hash(&state, &b), "case {index}");
            let mut system = TextSystem::default();
            system.ensure_styled("same", &left, left_width);
            system.ensure_styled("same", &right, right_width);
            assert_eq!(system.buffers.len(), 1, "case {index}");
        }
        let wide = wrapped(|s| s.rtl = true);
        let a = plain_key("same", &wide, Some(0.25));
        let b = plain_key("same", &wide, Some(1.0));
        assert_eq!(a, b);
        assert_eq!(map_hash(&state, &a), map_hash(&state, &b));
    }

    #[test]
    fn keys_that_differ_in_one_field_are_all_distinct() {
        let wrapped = TextStyle {
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let with = |edit: fn(&mut TextStyle)| {
            let mut style = wrapped.clone();
            edit(&mut style);
            plain_key("ab", &style, Some(100.0))
        };
        let rich = |spans: &[TextSpan]| {
            let text: String = spans.iter().map(|span| span.text.as_str()).collect();
            TextSystem::key(&text, spans, &wrapped, Some(100.0)).0
        };
        let keys = vec![
            plain_key("ab", &wrapped, Some(100.0)),
            plain_key("ac", &wrapped, Some(100.0)),
            plain_key("ab", &wrapped, Some(101.0)),
            plain_key("ab", &wrapped, None),
            with(|s| s.font_size = 17.0),
            with(|s| s.family = FontFamily::Serif),
            with(|s| s.family = FontFamily::Named("a".into())),
            with(|s| s.family = FontFamily::Named("b".into())),
            with(|s| s.weight = 700),
            with(|s| s.italic = true),
            with(|s| s.wrap = TextWrap::None),
            with(|s| s.rtl = true),
            with(|s| s.base = Some(false)),
            with(|s| s.base = Some(true)),
            rich(&[TextSpan::new("ab")]),
            rich(&[TextSpan::new("a"), TextSpan::new("b")]),
            rich(&[TextSpan::new("ab"), TextSpan::new("")]),
            rich(&[TextSpan::new("ab").weight(700)]),
            rich(&[TextSpan::new("ab").italic(true)]),
            rich(&[TextSpan::new("ab").family(FontFamily::Monospace)]),
            rich(&[TextSpan::new("ab").color(glassmere_scene::Color(1, 2, 3, 4))]),
            rich(&[TextSpan::new("a"), TextSpan::new("b").weight(700)]),
            plain_key("", &wrapped, Some(100.0)),
            plain_key("\0", &wrapped, Some(100.0)),
        ];
        let state = RandomState::new();
        let set: HashSet<&TextKey> = keys.iter().collect();
        assert_eq!(set.len(), keys.len());
        for (i, a) in keys.iter().enumerate() {
            for (j, b) in keys.iter().enumerate() {
                assert_eq!(a == b, i == j, "{i} against {j}");
                assert_eq!(
                    a.text.hash == b.text.hash,
                    i == j,
                    "cached hashes of {i} and {j}"
                );
                assert_eq!(map_hash(&state, a) == map_hash(&state, b), i == j);
            }
        }
    }

    #[test]
    fn a_rich_block_without_spans_is_the_plain_block_of_no_text() {
        let Some(mut system) = fira() else { return };
        let style = TextStyle {
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let rich = system.ensure_rich(&[], &style, Some(80.0));
        let plain = system.ensure_styled("", &style, Some(80.0));
        assert_eq!(rich, plain);
        assert_eq!(format!("{rich:?}"), format!("{plain:?}"));
        assert!(format!("{rich:?}").ends_with("spans: [] }"));
        assert_eq!(system.buffers.len(), 1);
        // A span of no text is still a span, and tells the keys apart.
        let one = system.ensure_rich(&[TextSpan::new("")], &style, Some(80.0));
        assert_ne!(one, plain);
        assert!(format!("{one:?}").ends_with("spans: [(0, None, None, None, None)] }"));
        assert_eq!(system.buffers.len(), 2);
    }

    #[test]
    fn keys_with_one_hash_are_told_apart_by_their_fields() {
        let style = TextStyle::default();
        let a = plain_key("first", &style, None);
        let mut other_text = a.clone();
        other_text.text.text = "second".into();
        let mut other_width = a.clone();
        other_width.width = Skip(Some(7));
        let mut other_family = a.clone();
        other_family.family = Skip(FontFamily::Monospace);
        let mut other_spans = a.clone();
        other_spans.spans = Skip(vec![(5, None, None, None, None)]);
        let keys = [a, other_text, other_width, other_family, other_spans];
        let state = RandomState::new();
        for (i, left) in keys.iter().enumerate() {
            assert_eq!(left.text.hash, keys[0].text.hash);
            for (j, right) in keys.iter().enumerate() {
                assert_eq!(left == right, i == j, "{i} against {j}");
                assert_eq!(map_hash(&state, left), map_hash(&state, right));
            }
        }
        let map: HashMap<TextKey, usize> = keys.iter().cloned().zip(0..).collect();
        assert_eq!(map.len(), keys.len());
        for (index, key) in keys.iter().enumerate() {
            assert_eq!(map[key], index);
        }
    }

    #[test]
    fn a_key_feeds_a_hasher_one_word() {
        #[derive(Default)]
        struct Recorder {
            calls: Vec<String>,
        }
        impl Hasher for Recorder {
            fn write(&mut self, bytes: &[u8]) {
                self.calls.push(format!("write {bytes:?}"));
            }
            fn write_u8(&mut self, value: u8) {
                self.calls.push(format!("u8 {value}"));
            }
            fn write_u16(&mut self, value: u16) {
                self.calls.push(format!("u16 {value}"));
            }
            fn write_u32(&mut self, value: u32) {
                self.calls.push(format!("u32 {value}"));
            }
            fn write_u64(&mut self, value: u64) {
                self.calls.push(format!("u64 {value}"));
            }
            fn write_usize(&mut self, value: usize) {
                self.calls.push(format!("usize {value}"));
            }
            fn finish(&self) -> u64 {
                0
            }
        }
        let spans = [TextSpan::new("ab"), TextSpan::new("cd").weight(700)];
        let style = TextStyle {
            family: FontFamily::Named("x".into()),
            ..TextStyle::default()
        };
        let plain = plain_key("hello", &style, None);
        let rich = TextSystem::key("abcd", &spans, &style, Some(5.0)).0;
        for key in [plain, rich] {
            let mut recorder = Recorder::default();
            key.hash(&mut recorder);
            assert_eq!(recorder.calls, [format!("u64 {}", key.text.hash)]);
        }
    }

    #[test]
    fn a_key_prints_the_way_its_fields_print() {
        let style = TextStyle::default();
        let plain = plain_key("hi", &style, None);
        assert_eq!(
            format!("{plain:?}"),
            "TextKey { text: \"hi\", size: 1098907648, family: SansSerif, weight: 400, \
             italic: false, wrap: None, rtl: false, align: Start, base: None, width: None, spans: [] }"
        );
        assert_eq!(
            format!("{plain:#?}"),
            "TextKey {\n    text: \"hi\",\n    size: 1098907648,\n    family: SansSerif,\n    \
             weight: 400,\n    italic: false,\n    wrap: None,\n    rtl: false,\n    align: Start,\n    \
             base: None,\n    width: None,\n    spans: [],\n}"
        );
        let spans = [
            TextSpan::new("a"),
            TextSpan::new("b")
                .weight(700)
                .italic(true)
                .family(FontFamily::Named("z".into()))
                .color(glassmere_scene::Color(1, 2, 3, 4)),
        ];
        let style = TextStyle {
            family: FontFamily::Named("q".into()),
            wrap: TextWrap::Word,
            base: Some(true),
            ..TextStyle::default()
        };
        let rich = TextSystem::key("ab", &spans, &style, Some(50.0)).0;
        assert_eq!(
            format!("{rich:?}"),
            "TextKey { text: \"ab\", size: 1098907648, family: Named(\"q\"), weight: 400, \
             italic: false, wrap: Word, rtl: false, align: Start, base: Some(true), width: Some(1112014848), \
             spans: [(1, None, None, None, None), \
             (1, Some([1, 2, 3, 4]), Some(700), Some(true), Some(Named(\"z\")))] }"
        );
        assert_eq!(
            format!("{rich:#?}"),
            "TextKey {\n    text: \"ab\",\n    size: 1098907648,\n    family: Named(\n        \
             \"q\",\n    ),\n    weight: 400,\n    italic: false,\n    wrap: Word,\n    \
             rtl: false,\n    align: Start,\n    base: Some(\n        true,\n    ),\n    width: Some(\n        \
             1112014848,\n    ),\n    spans: [\n        (\n            1,\n            None,\n            \
             None,\n            None,\n            None,\n        ),\n        (\n            1,\n            \
             Some(\n                [\n                    1,\n                    2,\n                    \
             3,\n                    4,\n                ],\n            ),\n            \
             Some(\n                700,\n            ),\n            Some(\n                \
             true,\n            ),\n            Some(\n                Named(\n                    \
             \"z\",\n                ),\n            ),\n        ),\n    ],\n}"
        );
    }

    /// The left and right edge of the characters of each line.
    fn extents(geometry: &TextGeometry) -> Vec<(f32, f32)> {
        geometry
            .lines
            .iter()
            .map(|line| {
                let inside = geometry
                    .clusters
                    .iter()
                    .filter(|cluster| {
                        cluster.bytes.start >= line.bytes.start
                            && cluster.bytes.end <= line.bytes.end
                    })
                    .filter(|cluster| cluster.rect.width > 0.0);
                inside.fold((f32::MAX, f32::MIN), |(left, right), cluster| {
                    (
                        left.min(cluster.rect.x),
                        right.max(cluster.rect.x + cluster.rect.width),
                    )
                })
            })
            .collect()
    }

    #[test]
    fn wrapped_lines_sit_where_the_alignment_puts_them() {
        use glassmere_scene::TextAlign;
        let text = "one two three four five six seven eight";
        let width = 140.0;
        let mut system = TextSystem::default();
        let mut shaped = |align, rtl| {
            let style = TextStyle {
                wrap: TextWrap::Word,
                align,
                rtl,
                ..TextStyle::default()
            };
            extents(&system.geometry(text, &style, Some(width)))
        };
        let start = shaped(TextAlign::Start, false);
        assert!(start.len() > 2, "the text wraps");
        assert!(start.iter().all(|(left, _)| left.abs() < 0.5), "{start:?}");
        assert!(
            start.iter().any(|(_, right)| *right < width - 5.0),
            "some line is short"
        );

        let center = shaped(TextAlign::Center, false);
        for ((left, right), (_, natural)) in center.iter().zip(&start) {
            assert!(
                ((left + right) / 2.0 - width / 2.0).abs() < 1.0,
                "{center:?}"
            );
            assert!(
                (right - left - natural).abs() < 1.0,
                "a line keeps its width"
            );
        }
        assert!(center.iter().any(|(left, _)| *left > 3.0));

        let end = shaped(TextAlign::End, false);
        assert!(
            end.iter().all(|(_, right)| (right - width).abs() < 1.0),
            "{end:?}"
        );

        // The last line of a justified block is not stretched; the others
        // reach both edges.
        let justify = shaped(TextAlign::Justify, false);
        let last = justify.len() - 1;
        for (index, (left, right)) in justify.iter().enumerate() {
            assert!(left.abs() < 1.0, "{justify:?}");
            if index < last {
                assert!((right - width).abs() < 1.5, "{justify:?}");
            }
        }

        // Right-to-left UI starts at the right edge, and `End` there is the
        // left one.
        let rtl_start = shaped(TextAlign::Start, true);
        assert!(
            rtl_start
                .iter()
                .all(|(_, right)| (right - width).abs() < 1.0)
        );
        let rtl_end = shaped(TextAlign::End, true);
        assert!(
            rtl_end.iter().all(|(left, _)| left.abs() < 1.0),
            "{rtl_end:?}"
        );
    }

    #[test]
    fn alignment_needs_a_width_and_a_block_without_one_is_where_it_was() {
        use glassmere_scene::TextAlign;
        let mut system = TextSystem::default();
        let plain = system.geometry("one two", &TextStyle::default(), None);
        for align in [TextAlign::Center, TextAlign::End, TextAlign::Justify] {
            let style = TextStyle {
                align,
                ..TextStyle::default()
            };
            let aligned = system.geometry("one two", &style, None);
            assert_eq!(extents(&aligned), extents(&plain), "{align:?}");
            // And the key is the same one, so nothing is shaped twice.
            assert_eq!(
                TextSystem::key("one two", &[], &style, None).0,
                TextSystem::key("one two", &[], &TextStyle::default(), None).0
            );
        }
        let wrapped = |align| TextStyle {
            wrap: TextWrap::Word,
            align,
            ..TextStyle::default()
        };
        assert_ne!(
            TextSystem::key("one two", &[], &wrapped(TextAlign::Start), Some(50.0)).0,
            TextSystem::key("one two", &[], &wrapped(TextAlign::Center), Some(50.0)).0
        );
    }

    #[test]
    fn a_key_finds_its_buffer_in_another_system_and_thread() {
        let style = TextStyle {
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let mut first = TextSystem::default();
        let mut second = TextSystem::default();
        let made = first.ensure_styled("shared words", &style, Some(60.0));
        second.ensure_styled("shared words", &style, Some(60.0));
        second.ensure_styled("other words", &style, Some(60.0));
        assert!(second.buffers.contains_key(&made));
        let elsewhere = std::thread::scope(|scope| {
            scope
                .spawn(|| plain_key("shared words", &style, Some(60.0)))
                .join()
                .unwrap()
        });
        assert_eq!(elsewhere, made);
        assert!(second.buffers.contains_key(&elsewhere));
        second.retain(&HashSet::from([made.clone()]));
        assert_eq!(second.buffers.len(), 1);
        assert!(second.buffers.contains_key(&elsewhere));
        second.retain(&HashSet::from([elsewhere]));
        assert_eq!(second.buffers.len(), 1);
    }

    #[test]
    fn texts_that_pad_alike_hash_apart() {
        let style = TextStyle::default();
        let hash = |text: &str, style: &TextStyle| plain_key(text, style, None).text.hash;
        let named = |name: &str| TextStyle {
            family: FontFamily::Named(name.into()),
            ..TextStyle::default()
        };
        let hashes = [
            hash("a", &style),
            hash("a\0", &style),
            hash("", &style),
            hash("\0", &style),
            hash("\0\0\0\0\0\0\0", &style),
            hash("\0\0\0\0\0\0\0\0", &style),
            hash("abcdefgh", &style),
            hash("abcdefgh\0", &style),
            hash("ab", &named("")),
            hash("a", &named("b")),
            hash("ab", &named("c")),
            hash("a", &named("bc")),
        ];
        let distinct: HashSet<u64> = hashes.into_iter().collect();
        assert_eq!(distinct.len(), hashes.len());
    }
}
