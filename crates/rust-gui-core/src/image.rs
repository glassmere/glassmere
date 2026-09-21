use crate::Style;
use rust_gui_scene::{Color, Rect, Size};

/// How an image fills a box whose shape differs from its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
}
