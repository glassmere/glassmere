use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Wrap};
use rust_gui_scene::{
    CaretStop, FontFamily, Rect, Size, TextCluster, TextGeometry, TextMeasurer, TextSpan,
    TextStyle, TextWrap,
};
use std::collections::{HashMap, HashSet};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TextKey {
    text: String,
    size: u32,
    family: FontFamily,
    weight: u16,
    italic: bool,
    wrap: TextWrap,
    width: Option<u32>,
    /// Byte length and overrides of each run of a rich block; empty for plain text.
    spans: Vec<SpanKey>,
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
        .weight(glyphon::Weight(weight.clamp(100, 900)))
        .style(if italic {
            glyphon::Style::Italic
        } else {
            glyphon::Style::Normal
        })
}

/// System fonts and shared shaping buffers for measurement, painting and editing.
pub struct TextSystem {
    pub(crate) fonts: FontSystem,
    pub(crate) buffers: HashMap<TextKey, Buffer>,
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
        let mut db = glyphon::fontdb::Database::new();
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

    pub(crate) fn ensure(&mut self, text: &str, font_size: f32) -> TextKey {
        self.ensure_styled(
            text,
            &TextStyle {
                font_size,
                ..TextStyle::default()
            },
            None,
        )
    }

    pub(crate) fn ensure_styled(
        &mut self,
        text: &str,
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextKey {
        self.ensure_buffer(text, &[], style, width)
    }

    /// A rich block: per-span family, weight, slant and colour over the block style.
    pub(crate) fn ensure_rich(
        &mut self,
        spans: &[TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextKey {
        let text: String = spans.iter().map(|span| span.text.as_str()).collect();
        self.ensure_buffer(&text, spans, style, width)
    }

    fn ensure_buffer(
        &mut self,
        text: &str,
        spans: &[TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextKey {
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
        let key = TextKey {
            text: text.into(),
            size: font_size.to_bits(),
            family: style.family.clone(),
            weight: style.weight.clamp(100, 900),
            italic: style.italic,
            wrap: style.wrap,
            width: width.map(f32::to_bits),
            spans: spans
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
                .collect(),
        };
        self.buffers.entry(key.clone()).or_insert_with(|| {
            let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(font_size, font_size * 1.4));
            buffer.set_wrap(
                &mut self.fonts,
                if style.wrap == TextWrap::None {
                    Wrap::None
                } else {
                    Wrap::WordOrGlyph
                },
            );
            buffer.set_size(&mut self.fonts, width, None);
            let base = attrs(&style.family, style.weight, style.italic);
            if spans.is_empty() {
                buffer.set_text(&mut self.fonts, text, &base, Shaping::Advanced, None);
            } else {
                // A span colour overrides the text area's default colour per glyph.
                let runs = spans.iter().map(|span| {
                    let run = attrs(
                        span.family.as_ref().unwrap_or(&style.family),
                        span.weight.unwrap_or(style.weight),
                        span.italic.unwrap_or(style.italic),
                    );
                    (
                        span.text.as_str(),
                        match span.color {
                            Some(color) => {
                                run.color(glyphon::Color::rgba(color.0, color.1, color.2, color.3))
                            }
                            None => run,
                        },
                    )
                });
                // cosmic-text shapes an ASCII word in one run when every explicit
                // span overlapping it is compatible, and text with the default
                // attributes is not an explicit span: "bold:" would be all bold.
                // Defaults that no run uses make every run explicit.
                let unused = base
                    .clone()
                    .weight(glyphon::Weight(style.weight.clamp(100, 900) ^ 1));
                buffer.set_rich_text(&mut self.fonts, runs, &unused, Shaping::Advanced, None);
            }
            buffer.shape_until_scroll(&mut self.fonts, false);
            buffer
        });
        key
    }

    pub(crate) fn retain(&mut self, active: &HashSet<TextKey>) {
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
        let key = self.ensure_styled(text, style, width);
        Self::size(&self.buffers[&key])
    }

    fn geometry(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> TextGeometry {
        let key = self.ensure_styled(text, style, width);
        Self::extract(&self.buffers[&key], text)
    }

    fn geometry_rich(
        &mut self,
        spans: &[TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextGeometry {
        let key = self.ensure_rich(spans, style, width);
        Self::extract(&self.buffers[&key], &key.text)
    }
}

impl TextSystem {
    /// Grapheme clusters and caret stops of a shaped buffer over `text`.
    fn extract(buffer: &Buffer, text: &str) -> TextGeometry {
        let mut result = TextGeometry {
            size: Self::size(buffer),
            ..TextGeometry::default()
        };
        let mut bases = vec![0];
        for (byte, ch) in text.char_indices() {
            if ch == '\n' {
                bases.push(byte + 1);
            }
        }
        for run in buffer.layout_runs() {
            let base = bases.get(run.line_i).copied().unwrap_or(0);
            for (byte, grapheme) in run.text.grapheme_indices(true) {
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
                let cluster_text = &run.text[glyph.start..glyph.end];
                let count = cluster_text.graphemes(true).count().max(1);
                let offset = run.text[glyph.start..byte].graphemes(true).count();
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
                    bytes: base + byte..base + byte + grapheme.len(),
                    rect,
                    rtl,
                });
                result.carets.push(CaretStop {
                    byte: base + byte,
                    rect: Rect {
                        x: if rtl { x + advance } else { x },
                        width: 1.0,
                        ..rect
                    },
                });
            }
            if run.glyphs.is_empty() {
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
    use rust_gui_scene::Point;

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
        let red = [TextSpan::new("abcd").color(rust_gui_scene::Color(255, 0, 0, 255))];
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

    #[test]
    fn generic_serif_resolves_to_an_installed_face_instead_of_sans_fallback() {
        let mut system = TextSystem::default();
        let expected = system
            .fonts
            .db()
            .query(&glyphon::fontdb::Query {
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
}
