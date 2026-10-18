//! GPU homes for scene images: small ones share atlas pages, large ones get a
//! texture of their own. Entries are keyed by image id, plus the raster size
//! for vector images, and uploaded once.
use glassmere_scene::{Image, Rect};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

/// Atlas page side, clamped to the device limit.
const PAGE_SIDE: u32 = 2048;
/// Images with a larger side get their own texture.
const ATLAS_MAX_SIDE: u32 = 512;
/// Preparations a texture survives without being drawn.
const IDLE_PREPARATIONS: u64 = 120;
pub(crate) const DEFAULT_BUDGET: u64 = 256 << 20;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

// Six full vertices per image; instance them if scenes reach
// thousands of images.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ImageVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub local: [f32; 2],
    pub size: [f32; 2],
    pub tint: [f32; 4],
    /// Corner radius, and 1 when `tint` replaces the colour.
    pub params: [f32; 2],
    /// Texel centres sampling may not leave, as uv: min x, min y, max x, max y.
    pub bounds: [f32; 4],
}

impl ImageVertex {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x2,
        4 => Float32x4, 5 => Float32x2, 6 => Float32x4
    ];
}

struct Shelf {
    y: u32,
    height: u32,
    x: u32,
}

// Shelves never free a single entry; space comes back when the whole
// page is dropped. A real allocator (guillotine, etlas) if churn shows up.
struct Shelves {
    side: u32,
    shelves: Vec<Shelf>,
}

impl Shelves {
    fn place(&mut self, width: u32, height: u32) -> Option<[u32; 2]> {
        let side = self.side;
        let fits = |shelf: &&mut Shelf| shelf.height >= height && side - shelf.x >= width;
        if let Some(shelf) = self
            .shelves
            .iter_mut()
            .filter(fits)
            .min_by_key(|shelf| shelf.height)
        {
            shelf.x += width;
            return Some([shelf.x - width, shelf.y]);
        }
        let y = self.shelves.last().map_or(0, |last| last.y + last.height);
        (width <= side && height <= side - y).then(|| {
            self.shelves.push(Shelf {
                y,
                height,
                x: width,
            });
            [0, y]
        })
    }
}

struct Texture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    size: [u32; 2],
    /// The preparation that last drew from it.
    used: u64,
    /// `Some` for an atlas page.
    shelves: Option<Shelves>,
}

impl Texture {
    fn bytes(&self) -> u64 {
        u64::from(self.size[0]) * u64::from(self.size[1]) * 4
    }
}

/// Where an image's texels live.
#[derive(Clone, Copy)]
pub(crate) struct Placement {
    pub texture: u32,
    pub texture_size: [u32; 2],
    /// x, y, width, height in texels.
    pub rect: [u32; 4],
}

pub(crate) struct ImageCache {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: BTreeMap<u32, Texture>,
    entries: HashMap<(u64, [u32; 2]), Placement>,
    next_texture: u32,
    preparation: u64,
    max_side: u32,
    pub budget: u64,
    pub uploads: u64,
}

impl ImageCache {
    pub fn new(
        device: &wgpu::Device,
        layout: wgpu::BindGroupLayout,
        sampler: wgpu::Sampler,
    ) -> Self {
        Self {
            layout,
            sampler,
            textures: BTreeMap::new(),
            entries: HashMap::new(),
            next_texture: 0,
            preparation: 0,
            max_side: device.limits().max_texture_dimension_2d,
            budget: DEFAULT_BUDGET,
            uploads: 0,
        }
    }

    /// Starts a preparation: drops what has not been drawn for a while, or, over
    /// budget, everything the previous preparation did not draw.
    pub fn begin(&mut self) {
        self.preparation += 1;
        let preparation = self.preparation;
        let idle = if self.bytes() > self.budget {
            1
        } else {
            IDLE_PREPARATIONS
        };
        self.textures
            .retain(|_, texture| preparation - texture.used <= idle);
        if self.bytes() > self.budget {
            // Pages are dropped whole and refilled by this preparation,
            // which sheds stale vector rasters; a working set that is itself over
            // budget re-uploads every preparation. Per-entry eviction if that happens.
            self.textures.retain(|_, texture| texture.shelves.is_none());
        }
        self.forget_dropped();
    }

    fn forget_dropped(&mut self) {
        let textures = &self.textures;
        self.entries
            .retain(|_, placement| textures.contains_key(&placement.texture));
    }

    pub fn bytes(&self) -> u64 {
        self.textures.values().map(Texture::bytes).sum()
    }

    /// Bytes in atlas pages, and in standalone textures.
    pub fn bytes_by_kind(&self) -> (u64, u64) {
        let pages = self
            .textures
            .values()
            .filter(|texture| texture.shelves.is_some())
            .map(Texture::bytes)
            .sum();
        (pages, self.bytes() - pages)
    }

    /// Atlas pages and standalone textures alive.
    pub fn counts(&self) -> (usize, usize) {
        let pages = self
            .textures
            .values()
            .filter(|texture| texture.shelves.is_some())
            .count();
        (pages, self.textures.len() - pages)
    }

    pub fn bind_group(&self, texture: u32) -> &wgpu::BindGroup {
        &self.textures[&texture].bind_group
    }

    /// Pixels a vector image is rasterized at so that `source` fills `rect` on a
    /// surface of this scale. Raster images ignore it.
    pub fn raster_size(&self, image: &Image, rect: Rect, source: Rect, scale: f32) -> [u32; 2] {
        if !image.is_vector() {
            return [0; 2];
        }
        let natural = image.size();
        let side = |natural: f32, rect: f32, source: f32| {
            let pixels = (natural * rect / source.abs() * scale).ceil();
            if pixels.is_finite() {
                (pixels.min(self.max_side as f32) as u32).max(1)
            } else {
                1
            }
        };
        [
            side(natural.width, rect.width, source.width),
            side(natural.height, rect.height, source.height),
        ]
    }

    /// The image's place on the GPU, uploading it on first use. `None` when a
    /// vector image returns a buffer of the wrong size.
    pub fn ensure(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &Image,
        raster: [u32; 2],
    ) -> Option<Placement> {
        let key = (image.id(), raster);
        if let Some(placement) = self.entries.get(&key) {
            self.textures.get_mut(&placement.texture)?.used = self.preparation;
            return Some(*placement);
        }
        let (mut width, mut height, mut pixels) = match image.vector() {
            // Rasterized on the render thread; a large SVG animating its
            // size stalls frames. Rasterize off-thread and draw the stale size
            // meanwhile if that shows up.
            Some(vector) => {
                let pixels = vector.rasterize(raster[0], raster[1]);
                (raster[0], raster[1], Cow::Owned(pixels))
            }
            None => {
                let (width, height, pixels) = image.pixels()?;
                (width, height, Cow::Borrowed(pixels))
            }
        };
        if pixels.len() != width as usize * height as usize * 4 {
            return None;
        }
        // Photos beyond the device's texture limit are halved until they fit.
        while width.max(height) > self.max_side {
            (width, height, pixels) = halved(width, height, &pixels);
        }
        let pixels = linear_premultiplied(pixels);
        let placement = if width.max(height) <= ATLAS_MAX_SIDE {
            self.place_in_atlas(device, queue, width, height, &pixels)
        } else {
            let texture = self.create(device, [width, height], None);
            self.write(queue, texture, [0, 0], [width, height], &pixels);
            Placement {
                texture,
                texture_size: [width, height],
                rect: [0, 0, width, height],
            }
        };
        self.uploads += 1;
        self.entries.insert(key, placement);
        Some(placement)
    }

    fn place_in_atlas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Placement {
        // One texel of the image's own border on every side: linear sampling at
        // an edge never reaches a neighbour.
        let padded = [width + 2, height + 2];
        let mut extruded = Vec::with_capacity(padded[0] as usize * padded[1] as usize * 4);
        for y in 0..padded[1] {
            let row = (y.clamp(1, height) - 1) as usize * width as usize;
            for x in 0..padded[0] {
                let texel = (row + (x.clamp(1, width) - 1) as usize) * 4;
                extruded.extend_from_slice(&pixels[texel..texel + 4]);
            }
        }
        let preparation = self.preparation;
        let found = self.textures.iter_mut().find_map(|(id, texture)| {
            let at = texture.shelves.as_mut()?.place(padded[0], padded[1])?;
            texture.used = preparation;
            Some((*id, at))
        });
        let (texture, at) = found.unwrap_or_else(|| {
            let side = PAGE_SIDE.min(self.max_side);
            if self.bytes() + u64::from(side) * u64::from(side) * 4 > self.budget {
                // Nothing prepared so far points into pages this preparation has
                // not touched, so they can go before another page is added.
                self.textures
                    .retain(|_, texture| texture.shelves.is_none() || texture.used == preparation);
                self.forget_dropped();
            }
            let mut shelves = Shelves {
                side,
                shelves: Vec::new(),
            };
            let at = shelves
                .place(padded[0], padded[1])
                .expect("an atlas image fits an empty page");
            (self.create(device, [side; 2], Some(shelves)), at)
        });
        self.write(queue, texture, at, padded, &extruded);
        Placement {
            texture,
            texture_size: self.textures[&texture].size,
            rect: [at[0] + 1, at[1] + 1, width, height],
        }
    }

    fn create(&mut self, device: &wgpu::Device, size: [u32; 2], shelves: Option<Shelves>) -> u32 {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glassmere images"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glassmere images"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let id = self.next_texture;
        self.next_texture += 1;
        self.textures.insert(
            id,
            Texture {
                texture,
                bind_group,
                size,
                used: self.preparation,
                shelves,
            },
        );
        id
    }

    fn write(
        &self,
        queue: &wgpu::Queue,
        texture: u32,
        at: [u32; 2],
        size: [u32; 2],
        pixels: &[u8],
    ) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.textures[&texture].texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: at[0],
                    y: at[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size[0] * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
        );
    }
}

/// A 2×2 box filter; odd trailing rows and columns repeat their last texel.
fn halved(width: u32, height: u32, pixels: &[u8]) -> (u32, u32, Cow<'static, [u8]>) {
    let (w, h) = (width.div_ceil(2), height.div_ceil(2));
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            for channel in 0..4 {
                let at = |x: u32, y: u32| {
                    let (x, y) = (x.min(width - 1) as usize, y.min(height - 1) as usize);
                    u32::from(pixels[(y * width as usize + x) * 4 + channel])
                };
                let sum = at(2 * x, 2 * y)
                    + at(2 * x + 1, 2 * y)
                    + at(2 * x, 2 * y + 1)
                    + at(2 * x + 1, 2 * y + 1);
                out.push(((sum + 2) / 4) as u8);
            }
        }
    }
    (w, h, Cow::Owned(out))
}

/// Scene images are premultiplied in sRGB space; blending here is linear. The
/// sRGB texture format decodes on sampling, so translucent texels are stored as
/// the sRGB encoding of their premultiplied *linear* colour. Opaque and fully
/// transparent texels already are.
fn linear_premultiplied(pixels: Cow<'_, [u8]>) -> Cow<'_, [u8]> {
    let settled = |pixel: &[u8; 4]| matches!(pixel[3], 0 | 255);
    if pixels.as_chunks::<4>().0.iter().all(settled) {
        return pixels;
    }
    let decode: Vec<f32> = (0..=255u8)
        .map(|value| glassmere_scene::Color(value, 0, 0, 255).linear()[0])
        .collect();
    let mut pixels = pixels.into_owned();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = pixel[3];
        if settled(pixel) {
            continue;
        }
        for channel in &mut pixel[..3] {
            let straight = (u32::from(*channel) * 255 / u32::from(alpha)).min(255) as usize;
            let linear = decode[straight] * f32::from(alpha) / 255.0;
            let encoded = if linear <= 0.003_130_8 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            };
            *channel = (encoded * 255.0).round() as u8;
        }
    }
    Cow::Owned(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelves_pack_without_overlap_until_the_page_is_full() {
        let mut shelves = Shelves {
            side: 64,
            shelves: Vec::new(),
        };
        let mut placed: Vec<[u32; 4]> = Vec::new();
        for size in [[30, 20], [30, 10], [10, 20], [40, 40], [24, 4], [64, 5]] {
            if let Some([x, y]) = shelves.place(size[0], size[1]) {
                assert!(x + size[0] <= 64 && y + size[1] <= 64);
                for other in &placed {
                    let apart = x >= other[0] + other[2]
                        || other[0] >= x + size[0]
                        || y >= other[1] + other[3]
                        || other[1] >= y + size[1];
                    assert!(apart, "{size:?} at {x},{y} overlaps {other:?}");
                }
                placed.push([x, y, size[0], size[1]]);
            }
        }
        // Only the 40×40 block is refused: 24 rows remain under two 20-tall shelves.
        assert_eq!(placed.len(), 5);
        assert!(shelves.place(65, 1).is_none());
    }

    #[test]
    fn translucent_texels_are_reencoded_and_others_kept() {
        let kept = linear_premultiplied(Cow::Borrowed(&[200, 100, 0, 255, 0, 0, 0, 0]));
        assert!(matches!(kept, Cow::Borrowed(_)));
        // Opaque white at half alpha: linear 0.5 premultiplied, sRGB-encoded.
        let half = linear_premultiplied(Cow::Borrowed(&[128, 128, 128, 128]));
        assert_eq!(half[3], 128);
        assert!(half[0].abs_diff(188) <= 1, "{}", half[0]);
    }

    #[test]
    fn halving_averages_blocks_and_keeps_odd_edges() {
        let (w, h, out) = halved(3, 1, &[0, 0, 0, 255, 100, 0, 0, 255, 50, 0, 0, 255]);
        assert_eq!((w, h), (2, 1));
        assert_eq!(out[0], 50);
        assert_eq!(out[4], 50);
    }
}
