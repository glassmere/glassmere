//! Text measurement wrapped for timing, and how an element becomes a Taffy
//! style and a measured leaf.

use super::*;

/// The application's measurer, timed. Wrapping it rather than the call sites
/// means every path through the framework is counted, including the ones
/// layout takes on its own.
pub(super) struct Timed<'a, M> {
    pub(super) inner: &'a mut M,
    pub(super) spent: Duration,
}

impl<M: TextMeasurer> TextMeasurer for Timed<'_, M> {
    fn measure_styled(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> Size {
        let started = Instant::now();
        let measured = self.inner.measure_styled(text, style, width);
        self.spent += started.elapsed();
        measured
    }

    fn geometry(
        &mut self,
        text: &str,
        style: &TextStyle,
        width: Option<f32>,
    ) -> seaglass_scene::TextGeometry {
        let started = Instant::now();
        let geometry = self.inner.geometry(text, style, width);
        self.spent += started.elapsed();
        geometry
    }

    fn geometry_rich(
        &mut self,
        spans: &[crate::TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> seaglass_scene::TextGeometry {
        let started = Instant::now();
        let geometry = self.inner.geometry_rich(spans, style, width);
        self.spent += started.elapsed();
        geometry
    }

    fn ascent(&mut self, style: &TextStyle) -> f32 {
        let started = Instant::now();
        let ascent = self.inner.ascent(style);
        self.spent += started.elapsed();
        ascent
    }

    fn measure(&mut self, text: &str, font_size: f32) -> Size {
        let started = Instant::now();
        let measured = self.inner.measure(text, font_size);
        self.spent += started.elapsed();
        measured
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Measurement {
    pub(super) size: Size,
    pub(super) wrapped: Option<(String, TextStyle)>,
    /// Rich blocks measure their styled runs instead of the plain text.
    pub(super) spans: Option<Vec<seaglass_scene::TextSpan>>,
}

pub(super) fn layout_style(style: &Style, kind: Kind, direction: crate::Direction) -> taffy::Style {
    let mut result = style.layout(match kind {
        Kind::Row | Kind::SplitRow | Kind::TabList | Kind::Toast => Flow::Row,
        Kind::Grid => Flow::Grid,
        _ => Flow::Column,
    });
    let rtl = direction == crate::Direction::Rtl;
    if rtl {
        result.direction = taffy::Direction::Rtl;
    }
    if matches!(kind, Kind::Popup | Kind::Popover) {
        result.position = taffy::Position::Absolute;
        result.inset.top = taffy::LengthPercentageAuto::percent(1.0);
        // Overlays hang from the start side of their owner.
        if rtl {
            result.inset.right = taffy::LengthPercentageAuto::length(0.0);
        } else {
            result.inset.left = taffy::LengthPercentageAuto::length(0.0);
        }
    }
    if kind == Kind::Toasts {
        // The bottom corner at the end of the reading direction.
        result.position = taffy::Position::Absolute;
        result.inset.bottom = taffy::LengthPercentageAuto::length(16.0);
        if rtl {
            result.inset.left = taffy::LengthPercentageAuto::length(16.0);
        } else {
            result.inset.right = taffy::LengthPercentageAuto::length(16.0);
        }
    }
    if kind == Kind::Dialog {
        result.position = taffy::Position::Absolute;
        result.inset.top = taffy::LengthPercentageAuto::length(0.0);
        result.inset.left = taffy::LengthPercentageAuto::length(0.0);
    }
    result
}

/// A text leaf that anchors an overlay (popup or popover) gains children, and
/// Taffy then ignores its measure context; keep its intrinsic size as a minimum.
pub(super) fn anchor_minimum(style: &mut taffy::Style, element: &Element, measured: Size) {
    if element.kind.text_leaf() && !element.children.is_empty() {
        let padding = element.style.padding.max(0.0) * 2.0;
        style.min_size = taffy::Size {
            width: taffy::Dimension::length(measured.width + padding),
            height: taffy::Dimension::length(measured.height + padding),
        };
    }
}

impl Measurement {
    pub(super) fn fixed(size: Size) -> Self {
        Self {
            size,
            wrapped: None,
            spans: None,
        }
    }
    pub(super) fn text(
        text: &str,
        text_style: TextStyle,
        measurer: &mut impl TextMeasurer,
    ) -> Self {
        Self {
            size: measurer.measure_styled(text, &text_style, None),
            wrapped: (text_style.wrap == TextWrap::Word).then(|| (text.to_owned(), text_style)),
            spans: None,
        }
    }
    /// An image's natural size; nothing until a file has loaded.
    pub(super) fn image(element: &Element) -> Option<Self> {
        let Control::Image { image, .. } = &element.control else {
            return None;
        };
        Some(Self::fixed(
            image
                .as_ref()
                .map_or(Size::default(), seaglass_scene::Image::size),
        ))
    }
    /// A text leaf: plain, or the styled runs of a rich block.
    pub(super) fn leaf(
        element: &Element,
        text_style: TextStyle,
        measurer: &mut impl TextMeasurer,
    ) -> Self {
        let Control::RichText { runs, .. } = &element.control else {
            return Self::text(&element.text, text_style, measurer);
        };
        let spans = rich_text::spans(runs, Color::TRANSPARENT);
        Self {
            size: measurer.geometry_rich(&spans, &text_style, None).size,
            wrapped: (text_style.wrap == TextWrap::Word).then(|| (String::new(), text_style)),
            spans: Some(spans),
        }
    }
}
