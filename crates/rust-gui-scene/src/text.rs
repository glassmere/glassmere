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
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            family: FontFamily::SansSerif,
            weight: 400,
            italic: false,
            wrap: TextWrap::None,
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextGeometry {
    pub size: Size,
    pub carets: Vec<CaretStop>,
    pub clusters: Vec<TextCluster>,
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

    /// Selection rectangles for each intersecting shaped grapheme.
    pub fn selection(&self, range: Range<usize>) -> Vec<Rect> {
        self.clusters
            .iter()
            .filter(|cluster| cluster.bytes.start < range.end && cluster.bytes.end > range.start)
            .map(|cluster| cluster.rect)
            .collect()
    }
}

/// Shared shaping contract. Desktop implementations use the renderer's fonts;
/// the synthetic implementation below is for deterministic headless tests.
pub trait TextMeasurer {
    fn measure_styled(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> Size;
    fn geometry(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> TextGeometry;

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
        let mut x = 0.0_f32;
        let mut y = 0.0;
        let mut max_x = 0.0_f32;
        let mut carets = Vec::new();
        let mut clusters = Vec::new();
        for (byte, grapheme) in text.grapheme_indices(true) {
            if style.wrap == TextWrap::Word && x > 0.0 && x + advance > limit {
                x = 0.0;
                y += line_height;
            }
            carets.push(CaretStop {
                byte,
                rect: Rect {
                    x,
                    y,
                    width: 1.0,
                    height: line_height,
                },
            });
            clusters.push(TextCluster {
                bytes: byte..byte + grapheme.len(),
                rect: Rect {
                    x,
                    y,
                    width: advance,
                    height: line_height,
                },
                rtl: false,
            });
            if grapheme == "\n" {
                x = 0.0;
                y += line_height;
            } else {
                x += advance;
                max_x = max_x.max(x);
            }
        }
        carets.push(CaretStop {
            byte: text.len(),
            rect: Rect {
                x,
                y,
                width: 1.0,
                height: line_height,
            },
        });
        TextGeometry {
            size: Size::new(max_x, y + line_height),
            carets,
            clusters,
        }
    }
}
