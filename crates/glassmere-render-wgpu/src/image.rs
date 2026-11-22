//! GPU homes for scene images: small ones share atlas pages, large ones get a
//! texture of their own. Entries are keyed by image id, plus the raster size
//! for vector images and for photos drawn at under half their size, and
//! uploaded once.
use glassmere_scene::{Image, Rect};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

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
    premultiply: Premultiply,
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
            premultiply: Premultiply::new(),
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
    /// surface of this scale. A photo drawn at under half its size is filtered
    /// down to the smallest halving that still covers them, so that linear
    /// sampling never shrinks it by more than two; any other photo is `[0; 2]`
    /// and keeps its own pixels.
    pub fn raster_size(&self, image: &Image, rect: Rect, source: Rect, scale: f32) -> [u32; 2] {
        let natural = image.size();
        let drawn = |natural: f32, rect: f32, source: f32| natural * rect / source.abs() * scale;
        if let Some((width, height, _)) = image.pixels() {
            let halved = [
                halved_side(width, drawn(natural.width, rect.width, source.width)),
                halved_side(height, drawn(natural.height, rect.height, source.height)),
            ];
            return if halved == [width, height] {
                [0; 2]
            } else {
                halved
            };
        }
        let side = |natural: f32, rect: f32, source: f32| {
            let pixels = drawn(natural, rect, source).ceil();
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
        let (width, height, pixels) = match image.vector() {
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
        // A photo drawn small comes down to the size `raster_size` chose; one
        // beyond the device's texture limit is halved until it fits.
        let target = if raster == [0; 2] {
            [width, height]
        } else {
            raster
        };
        let ([width, height], pixels) = raster_pixels(
            pixels,
            [width, height],
            target,
            self.max_side,
            &mut self.premultiply,
            tables(),
        );
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

/// `side` halved, rounding up, for as long as the result still covers `drawn`
/// pixels: what is left is under twice what is drawn.
fn halved_side(mut side: u32, drawn: f32) -> u32 {
    while side > 1 && side.div_ceil(2) as f32 >= drawn {
        side = side.div_ceil(2);
    }
    side
}

/// The tables that filtering and converting read.
struct Tables {
    /// Linear light of a byte in 12 bits, as the CPU renderer holds it.
    decode: [u16; 256],
    /// 12 bits of linear light to a byte.
    encode: [u8; 4096],
    /// Linear light of a byte.
    light: [f32; 256],
}

/// The tables, worked out on first use: they cost thousands of `powf`.
fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let light: [f32; 256] =
            std::array::from_fn(|byte| glassmere_scene::Color(byte as u8, 0, 0, 255).linear()[0]);
        Tables {
            decode: std::array::from_fn(|byte| (light[byte] * 4095.0).round() as u16),
            encode: std::array::from_fn(|light| encoded(light as f32 / 4095.0)),
            light,
        }
    })
}

/// A linear light as the byte of its sRGB encoding.
fn encoded(linear: f32) -> u8 {
    let encoded = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

impl Tables {
    /// A colour channel of a texel with `alpha`, neither 0 nor 255, as it is
    /// stored once premultiplied: straight, in linear light under the alpha,
    /// sRGB-encoded again.
    fn premultiplied(&self, channel: u8, alpha: u8) -> u8 {
        let straight = (u32::from(channel) * 255 / u32::from(alpha)).min(255) as usize;
        encoded(self.light[straight] * f32::from(alpha) / 255.0)
    }
}

/// What a translucent texel's colour channel is converted to, for every
/// channel and alpha that has come up. The result depends on those two bytes
/// alone, and a photo has a few distinct ones among millions of texels.
struct Premultiply {
    /// By `alpha << 8 | channel`; empty until the first conversion.
    bytes: Vec<u16>,
    /// How many conversions were worked out rather than looked up.
    #[cfg(test)]
    evaluated: usize,
}

/// A slot of `Premultiply` that has not been worked out. No converted byte is
/// as large.
const UNKNOWN: u16 = u16::MAX;

impl Premultiply {
    const fn new() -> Self {
        Self {
            bytes: Vec::new(),
            #[cfg(test)]
            evaluated: 0,
        }
    }

    /// `Tables::premultiplied`, worked out once for each channel and alpha.
    #[inline]
    fn convert(&mut self, channel: u8, alpha: u8, tables: &Tables) -> u8 {
        debug_assert!(!matches!(alpha, 0 | 255));
        if self.bytes.is_empty() {
            self.bytes = vec![UNKNOWN; 1 << 16];
        }
        let slot = &mut self.bytes[usize::from(alpha) << 8 | usize::from(channel)];
        if *slot == UNKNOWN {
            *slot = u16::from(tables.premultiplied(channel, alpha));
            #[cfg(test)]
            {
                self.evaluated += 1;
            }
        }
        *slot as u8
    }
}

/// A box filter over `src`, `width` by `height` premultiplied texels, that
/// halves the flagged axes, in linear light: a texel's colour is sRGB-encoded
/// and its alpha is not. Odd trailing rows and columns repeat their last
/// texel. With `premultiply`, translucent texels are converted as they are
/// read, which is the conversion of the whole image before filtering without
/// a copy of it.
fn halve(
    src: &[u8],
    width: u32,
    height: u32,
    axes: [bool; 2],
    premultiply: Option<&mut Premultiply>,
    tables: &Tables,
) -> ([u32; 2], Vec<u8>) {
    let (across, down) = (1 + u32::from(axes[0]), 1 + u32::from(axes[1]));
    let size = [width.div_ceil(across), height.div_ceil(down)];
    let mut halved = vec![0; size[0] as usize * size[1] as usize * 4];
    let (columns, rows) = (width as usize, height as usize);
    match axes {
        [true, true] => halve_into::<2, 2>(src, columns, rows, premultiply, tables, &mut halved),
        [true, false] => halve_into::<2, 1>(src, columns, rows, premultiply, tables, &mut halved),
        [false, true] => halve_into::<1, 2>(src, columns, rows, premultiply, tables, &mut halved),
        [false, false] => halve_into::<1, 1>(src, columns, rows, premultiply, tables, &mut halved),
    }
    (size, halved)
}

/// `halve` for `ACROSS` by `DOWN` texels to one, written into `halved`.
fn halve_into<const ACROSS: usize, const DOWN: usize>(
    src: &[u8],
    columns: usize,
    rows: usize,
    premultiply: Option<&mut Premultiply>,
    tables: &Tables,
    halved: &mut [u8],
) {
    // Opaque and clear texels are read as they are, which is most of them.
    let plain = |texel: &[u8]| {
        [
            tables.decode[usize::from(texel[0])],
            tables.decode[usize::from(texel[1])],
            tables.decode[usize::from(texel[2])],
            u16::from(texel[3]),
        ]
    };
    let row = columns.div_ceil(ACROSS) * 4;
    match premultiply {
        None => sum_taps::<ACROSS, DOWN>(src, columns, rows, plain, tables, halved, row),
        Some(premultiply) => {
            let converting = |texel: &[u8]| {
                let alpha = texel[3];
                if matches!(alpha, 0 | 255) {
                    return plain(texel);
                }
                [
                    tables.decode[usize::from(premultiply.convert(texel[0], alpha, tables))],
                    tables.decode[usize::from(premultiply.convert(texel[1], alpha, tables))],
                    tables.decode[usize::from(premultiply.convert(texel[2], alpha, tables))],
                    u16::from(alpha),
                ]
            };
            sum_taps::<ACROSS, DOWN>(src, columns, rows, converting, tables, halved, row);
        }
    }
}

/// The mean of the `ACROSS` by `DOWN` texels under each texel of `halved`,
/// which has `row` bytes to a row, `light` being a texel's linear colour in
/// 12 bits and its alpha.
fn sum_taps<const ACROSS: usize, const DOWN: usize>(
    src: &[u8],
    columns: usize,
    rows: usize,
    mut light: impl FnMut(&[u8]) -> [u16; 4],
    tables: &Tables,
    halved: &mut [u8],
    row: usize,
) {
    let taps = (ACROSS * DOWN) as u32;
    let mean = |total: u32| ((total + taps / 2) / taps) as usize;
    let stride = columns * 4;
    for (y, line) in halved.chunks_exact_mut(row).enumerate() {
        // Past the last row, the last row again.
        let lines: [&[u8]; DOWN] = std::array::from_fn(|dy| {
            let at = (DOWN * y + dy).min(rows - 1);
            &src[at * stride..][..stride]
        });
        for (x, pixel) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let mut sum = [0u32; 4];
            for line in lines {
                for dx in 0..ACROSS {
                    let at = (ACROSS * x + dx).min(columns - 1) * 4;
                    for (total, value) in sum.iter_mut().zip(light(&line[at..][..4])) {
                        *total += u32::from(value);
                    }
                }
            }
            for (channel, total) in pixel[..3].iter_mut().zip(sum) {
                *channel = tables.encode[mean(total)];
            }
            pixel[3] = mean(sum[3]) as u8;
        }
    }
}

/// Scene images are premultiplied in sRGB space; blending here is linear. The
/// sRGB texture format decodes on sampling, so translucent texels are stored as
/// the sRGB encoding of their premultiplied *linear* colour. Opaque and fully
/// transparent texels already are.
fn linear_premultiplied<'a>(
    pixels: Cow<'a, [u8]>,
    premultiply: &mut Premultiply,
    tables: &Tables,
) -> Cow<'a, [u8]> {
    let settled = |pixel: &[u8; 4]| matches!(pixel[3], 0 | 255);
    if pixels.as_chunks::<4>().0.iter().all(settled) {
        return pixels;
    }
    let mut pixels = pixels.into_owned();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = pixel[3];
        if settled(pixel) {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = premultiply.convert(*channel, alpha, tables);
        }
    }
    Cow::Owned(pixels)
}

/// The axes the next halving flags: those over `target`, or, when none is,
/// both while the larger side is over `max_side`. `None` when the texels are
/// the size they should be.
fn halving(size: [u32; 2], target: [u32; 2], max_side: u32) -> Option<[bool; 2]> {
    if size[0] > target[0] || size[1] > target[1] {
        Some([size[0] > target[0], size[1] > target[1]])
    } else if size[0].max(size[1]) > max_side {
        Some([true; 2])
    } else {
        None
    }
}

/// The size and texels that go to the GPU for `pixels`, `size` texels across:
/// halved while they are over `target` on either side, and while the larger
/// side is over `max_side`. A photo that is halved is read from the image's
/// own texels, translucent ones converted as the first halving reads them; one
/// that is not is converted whole, and borrowed back when nothing needs it.
fn raster_pixels<'a>(
    pixels: Cow<'a, [u8]>,
    size: [u32; 2],
    target: [u32; 2],
    max_side: u32,
    premultiply: &mut Premultiply,
    tables: &Tables,
) -> ([u32; 2], Cow<'a, [u8]>) {
    let Some(axes) = halving(size, target, max_side) else {
        return (size, linear_premultiplied(pixels, premultiply, tables));
    };
    let (mut size, mut texels) = halve(&pixels, size[0], size[1], axes, Some(premultiply), tables);
    while let Some(axes) = halving(size, target, max_side) {
        (size, texels) = halve(&texels, size[0], size[1], axes, None, tables);
    }
    (size, Cow::Owned(texels))
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
        let mut memo = Premultiply::new();
        let kept = linear_premultiplied(
            Cow::Borrowed(&[200, 100, 0, 255, 0, 0, 0, 0]),
            &mut memo,
            tables(),
        );
        assert!(matches!(kept, Cow::Borrowed(_)));
        // Opaque white at half alpha: linear 0.5 premultiplied, sRGB-encoded.
        let half = linear_premultiplied(Cow::Borrowed(&[128, 128, 128, 128]), &mut memo, tables());
        assert_eq!(half[3], 128);
        assert!(half[0].abs_diff(188) <= 1, "{}", half[0]);
    }

    #[test]
    fn halving_averages_blocks_in_linear_light_and_keeps_odd_edges() {
        let black = [0, 0, 0, 255];
        let white = [255; 4];
        let both = [black, white, white].concat();
        let ([w, h], out) = halve(&both, 3, 1, [true; 2], None, tables());
        assert_eq!((w, h), (2, 1));
        // Half of the light of black and white is sRGB 188, not 128.
        assert!(out[0].abs_diff(188) <= 1, "{}", out[0]);
        assert_eq!(out[3], 255);
        assert_eq!(out[4..], white);
        // One axis at a time.
        let both = [black, white, white, black].concat();
        let ([w, h], out) = halve(&both, 2, 2, [true, false], None, tables());
        assert_eq!((w, h), (1, 2));
        assert!(out[0].abs_diff(188) <= 1 && out[4].abs_diff(188) <= 1);
        // Alpha is averaged as it is.
        let (_, out) = halve(
            &[0, 0, 0, 0, 255, 255, 255, 255],
            2,
            1,
            [true, false],
            None,
            tables(),
        );
        assert!(out[3].abs_diff(128) <= 1);
    }

    #[test]
    fn a_side_is_halved_while_it_still_covers_what_is_drawn() {
        assert_eq!(halved_side(512, 64.0), 64);
        assert_eq!(halved_side(512, 65.0), 128);
        // Never below what is drawn, and untouched drawn at or near its size.
        assert_eq!(halved_side(512, 300.0), 512);
        assert_eq!(halved_side(512, 512.0), 512);
        assert_eq!(halved_side(512, 4000.0), 512);
        assert_eq!(halved_side(1000, 100.0), 125);
        assert_eq!(halved_side(1000, 0.0), 1);
        assert_eq!(halved_side(512, f32::NAN), 512);
    }

    /// Texels of every alpha, clear and opaque ones among them, and colour
    /// channels that may exceed the alpha, which premultiplied pixels allow.
    fn noise(width: u32, height: u32, seed: u64) -> Vec<u8> {
        let mut state = seed | 1;
        let mut bytes = Vec::new();
        for _ in 0..width * height {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let alpha = match state % 4 {
                0 => 0,
                1 => 255,
                _ => (state >> 8) as u8,
            };
            bytes.extend([
                (state >> 16) as u8,
                (state >> 24) as u8,
                (state >> 32) as u8,
                alpha,
            ]);
        }
        bytes
    }

    /// The box filter written the slow way, one tap at a time, with the
    /// tables built where they are used.
    fn slow_halved(
        width: u32,
        height: u32,
        pixels: &[u8],
        axes: [bool; 2],
    ) -> (u32, u32, Cow<'static, [u8]>) {
        let (across, down) = (1 + u32::from(axes[0]), 1 + u32::from(axes[1]));
        let (w, h) = (width.div_ceil(across), height.div_ceil(down));
        let decode: [u16; 256] = std::array::from_fn(|byte| {
            (glassmere_scene::Color(byte as u8, 0, 0, 255).linear()[0] * 4095.0).round() as u16
        });
        let encode: [u8; 4096] = std::array::from_fn(|light| {
            let light = light as f32 / 4095.0;
            let encoded = if light <= 0.003_130_8 {
                light * 12.92
            } else {
                1.055 * light.powf(1.0 / 2.4) - 0.055
            };
            (encoded * 255.0).round() as u8
        });
        let taps = across * down;
        let mean = |total: u32| ((total + taps / 2) / taps) as usize;
        let mut out = Vec::with_capacity(w as usize * h as usize * 4);
        for y in 0..h {
            for x in 0..w {
                let mut sum = [0u32; 4];
                for (dx, dy) in (0..across).flat_map(|dx| (0..down).map(move |dy| (dx, dy))) {
                    let (x, y) = (
                        (across * x + dx).min(width - 1),
                        (down * y + dy).min(height - 1),
                    );
                    let at = (y as usize * width as usize + x as usize) * 4;
                    let texel = &pixels[at..at + 4];
                    let light = [
                        decode[usize::from(texel[0])],
                        decode[usize::from(texel[1])],
                        decode[usize::from(texel[2])],
                        u16::from(texel[3]),
                    ];
                    for (total, value) in sum.iter_mut().zip(light) {
                        *total += u32::from(value);
                    }
                }
                out.extend(sum[..3].iter().map(|total| encode[mean(*total)]));
                out.push(mean(sum[3]) as u8);
            }
        }
        (w, h, Cow::Owned(out))
    }

    /// Every translucent texel converted by the formula, in a pass of its own
    /// over all of them.
    fn slow_premultiplied(pixels: Cow<'_, [u8]>) -> Cow<'_, [u8]> {
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

    #[test]
    fn halving_matches_the_slow_filter_at_every_edge() {
        let mut sizes: Vec<(u32, u32)> = (1..=9)
            .flat_map(|width| (1..=9).map(move |height| (width, height)))
            .collect();
        sizes.extend([(17, 5), (64, 33), (1, 40), (40, 1)]);
        for (width, height) in sizes {
            let pixels = noise(width, height, u64::from(width * 1000 + height));
            for axes in [[true, true], [true, false], [false, true], [false, false]] {
                // An axis is flagged only where there is something to halve.
                if (axes[0] && width < 2) || (axes[1] && height < 2) {
                    continue;
                }
                let at = format!("{width}x{height} {axes:?}");
                let ([w, h], read) = halve(&pixels, width, height, axes, None, tables());
                let (slow_w, slow_h, slow) = slow_halved(width, height, &pixels, axes);
                assert_eq!((w, h), (slow_w, slow_h), "{at}");
                assert_eq!(read, *slow, "read as they are, {at}");

                let mut memo = Premultiply::new();
                let (_, converted) = halve(&pixels, width, height, axes, Some(&mut memo), tables());
                let swept = slow_premultiplied(Cow::Borrowed(&pixels));
                let (_, _, slow) = slow_halved(width, height, &swept, axes);
                assert_eq!(converted, *slow, "converted as read, {at}");
            }
        }
    }

    #[test]
    fn raster_pixels_matches_the_slow_pipeline() {
        let slow = |pixels: &[u8], size: [u32; 2], target: [u32; 2], max_side: u32| {
            let mut pixels = slow_premultiplied(Cow::Borrowed(pixels)).into_owned();
            let [mut width, mut height] = size;
            while width > target[0] || height > target[1] {
                let axes = [width > target[0], height > target[1]];
                let (w, h, halved) = slow_halved(width, height, &pixels, axes);
                (width, height, pixels) = (w, h, halved.into_owned());
            }
            while width.max(height) > max_side {
                let (w, h, halved) = slow_halved(width, height, &pixels, [true; 2]);
                (width, height, pixels) = (w, h, halved.into_owned());
            }
            ([width, height], pixels)
        };
        let opaque = |pixels: Vec<u8>| -> Vec<u8> {
            let mut pixels = pixels;
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel[3] = 255;
            }
            pixels
        };
        let mut memo = Premultiply::new();
        for (width, height) in [(9u32, 7u32), (64, 33), (501, 333)] {
            let translucent = noise(width, height, u64::from(width));
            for pixels in [opaque(translucent.clone()), translucent] {
                let size = [width, height];
                let quarter = [width.div_ceil(4), height.div_ceil(4)];
                let cases = [
                    // Nothing to halve.
                    (size, u32::MAX),
                    (quarter, u32::MAX),
                    ([width.div_ceil(2), height.div_ceil(2)], u32::MAX),
                    // One axis at a time.
                    ([width, height.div_ceil(2)], u32::MAX),
                    ([width.div_ceil(2), height], u32::MAX),
                    // Too large for the device.
                    (size, 4),
                    (quarter, 2),
                ];
                for (target, max_side) in cases {
                    let at = format!("{width}x{height} to {target:?} within {max_side}");
                    let (got, texels) = raster_pixels(
                        Cow::Borrowed(&pixels),
                        size,
                        target,
                        max_side,
                        &mut memo,
                        tables(),
                    );
                    let (expected, slow) = slow(&pixels, size, target, max_side);
                    assert_eq!(got, expected, "{at}");
                    assert_eq!(*texels, *slow, "{at}");
                }
            }
        }
    }

    #[test]
    fn the_premultiply_memo_equals_the_formula_and_works_each_pair_out_once() {
        let mut memo = Premultiply::new();
        assert_eq!(memo.evaluated, 0);
        for alpha in 1..=254u8 {
            for channel in 0..=255u8 {
                let slow = slow_premultiplied(Cow::Owned(vec![channel, 0, 0, alpha]));
                assert_eq!(
                    memo.convert(channel, alpha, tables()),
                    slow[0],
                    "{channel} at {alpha}"
                );
            }
        }
        assert_eq!(memo.evaluated, 254 * 256);
        // Asked again, in the other order: nothing is worked out.
        for alpha in (1..=254u8).rev() {
            for channel in (0..=255u8).rev() {
                memo.convert(channel, alpha, tables());
            }
        }
        assert_eq!(memo.evaluated, 254 * 256);

        // Four colours over 360,000 texels are twelve conversions at most.
        let colours = [
            [10, 20, 30, 100],
            [200, 100, 50, 128],
            [255, 255, 255, 17],
            [0, 5, 0, 250],
        ];
        let pixels: Vec<u8> = (0..600usize * 600)
            .flat_map(|texel| colours[(texel / 7 + texel / 600) % 4])
            .collect();
        let mut memo = Premultiply::new();
        let converted = linear_premultiplied(Cow::Borrowed(&pixels), &mut memo, tables());
        assert!((1..=12).contains(&memo.evaluated), "{}", memo.evaluated);
        assert_eq!(*converted, *slow_premultiplied(Cow::Borrowed(&pixels)));
    }

    #[test]
    fn the_tables_are_built_once_and_equal_the_formulas() {
        let decode: [u16; 256] = std::array::from_fn(|byte| {
            (glassmere_scene::Color(byte as u8, 0, 0, 255).linear()[0] * 4095.0).round() as u16
        });
        let encode: [u8; 4096] = std::array::from_fn(|light| {
            let light = light as f32 / 4095.0;
            let encoded = if light <= 0.003_130_8 {
                light * 12.92
            } else {
                1.055 * light.powf(1.0 / 2.4) - 0.055
            };
            (encoded * 255.0).round() as u8
        });
        let light: [f32; 256] =
            std::array::from_fn(|byte| glassmere_scene::Color(byte as u8, 0, 0, 255).linear()[0]);
        assert_eq!(tables().decode, decode);
        assert_eq!(tables().encode, encode);
        assert_eq!(tables().light, light);
        assert!(std::ptr::eq(tables(), tables()));
    }

    #[test]
    fn an_opaque_photo_that_is_not_halved_is_borrowed_back() {
        let pixels = [200, 100, 50, 255, 0, 0, 0, 0].repeat(8);
        let (size, texels) = raster_pixels(
            Cow::Borrowed(&pixels),
            [4, 4],
            [4, 4],
            4,
            &mut Premultiply::new(),
            tables(),
        );
        assert_eq!(size, [4, 4]);
        assert!(matches!(texels, Cow::Borrowed(_)));
        assert!(std::ptr::eq(texels.as_ptr(), pixels.as_ptr()));
    }
}
