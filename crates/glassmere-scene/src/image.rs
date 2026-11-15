//! Images as the scene sees them: decoded pixels or a vector source that a
//! renderer rasterizes at the size it is drawn. Decoding lives elsewhere.
use crate::Size;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// A resolution-independent image. Renderers call `rasterize` for the pixel
/// size they need and cache the result per image and size.
pub trait VectorImage: Send + Sync {
    /// Natural size in logical pixels.
    fn size(&self) -> Size;
    /// Premultiplied RGBA8, `width * height * 4` bytes in row order, with the
    /// whole image scaled to fill `width` by `height`.
    fn rasterize(&self, width: u32, height: u32) -> Vec<u8>;
}

enum Source {
    Pixels {
        width: u32,
        height: u32,
        /// Premultiplied RGBA8 in row order.
        rgba: Vec<u8>,
    },
    Vector(Box<dyn VectorImage>),
    Animated {
        frames: Vec<ImageFrame>,
        loops: Option<NonZeroU32>,
    },
}

/// One still of an animated image and how long it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageFrame {
    /// A raster image that is not itself animated.
    pub image: Image,
    pub delay: Duration,
}

struct Inner {
    id: u64,
    source: Source,
}

/// A cheaply cloned, immutable image. Two handles are equal when they are the
/// same image, which is what style and scene comparisons need.
#[derive(Clone)]
pub struct Image(Arc<Inner>);

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size = self.size();
        write!(
            f,
            "Image#{} {}x{}{}",
            self.0.id,
            size.width,
            size.height,
            if self.is_vector() {
                " vector"
            } else if !self.frames().is_empty() {
                " animated"
            } else {
                ""
            }
        )
    }
}

fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl Image {
    /// From straight (not premultiplied) RGBA8, as decoders produce. `None`
    /// when the buffer does not hold `width * height` pixels or a side is zero.
    pub fn from_rgba8(width: u32, height: u32, mut rgba: Vec<u8>) -> Option<Self> {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let alpha = u16::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
            }
        }
        Self::from_premultiplied(width, height, rgba)
    }

    pub fn from_premultiplied(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        (width > 0 && height > 0 && rgba.len() == expected).then(|| {
            Self(Arc::new(Inner {
                id: next_id(),
                source: Source::Pixels {
                    width,
                    height,
                    rgba,
                },
            }))
        })
    }

    pub fn from_vector(vector: impl VectorImage + 'static) -> Self {
        Self(Arc::new(Inner {
            id: next_id(),
            source: Source::Vector(Box::new(vector)),
        }))
    }

    /// Stills shown in turn, each for its delay, the whole sequence `loops`
    /// times (`None`: forever); it then stays on the last frame. `None` when
    /// there are no frames, when a frame is not a raster still or differs in
    /// size from the first, or when the delays add up to nothing.
    ///
    /// Everything that is not an animation player sees the first frame: the
    /// size and `pixels` are its own, and a renderer asked to draw this image
    /// draws it. `Ui::image` plays the animation.
    pub fn animated(frames: Vec<ImageFrame>, loops: Option<NonZeroU32>) -> Option<Self> {
        let size = |frame: &ImageFrame| match &frame.image.0.source {
            Source::Pixels { width, height, .. } => Some((*width, *height)),
            _ => None,
        };
        let first = size(frames.first()?)?;
        let uniform = frames.iter().all(|frame| size(frame) == Some(first));
        let lasting = frames.iter().any(|frame| !frame.delay.is_zero());
        (uniform && lasting).then(|| {
            Self(Arc::new(Inner {
                id: next_id(),
                source: Source::Animated { frames, loops },
            }))
        })
    }

    /// The frames of an animated image in play order; empty for any other.
    pub fn frames(&self) -> &[ImageFrame] {
        match &self.0.source {
            Source::Animated { frames, .. } => frames,
            _ => &[],
        }
    }

    /// How many times an animated image plays through; `None` when it plays
    /// forever, and for an image that is not animated.
    pub fn loops(&self) -> Option<NonZeroU32> {
        match &self.0.source {
            Source::Animated { loops, .. } => *loops,
            _ => None,
        }
    }

    /// Unique per image, stable across clones; renderers key their caches on it.
    pub fn id(&self) -> u64 {
        self.0.id
    }

    /// Natural size: one logical pixel per image pixel, or the vector's size.
    pub fn size(&self) -> Size {
        match &self.0.source {
            Source::Pixels { width, height, .. } => Size::new(*width as f32, *height as f32),
            Source::Vector(vector) => vector.size(),
            Source::Animated { frames, .. } => frames[0].image.size(),
        }
    }

    pub fn is_vector(&self) -> bool {
        matches!(self.0.source, Source::Vector(_))
    }

    /// Width, height and premultiplied RGBA8 of a raster image; the first
    /// frame of an animated one.
    pub fn pixels(&self) -> Option<(u32, u32, &[u8])> {
        match &self.0.source {
            Source::Pixels {
                width,
                height,
                rgba,
            } => Some((*width, *height, rgba)),
            Source::Vector(_) => None,
            Source::Animated { frames, .. } => frames[0].image.pixels(),
        }
    }

    pub fn vector(&self) -> Option<&dyn VectorImage> {
        match &self.0.source {
            Source::Vector(vector) => Some(vector.as_ref()),
            Source::Pixels { .. } | Source::Animated { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Flat;
    impl VectorImage for Flat {
        fn size(&self) -> Size {
            Size::new(24.0, 12.0)
        }
        fn rasterize(&self, width: u32, height: u32) -> Vec<u8> {
            vec![255; (width * height * 4) as usize]
        }
    }

    #[test]
    fn images_premultiply_validate_and_compare_by_identity() {
        let image = Image::from_rgba8(1, 2, vec![255, 128, 0, 128, 10, 20, 30, 0]).unwrap();
        assert_eq!(image.pixels().unwrap().2, [128, 64, 0, 128, 0, 0, 0, 0]);
        assert_eq!(image.size(), Size::new(1.0, 2.0));
        assert_eq!(image, image.clone());
        let twin = Image::from_rgba8(1, 2, vec![255, 128, 0, 128, 10, 20, 30, 0]).unwrap();
        assert_ne!(image, twin);
        assert!(Image::from_rgba8(2, 2, vec![0; 15]).is_none());
        assert!(Image::from_rgba8(0, 2, Vec::new()).is_none());
        let vector = Image::from_vector(Flat);
        assert!(vector.is_vector() && vector.pixels().is_none());
        assert_eq!(vector.size(), Size::new(24.0, 12.0));
        assert_eq!(vector.vector().unwrap().rasterize(2, 1).len(), 8);
    }

    #[test]
    fn animations_need_same_sized_raster_stills_and_a_duration() {
        let still =
            |width, shade: u8| Image::from_premultiplied(width, 1, vec![shade; width as usize * 4]);
        let frame = |image: Option<Image>, millis| ImageFrame {
            image: image.unwrap(),
            delay: Duration::from_millis(millis),
        };
        let frames = || vec![frame(still(2, 10), 40), frame(still(2, 20), 60)];
        let image = Image::animated(frames(), NonZeroU32::new(3)).unwrap();
        // Everything that does not play it sees the first frame.
        assert_eq!(image.size(), Size::new(2.0, 1.0));
        assert_eq!(image.pixels().unwrap().2, [10; 8]);
        assert!(image.vector().is_none() && !image.is_vector());
        assert_eq!(image.frames().len(), 2);
        assert_eq!(image.frames()[1].delay, Duration::from_millis(60));
        assert_eq!(image.loops(), NonZeroU32::new(3));
        assert_eq!(Image::animated(frames(), None).unwrap().loops(), None);
        let flat = still(2, 10).unwrap();
        assert!(flat.frames().is_empty() && flat.loops().is_none());

        assert!(Image::animated(Vec::new(), None).is_none());
        let mixed = vec![frame(still(2, 10), 40), frame(still(3, 10), 40)];
        assert!(Image::animated(mixed, None).is_none());
        assert!(Image::animated(vec![frame(still(2, 10), 0)], None).is_none());
        let nested = vec![frame(Some(image), 40)];
        assert!(Image::animated(nested, None).is_none());
        let vector = vec![frame(Some(Image::from_vector(Flat)), 40)];
        assert!(Image::animated(vector, None).is_none());
    }
}
