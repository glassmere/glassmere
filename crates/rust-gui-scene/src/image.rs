//! Images as the scene sees them: decoded pixels or a vector source that a
//! renderer rasterizes at the size it is drawn. Decoding lives elsewhere.
use crate::Size;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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
            if self.is_vector() { " vector" } else { "" }
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

    /// Unique per image, stable across clones; renderers key their caches on it.
    pub fn id(&self) -> u64 {
        self.0.id
    }

    /// Natural size: one logical pixel per image pixel, or the vector's size.
    pub fn size(&self) -> Size {
        match &self.0.source {
            Source::Pixels { width, height, .. } => Size::new(*width as f32, *height as f32),
            Source::Vector(vector) => vector.size(),
        }
    }

    pub fn is_vector(&self) -> bool {
        matches!(self.0.source, Source::Vector(_))
    }

    /// Width, height and premultiplied RGBA8 of a raster image.
    pub fn pixels(&self) -> Option<(u32, u32, &[u8])> {
        match &self.0.source {
            Source::Pixels {
                width,
                height,
                rgba,
            } => Some((*width, *height, rgba)),
            Source::Vector(_) => None,
        }
    }

    pub fn vector(&self) -> Option<&dyn VectorImage> {
        match &self.0.source {
            Source::Vector(vector) => Some(vector.as_ref()),
            Source::Pixels { .. } => None,
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
}
