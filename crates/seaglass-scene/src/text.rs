use crate::{Point, Rect, Size};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontFamily {
    #[default]
    SansSerif,
    Serif,
    Monospace,
    Named(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextWrap {
    #[default]
    None,
    Word,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub font_size: f32,
    pub family: FontFamily,
    pub weight: u16,
    pub italic: bool,
    pub wrap: TextWrap,
    /// The block belongs to right-to-left UI: wrapped lines align to the right
    /// edge whatever their content. Left-to-right UI keeps each line on the
    /// side its own content asks for.
    pub rtl: bool,
    /// The direction the paragraph starts in, whatever its first strong
    /// character is: `Some(true)` for right to left, `Some(false)` for left to
    /// right, `None` to let the text decide. Editors leave it unset, because
    /// forcing it shifts the byte offsets a caret is placed by.
    pub base: Option<bool>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            family: FontFamily::SansSerif,
            weight: 400,
            italic: false,
            wrap: TextWrap::None,
            rtl: false,
            base: None,
        }
    }
}

/// A grapheme boundary in UTF-8 bytes and its shaped logical caret geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretStop {
    pub byte: usize,
    pub rect: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextCluster {
    pub bytes: Range<usize>,
    pub rect: Rect,
    pub rtl: bool,
}

/// One visual line: its box, base direction and the bytes it shows, without
/// a trailing line break. A wrapped line ends where the next one starts.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    pub y: f32,
    pub height: f32,
    pub rtl: bool,
    pub bytes: Range<usize>,
}

/// Which neighbour a caret belongs to where one byte offset has two places on
/// screen: at direction boundaries and between wrapped lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Affinity {
    /// With the character after the offset: its leading edge.
    #[default]
    Downstream,
    /// With the character before the offset: its trailing edge.
    Upstream,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextGeometry {
    pub size: Size,
    pub carets: Vec<CaretStop>,
    pub clusters: Vec<TextCluster>,
    pub lines: Vec<TextLine>,
}

impl TextGeometry {
    pub fn caret(&self, byte: usize) -> Rect {
        self.carets
            .iter()
            .find(|stop| stop.byte == byte)
            .or(self.carets.last())
            .map_or(Rect::default(), |stop| stop.rect)
    }

    pub fn hit(&self, point: Point) -> usize {
        if let Some(cluster) = self.clusters.iter().min_by(|a, b| {
            let distance = |cluster: &TextCluster| {
                let rect = cluster.rect;
                let dy = (rect.y - point.y).max(0.0) + (point.y - rect.y - rect.height).max(0.0);
                let dx = (rect.x - point.x).max(0.0) + (point.x - rect.x - rect.width).max(0.0);
                dy * 10000.0 + dx
            };
            distance(a).total_cmp(&distance(b))
        }) {
            return if (point.x < cluster.rect.x + cluster.rect.width / 2.0) != cluster.rtl {
                cluster.bytes.start
            } else {
                cluster.bytes.end
            };
        }
        self.carets
            .iter()
            .min_by(|a, b| {
                let distance = |stop: &CaretStop| {
                    let vertical = if point.y < stop.rect.y {
                        stop.rect.y - point.y
                    } else if point.y > stop.rect.y + stop.rect.height {
                        point.y - stop.rect.y - stop.rect.height
                    } else {
                        0.0
                    };
                    vertical * 10000.0 + (point.x - stop.rect.x).abs()
                };
                distance(a).total_cmp(&distance(b))
            })
            .map_or(0, |stop| stop.byte)
    }

    fn edge(cluster: &TextCluster, leading: bool) -> Rect {
        // The leading edge is the left of an LTR cluster and the right of an RTL one.
        let x = if leading != cluster.rtl {
            cluster.rect.x
        } else {
            cluster.rect.x + cluster.rect.width
        };
        Rect {
            x,
            width: 1.0,
            ..cluster.rect
        }
    }

    /// The caret rectangle of an offset with an affinity. Without a neighbour
    /// on the preferred side, the other side is used.
    pub fn caret_at(&self, byte: usize, affinity: Affinity) -> Rect {
        let after = self
            .clusters
            .iter()
            .find(|cluster| cluster.bytes.start == byte)
            .map(|cluster| Self::edge(cluster, true));
        let before = self
            .clusters
            .iter()
            .find(|cluster| cluster.bytes.end == byte)
            .map(|cluster| Self::edge(cluster, false));
        match affinity {
            Affinity::Downstream => after.or(before),
            Affinity::Upstream => before.or(after),
        }
        .unwrap_or_else(|| self.caret(byte))
    }

    /// `hit` plus the side of the character that was hit, so the caret shows
    /// where the pointer went even at a direction boundary or a wrapped end.
    pub fn hit_affinity(&self, point: Point) -> (usize, Affinity) {
        let byte = self.hit(point);
        let cluster = self
            .clusters
            .iter()
            .filter(|cluster| cluster.bytes.start == byte || cluster.bytes.end == byte)
            .min_by(|a, b| {
                let distance = |cluster: &TextCluster| {
                    let rect = cluster.rect;
                    let dy =
                        (rect.y - point.y).max(0.0) + (point.y - rect.y - rect.height).max(0.0);
                    let dx = (rect.x - point.x).max(0.0) + (point.x - rect.x - rect.width).max(0.0);
                    dy * 10000.0 + dx
                };
                distance(a).total_cmp(&distance(b))
            });
        match cluster {
            Some(cluster) if cluster.bytes.end == byte => (byte, Affinity::Upstream),
            _ => (byte, Affinity::Downstream),
        }
    }

    fn clusters_on(&self, y: f32) -> impl Iterator<Item = &TextCluster> {
        self.clusters
            .iter()
            .filter(move |cluster| (cluster.rect.y - y).abs() < 0.5)
    }

    /// One step left or right on screen, across direction boundaries. `None`
    /// at the visual end of the line, where the caller leaves the line.
    pub fn visual_step(
        &self,
        byte: usize,
        affinity: Affinity,
        right: bool,
    ) -> Option<(usize, Affinity)> {
        let caret = self.caret_at(byte, affinity);
        let center = |cluster: &TextCluster| cluster.rect.x + cluster.rect.width / 2.0;
        let line = self.clusters_on(caret.y);
        let next = if right {
            line.filter(|cluster| center(cluster) > caret.x)
                .min_by(|a, b| a.rect.x.total_cmp(&b.rect.x))
        } else {
            line.filter(|cluster| center(cluster) < caret.x)
                .max_by(|a, b| a.rect.x.total_cmp(&b.rect.x))
        }?;
        // Land on the far edge of the cluster that was stepped over.
        Some(if right != next.rtl {
            (next.bytes.end, Affinity::Upstream)
        } else {
            (next.bytes.start, Affinity::Downstream)
        })
    }

    /// The visual line a caret is drawn on.
    pub fn line_at(&self, byte: usize, affinity: Affinity) -> Option<&TextLine> {
        let caret = self.caret_at(byte, affinity);
        self.lines
            .iter()
            .find(|line| (line.y - caret.y).abs() < 0.5)
    }

    /// Logical start or end of the caret's visual line. The end of a wrapped
    /// line keeps an upstream affinity so it is drawn on that line.
    pub fn line_edge(&self, byte: usize, affinity: Affinity, end: bool) -> (usize, Affinity) {
        match self.line_at(byte, affinity) {
            Some(line) if end => (line.bytes.end, Affinity::Upstream),
            Some(line) => (line.bytes.start, Affinity::Downstream),
            None => (byte, affinity),
        }
    }

    /// Direction of the character a caret belongs to, when its line mixes
    /// directions and the caret is therefore worth flagging.
    pub fn mixed_direction(&self, byte: usize, affinity: Affinity) -> Option<bool> {
        let caret = self.caret_at(byte, affinity);
        let mut line = self.clusters_on(caret.y);
        let first = line.next()?.rtl;
        if line.all(|cluster| cluster.rtl == first) {
            return None;
        }
        let own = |cluster: &&TextCluster| match affinity {
            Affinity::Upstream => cluster.bytes.end == byte,
            Affinity::Downstream => cluster.bytes.start == byte,
        };
        self.clusters
            .iter()
            .find(own)
            .or_else(|| {
                self.clusters
                    .iter()
                    .find(|cluster| cluster.bytes.start == byte || cluster.bytes.end == byte)
            })
            .map(|cluster| cluster.rtl)
    }

    /// Selection rectangles for each intersecting shaped grapheme.
    pub fn selection(&self, range: Range<usize>) -> Vec<Rect> {
        self.clusters
            .iter()
            .filter(|cluster| cluster.bytes.start < range.end && cluster.bytes.end > range.start)
            .map(|cluster| cluster.rect)
            .collect()
    }
}

/// One run of a rich text block. Unset properties fall back to the block's
/// `TextStyle` and colour. Font size and wrapping are per block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextSpan {
    pub text: String,
    pub color: Option<crate::Color>,
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    pub family: Option<FontFamily>,
}

impl TextSpan {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
    pub fn color(mut self, color: crate::Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn weight(mut self, weight: u16) -> Self {
        self.weight = Some(weight);
        self
    }
    pub fn italic(mut self, italic: bool) -> Self {
        self.italic = Some(italic);
        self
    }
    pub fn family(mut self, family: FontFamily) -> Self {
        self.family = Some(family);
        self
    }
}

/// Shared shaping contract. Desktop implementations use the renderer's fonts;
/// the synthetic implementation below is for deterministic headless tests.
pub trait TextMeasurer {
    fn measure_styled(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> Size;
    fn geometry(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> TextGeometry;

    /// Geometry of a rich block; byte offsets index the concatenated span
    /// texts. The default ignores span fonts, which is exact for measurers whose
    /// advances do not depend on weight, slant or family.
    fn geometry_rich(
        &mut self,
        spans: &[TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextGeometry {
        let text: String = spans.iter().map(|span| span.text.as_str()).collect();
        self.geometry(&text, style, width)
    }

    /// How far the first line's baseline sits below the top of the block.
    /// The default is a typical Latin ascent; a measurer with real font
    /// metrics answers with them, and that is what baseline alignment lines
    /// text of different sizes up by.
    fn ascent(&mut self, style: &TextStyle) -> f32 {
        style.font_size * 0.8
    }

    fn measure(&mut self, text: &str, font_size: f32) -> Size {
        self.measure_styled(
            text,
            &TextStyle {
                font_size,
                ..TextStyle::default()
            },
            None,
        )
    }
}

#[derive(Default)]
pub struct MonospaceMeasurer;

impl TextMeasurer for MonospaceMeasurer {
    fn measure_styled(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> Size {
        self.geometry(text, style, width).size
    }

    fn geometry(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> TextGeometry {
        let advance = style.font_size.max(1.0) * 0.6;
        let line_height = style.font_size.max(1.0) * 1.4;
        let limit = width
            .filter(|width| width.is_finite())
            .unwrap_or(f32::MAX)
            .max(advance);
        let per_line = if style.wrap == TextWrap::Word {
            ((limit / advance).floor() as usize).max(1)
        } else {
            usize::MAX
        };
        // Visual lines in logical order: hard breaks and fixed-width wrapping.
        let mut lines: Vec<Vec<(usize, &str)>> = vec![Vec::new()];
        let mut starts = vec![0];
        for (byte, grapheme) in text.grapheme_indices(true) {
            if grapheme == "\n" || grapheme == "\r\n" {
                lines.push(Vec::new());
                starts.push(byte + grapheme.len());
                continue;
            }
            if lines.last().unwrap().len() >= per_line {
                lines.push(Vec::new());
                starts.push(byte);
            }
            lines.last_mut().unwrap().push((byte, grapheme));
        }
        let mut geometry = TextGeometry::default();
        let mut max_x = 0.0_f32;
        for (index, (items, start)) in lines.iter().zip(&starts).enumerate() {
            let y = index as f32 * line_height;
            // Strong characters set a direction; the rest follow their neighbour.
            let strong: Vec<Option<bool>> = items
                .iter()
                .map(|(_, grapheme)| grapheme.chars().next().and_then(strong_rtl))
                .collect();
            let base = strong.iter().flatten().next().copied().unwrap_or(false);
            let mut current = base;
            let directions: Vec<bool> = strong
                .iter()
                .map(|direction| {
                    current = direction.unwrap_or(current);
                    current
                })
                .collect();
            // Runs opposite to the base direction are reversed, then an RTL
            // line is mirrored as a whole.
            let mut order: Vec<usize> = Vec::with_capacity(items.len());
            let mut at = 0;
            while at < items.len() {
                let mut end = at;
                while end < items.len() && directions[end] == directions[at] {
                    end += 1;
                }
                if directions[at] == base {
                    order.extend(at..end);
                } else {
                    order.extend((at..end).rev());
                }
                at = end;
            }
            if base {
                order.reverse();
            }
            let shift = match width {
                Some(width) if style.rtl && width.is_finite() => {
                    (width - items.len() as f32 * advance).max(0.0)
                }
                _ => 0.0,
            };
            for (slot, item) in order.iter().enumerate() {
                let (byte, grapheme) = items[*item];
                geometry.clusters.push(TextCluster {
                    bytes: byte..byte + grapheme.len(),
                    rect: Rect {
                        x: shift + slot as f32 * advance,
                        y,
                        width: advance,
                        height: line_height,
                    },
                    rtl: directions[*item],
                });
            }
            max_x = max_x.max(shift + items.len() as f32 * advance);
            let end = items
                .last()
                .map_or(*start, |(byte, grapheme)| byte + grapheme.len());
            geometry.lines.push(TextLine {
                y,
                height: line_height,
                rtl: base,
                bytes: *start..end,
            });
            if items.is_empty() {
                geometry.carets.push(CaretStop {
                    byte: *start,
                    rect: Rect {
                        x: shift,
                        y,
                        width: 1.0,
                        height: line_height,
                    },
                });
            }
        }
        geometry.clusters.sort_by_key(|cluster| cluster.bytes.start);
        for cluster in &geometry.clusters {
            geometry.carets.push(CaretStop {
                byte: cluster.bytes.start,
                rect: TextGeometry::edge(cluster, true),
            });
        }
        if let Some(last) = geometry.clusters.last()
            && last.bytes.end == text.len()
        {
            geometry.carets.push(CaretStop {
                byte: text.len(),
                rect: TextGeometry::edge(last, false),
            });
        }
        geometry.carets.sort_by_key(|caret| caret.byte);
        geometry.size = Size::new(max_x, lines.len() as f32 * line_height);
        geometry
    }
}

/// Direction of a strong character in the synthetic measurer: Hebrew and
/// Arabic blocks are right-to-left, letters and digits left-to-right.
fn strong_rtl(ch: char) -> Option<bool> {
    match ch {
        '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}' => Some(true),
        ch if ch.is_alphanumeric() => Some(false),
        _ => None,
    }
}
