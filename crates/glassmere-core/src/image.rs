use crate::Style;
use glassmere_scene::{Color, Image, Rect, Size};
use std::path::PathBuf;

/// How an image fills a box whose shape differs from its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageFit {
    /// Scaled to fit inside the box, centred, aspect ratio kept.
    #[default]
    Contain,
    /// Scaled to cover the box, centred, the overflow cropped.
    Cover,
    /// Stretched to the box.
    Fill,
    /// Natural size, centred, cropped where it is larger than the box.
    None,
}

/// Recolouring for single-colour images such as icons: the image's alpha is
/// kept and its colour replaced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageTint {
    #[default]
    None,
    /// The inherited text colour, so an icon follows themes and states.
    Foreground,
    Color(Color),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImageOptions {
    /// Accessible name. Empty marks the image as decorative: assistive
    /// technology skips it.
    pub alt: String,
    pub fit: ImageFit,
    pub tint: ImageTint,
    /// Without a size the image takes its natural size; with only a width or
    /// only a height the other side follows the aspect ratio. `radius` rounds
    /// the image, `opacity` fades it.
    pub style: Style,
}

impl ImageOptions {
    pub fn new(alt: impl Into<String>) -> Self {
        Self {
            alt: alt.into(),
            ..Self::default()
        }
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn tint(mut self, tint: ImageTint) -> Self {
        self.tint = tint;
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

/// Where a background image comes from.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ImageSource {
    Image(Image),
    /// Loaded by the host like `Ui::image_file`; nothing is painted until it
    /// arrives.
    File(PathBuf),
}

/// An image painted over a node's background colour and under its content,
/// across the node's whole bounds.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundImage {
    pub source: ImageSource,
    /// `Cover` unless set. Ignored with `slice`.
    pub fit: ImageFit,
    /// Nine-slice insets from the image's top, right, bottom and left edges,
    /// in image units. The four corners keep their size, the edges stretch
    /// along one axis and the centre along both, so a small frame image fits
    /// a box of any size. The node's `radius` does not apply.
    pub slice: Option<[f32; 4]>,
    pub tint: ImageTint,
}

impl BackgroundImage {
    pub fn new(image: &Image) -> Self {
        Self::from_source(ImageSource::Image(image.clone()))
    }

    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::from_source(ImageSource::File(path.into()))
    }

    fn from_source(source: ImageSource) -> Self {
        Self {
            source,
            fit: ImageFit::Cover,
            slice: None,
            tint: ImageTint::None,
        }
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn slice(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.slice = Some([top, right, bottom, left]);
        self
    }

    pub fn tint(mut self, tint: ImageTint) -> Self {
        self.tint = tint;
        self
    }
}

/// Destination and source rectangles of the up to nine pieces of a sliced
/// image of `natural` size stretched over `bounds`.
pub(crate) fn nine_slice(slice: [f32; 4], natural: Size, bounds: Rect) -> Vec<(Rect, Rect)> {
    let [top, right, bottom, left] = slice.map(|inset| inset.max(0.0));
    // Insets that overlap in the image leave nothing to stretch: clamp them.
    let fit = |first: f32, second: f32, room: f32| {
        let total = first + second;
        if total > room && total > 0.0 {
            (first * room / total, second * room / total)
        } else {
            (first, second)
        }
    };
    let (left, right) = fit(left, right, natural.width);
    let (top, bottom) = fit(top, bottom, natural.height);
    // On screen the borders keep their size, shrinking only in a box too small for them.
    let (screen_left, screen_right) = fit(left, right, bounds.width);
    let (screen_top, screen_bottom) = fit(top, bottom, bounds.height);
    let columns = [
        (0.0, left, 0.0, screen_left),
        (
            left,
            natural.width - left - right,
            screen_left,
            bounds.width - screen_left - screen_right,
        ),
        (
            natural.width - right,
            right,
            bounds.width - screen_right,
            screen_right,
        ),
    ];
    let rows = [
        (0.0, top, 0.0, screen_top),
        (
            top,
            natural.height - top - bottom,
            screen_top,
            bounds.height - screen_top - screen_bottom,
        ),
        (
            natural.height - bottom,
            bottom,
            bounds.height - screen_bottom,
            screen_bottom,
        ),
    ];
    let mut pieces = Vec::with_capacity(9);
    for (source_y, source_height, y, height) in rows {
        for (source_x, source_width, x, width) in columns {
            if source_width > 0.0 && source_height > 0.0 && width > 0.0 && height > 0.0 {
                pieces.push((
                    Rect {
                        x: bounds.x + x,
                        y: bounds.y + y,
                        width,
                        height,
                    },
                    Rect {
                        x: source_x,
                        y: source_y,
                        width: source_width,
                        height: source_height,
                    },
                ));
            }
        }
    }
    pieces
}

impl ImageFit {
    /// Where an image of `natural` size lands in `bounds`: the destination
    /// rectangle and the part of the image (in its own units) drawn there.
    pub(crate) fn place(self, natural: Size, bounds: Rect) -> Option<(Rect, Rect)> {
        let positive = |size: f32| size.is_finite() && size > 0.0;
        if ![natural.width, natural.height, bounds.width, bounds.height]
            .into_iter()
            .all(positive)
        {
            return None;
        }
        let ratios = (bounds.width / natural.width, bounds.height / natural.height);
        let scale = match self {
            Self::Fill => {
                let source = Rect {
                    x: 0.0,
                    y: 0.0,
                    width: natural.width,
                    height: natural.height,
                };
                return Some((bounds, source));
            }
            Self::Contain => ratios.0.min(ratios.1),
            Self::Cover => ratios.0.max(ratios.1),
            Self::None => 1.0,
        };
        // The scaled image, cropped to the box on each axis and centred in both.
        let width = (natural.width * scale).min(bounds.width);
        let height = (natural.height * scale).min(bounds.height);
        let destination = Rect {
            x: bounds.x + (bounds.width - width) / 2.0,
            y: bounds.y + (bounds.height - height) / 2.0,
            width,
            height,
        };
        let source = Rect {
            x: (natural.width - width / scale) / 2.0,
            y: (natural.height - height / scale) / 2.0,
            width: width / scale,
            height: height / scale,
        };
        Some((destination, source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_modes_place_a_wide_image_in_a_square_box() {
        let natural = Size::new(200.0, 100.0);
        let bounds = Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 100.0,
        };
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        let full = rect(0.0, 0.0, 200.0, 100.0);
        assert_eq!(
            ImageFit::Contain.place(natural, bounds),
            Some((rect(10.0, 45.0, 100.0, 50.0), full))
        );
        assert_eq!(
            ImageFit::Cover.place(natural, bounds),
            Some((bounds, rect(50.0, 0.0, 100.0, 100.0)))
        );
        assert_eq!(ImageFit::Fill.place(natural, bounds), Some((bounds, full)));
        assert_eq!(
            ImageFit::None.place(natural, bounds),
            Some((bounds, rect(50.0, 0.0, 100.0, 100.0)))
        );
        // Smaller than the box: natural size, centred.
        assert_eq!(
            ImageFit::None.place(Size::new(40.0, 20.0), bounds),
            Some((rect(40.0, 60.0, 40.0, 20.0), rect(0.0, 0.0, 40.0, 20.0)))
        );
        assert_eq!(
            ImageFit::Cover.place(natural, rect(0.0, 0.0, 0.0, 9.0)),
            None
        );
    }

    #[test]
    fn nine_slice_keeps_corners_stretches_edges_and_survives_small_boxes() {
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        let natural = Size::new(30.0, 30.0);
        let pieces = nine_slice(
            [10.0, 10.0, 10.0, 10.0],
            natural,
            rect(100.0, 50.0, 200.0, 80.0),
        );
        assert_eq!(pieces.len(), 9);
        // Top-left corner: same size on screen as in the image.
        assert_eq!(
            pieces[0],
            (rect(100.0, 50.0, 10.0, 10.0), rect(0.0, 0.0, 10.0, 10.0))
        );
        // Top edge: stretched along x only.
        assert_eq!(
            pieces[1],
            (rect(110.0, 50.0, 180.0, 10.0), rect(10.0, 0.0, 10.0, 10.0))
        );
        // Centre: stretched along both.
        assert_eq!(
            pieces[4],
            (rect(110.0, 60.0, 180.0, 60.0), rect(10.0, 10.0, 10.0, 10.0))
        );
        // Bottom-right corner.
        assert_eq!(
            pieces[8],
            (rect(290.0, 120.0, 10.0, 10.0), rect(20.0, 20.0, 10.0, 10.0))
        );

        // A box narrower than the two side borders: they share it, no centre column.
        let narrow = nine_slice(
            [10.0, 10.0, 10.0, 10.0],
            natural,
            rect(0.0, 0.0, 10.0, 80.0),
        );
        assert_eq!(narrow.len(), 6);
        assert_eq!(narrow[0].0, rect(0.0, 0.0, 5.0, 10.0));
        assert_eq!(narrow[1].0, rect(5.0, 0.0, 5.0, 10.0));

        // Insets larger than the image are scaled to it; only edges that exist are drawn.
        let greedy = nine_slice([0.0, 40.0, 0.0, 20.0], natural, rect(0.0, 0.0, 90.0, 30.0));
        assert_eq!(greedy.len(), 2);
        assert_eq!(greedy[0].1, rect(0.0, 0.0, 10.0, 30.0));
        assert_eq!(greedy[1].1, rect(10.0, 0.0, 20.0, 30.0));
    }
}
