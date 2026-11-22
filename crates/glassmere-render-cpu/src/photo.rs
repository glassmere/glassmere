//! Photos drawn at under half their size: filtering them down, converting
//! their translucent texels once for each colour, and deciding how long the
//! filtered texels are worth keeping.
use super::Gamma;
use std::collections::HashMap;
use tiny_skia::Pixmap;

/// Frames a vector image, or a translucent image drawn at its own size, goes
/// without being drawn before its texels are let go.
pub(crate) const IDLE_FRAMES: u64 = 2;
/// The same for a filtered photo, which costs a pass over the whole photo to
/// make and a few kilobytes to keep.
pub(crate) const PHOTO_IDLE_FRAMES: u64 = 120;
/// Bytes of filtered photos kept, past which the ones drawn least recently go.
pub(crate) const PHOTO_BYTES: usize = 64 << 20;

/// Texels ready to sample, and the frame that used them last.
pub(crate) struct Raster {
    pub texels: Pixmap,
    pub used: u64,
    /// Filtered down from a photo.
    pub photo: bool,
}

/// What a translucent texel's colour channel is converted to, for every
/// channel and alpha that has come up. The result depends on those two bytes
/// alone, and a photo has a few distinct ones among millions of texels.
pub(crate) struct Premultiply {
    /// By `alpha << 8 | channel`; empty until the first conversion.
    bytes: Vec<u16>,
    /// How many conversions were worked out rather than looked up.
    #[cfg(test)]
    pub(crate) evaluated: usize,
}

/// A slot of `Premultiply` that has not been worked out. No converted byte is
/// as large.
const UNKNOWN: u16 = u16::MAX;

impl Premultiply {
    pub(crate) const fn new() -> Self {
        Self {
            bytes: Vec::new(),
            #[cfg(test)]
            evaluated: 0,
        }
    }

    /// The colour channel of a texel with `alpha`, stored as the sRGB encoding
    /// of its premultiplied linear light. `alpha` is neither 0 nor 255: those
    /// texels need no conversion and are not asked about.
    #[inline]
    pub(crate) fn convert(&mut self, channel: u8, alpha: u8, gamma: &Gamma) -> u8 {
        debug_assert!(!matches!(alpha, 0 | 255));
        if self.bytes.is_empty() {
            self.bytes = vec![UNKNOWN; 1 << 16];
        }
        let slot = &mut self.bytes[usize::from(alpha) << 8 | usize::from(channel)];
        if *slot == UNKNOWN {
            *slot = u16::from(gamma.premultiplied(channel, alpha));
            #[cfg(test)]
            {
                self.evaluated += 1;
            }
        }
        *slot as u8
    }
}

/// A box filter over `src`, `width` by `height` premultiplied texels, that
/// halves the flagged axes in linear light. Odd trailing rows and columns
/// repeat their last texel. With `premultiply`, translucent texels are
/// converted as they are read, which is the conversion of the whole image
/// before filtering without a copy of it.
///
/// `None` when `src` is not `width` by `height` texels, or when the result
/// cannot be allocated.
pub(crate) fn halve(
    src: &[u8],
    width: u32,
    height: u32,
    axes: [bool; 2],
    premultiply: Option<&mut Premultiply>,
    gamma: &Gamma,
) -> Option<Pixmap> {
    let (columns, rows) = (width as usize, height as usize);
    if columns.checked_mul(rows)?.checked_mul(4)? != src.len() {
        return None;
    }
    let (across, down) = (1 + u32::from(axes[0]), 1 + u32::from(axes[1]));
    let mut halved = Pixmap::new(width.div_ceil(across), height.div_ceil(down))?;
    match axes {
        [true, true] => halve_into::<2, 2>(src, columns, rows, premultiply, gamma, &mut halved),
        [true, false] => halve_into::<2, 1>(src, columns, rows, premultiply, gamma, &mut halved),
        [false, true] => halve_into::<1, 2>(src, columns, rows, premultiply, gamma, &mut halved),
        [false, false] => halve_into::<1, 1>(src, columns, rows, premultiply, gamma, &mut halved),
    }
    Some(halved)
}

/// `halve` for `ACROSS` by `DOWN` texels to one, written into `halved`.
fn halve_into<const ACROSS: usize, const DOWN: usize>(
    src: &[u8],
    columns: usize,
    rows: usize,
    premultiply: Option<&mut Premultiply>,
    gamma: &Gamma,
    halved: &mut Pixmap,
) {
    // Opaque and clear texels are read as they are, which is most of them.
    let plain = |texel: &[u8]| {
        [
            gamma.linear[usize::from(texel[0])],
            gamma.linear[usize::from(texel[1])],
            gamma.linear[usize::from(texel[2])],
            u16::from(texel[3]),
        ]
    };
    match premultiply {
        None => sum_taps::<ACROSS, DOWN>(src, columns, rows, plain, gamma, halved),
        Some(premultiply) => {
            let converting = |texel: &[u8]| {
                let alpha = texel[3];
                if matches!(alpha, 0 | 255) {
                    return plain(texel);
                }
                [
                    gamma.linear[usize::from(premultiply.convert(texel[0], alpha, gamma))],
                    gamma.linear[usize::from(premultiply.convert(texel[1], alpha, gamma))],
                    gamma.linear[usize::from(premultiply.convert(texel[2], alpha, gamma))],
                    u16::from(alpha),
                ]
            };
            sum_taps::<ACROSS, DOWN>(src, columns, rows, converting, gamma, halved);
        }
    }
}

/// The mean of the `ACROSS` by `DOWN` texels under each texel of `halved`,
/// `light` being a texel's linear colour in 12 bits and its alpha.
fn sum_taps<const ACROSS: usize, const DOWN: usize>(
    src: &[u8],
    columns: usize,
    rows: usize,
    mut light: impl FnMut(&[u8]) -> [u16; 4],
    gamma: &Gamma,
    halved: &mut Pixmap,
) {
    let taps = (ACROSS * DOWN) as u32;
    let mean = |total: u32| ((total + taps / 2) / taps) as usize;
    let stride = columns * 4;
    let row = halved.width() as usize * 4;
    for (y, line) in halved.data_mut().chunks_exact_mut(row).enumerate() {
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
                *channel = gamma.back[mean(total)];
            }
            pixel[3] = mean(sum[3]) as u8;
        }
    }
}

/// Lets go of the rasters that have gone unused for long enough, and then,
/// while the filtered photos together are over `budget`, of the ones drawn
/// least recently. Whatever `frame` drew stays, even when it alone is over.
pub(crate) fn retire(rasters: &mut HashMap<(u64, [u32; 2]), Raster>, frame: u64, budget: usize) {
    rasters.retain(|_, raster| {
        let idle = if raster.photo {
            PHOTO_IDLE_FRAMES
        } else {
            IDLE_FRAMES
        };
        frame.saturating_sub(raster.used) < idle
    });
    let bytes = |raster: &Raster| raster.texels.data().len();
    let mut kept: usize = rasters
        .values()
        .filter(|raster| raster.photo)
        .map(bytes)
        .sum();
    if kept <= budget {
        return;
    }
    // Ordered by when they were drawn and then by key, so that which go does
    // not depend on the order the map holds them in.
    let mut candidates: Vec<(u64, (u64, [u32; 2]), usize)> = rasters
        .iter()
        .filter(|(_, raster)| raster.photo && raster.used != frame)
        .map(|(key, raster)| (raster.used, *key, bytes(raster)))
        .collect();
    candidates.sort_unstable();
    for (_, key, size) in candidates {
        if kept <= budget {
            break;
        }
        rasters.remove(&key);
        kept -= size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuRenderer, halved_side};
    use glassmere_scene::{Color, Image, Primitive, Rect, Scene, Size};
    use glassmere_text::TextSystem;

    fn renderer() -> CpuRenderer {
        CpuRenderer::with_text_system(TextSystem::with_fonts([include_bytes!(
            "../../glassmere-render-wgpu/fonts/FiraMono-Regular.ttf"
        )
        .to_vec()]))
    }

    fn pixmap(width: u32, height: u32, bytes: &[u8]) -> Pixmap {
        let mut pixmap = Pixmap::new(width, height).unwrap();
        pixmap.data_mut().copy_from_slice(bytes);
        pixmap
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

    /// The box filter written the slow way, one tap at a time, to compare
    /// with.
    fn slow_halved(gamma: &Gamma, texels: &Pixmap, axes: [bool; 2]) -> Option<Pixmap> {
        let (width, height) = (texels.width(), texels.height());
        let (across, down) = (1 + u32::from(axes[0]), 1 + u32::from(axes[1]));
        let mut halved = Pixmap::new(width.div_ceil(across), height.div_ceil(down))?;
        let taps = across * down;
        let mean = |total: u32| ((total + taps / 2) / taps) as usize;
        let row = halved.width() as usize * 4;
        for (y, line) in halved.data_mut().chunks_exact_mut(row).enumerate() {
            for (x, pixel) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let mut sum = [0u32; 4];
                for (dx, dy) in (0..across).flat_map(|dx| (0..down).map(move |dy| (dx, dy))) {
                    let x = (across * x as u32 + dx).min(width - 1);
                    let y = (down * y as u32 + dy).min(height - 1);
                    let texel =
                        &texels.data()[(y as usize * width as usize + x as usize) * 4..][..4];
                    let light = [
                        gamma.linear[usize::from(texel[0])],
                        gamma.linear[usize::from(texel[1])],
                        gamma.linear[usize::from(texel[2])],
                        u16::from(texel[3]),
                    ];
                    for (total, value) in sum.iter_mut().zip(light) {
                        *total += u32::from(value);
                    }
                }
                for (channel, total) in pixel[..3].iter_mut().zip(sum) {
                    *channel = gamma.back[mean(total)];
                }
                pixel[3] = mean(sum[3]) as u8;
            }
        }
        Some(halved)
    }

    /// Every translucent texel converted, in a pass of its own over all of
    /// them.
    fn slow_sweep(gamma: &Gamma, texels: &mut Pixmap) {
        for pixel in texels.data_mut().as_chunks_mut::<4>().0 {
            let alpha = u32::from(pixel[3]);
            if matches!(pixel[3], 0 | 255) {
                continue;
            }
            for channel in &mut pixel[..3] {
                let straight = (u32::from(*channel) * 255 / alpha).min(255) as usize;
                let light = f32::from(gamma.linear[straight]) * alpha as f32 / 255.0;
                *channel = gamma.back[light.round().clamp(0.0, 4095.0) as usize];
            }
        }
    }

    /// `texels` halved, flagging each axis that is over `size`, until it is
    /// `size`, by the slow filter.
    fn slow_chain(gamma: &Gamma, mut texels: Pixmap, size: [u32; 2]) -> Pixmap {
        while texels.width() > size[0] || texels.height() > size[1] {
            let axes = [texels.width() > size[0], texels.height() > size[1]];
            texels = slow_halved(gamma, &texels, axes).unwrap();
        }
        texels
    }

    #[test]
    fn halving_matches_the_slow_filter_at_every_edge() {
        let gamma = Gamma::new();
        let mut sizes: Vec<(u32, u32)> = (1..=9)
            .flat_map(|width| (1..=9).map(move |height| (width, height)))
            .collect();
        sizes.extend([(17, 5), (64, 33), (1, 40), (40, 1)]);
        for (width, height) in sizes {
            let bytes = noise(width, height, u64::from(width * 1000 + height));
            for axes in [[true, true], [true, false], [false, true], [false, false]] {
                // An axis is flagged only where there is something to halve.
                if (axes[0] && width < 2) || (axes[1] && height < 2) {
                    continue;
                }
                let at = format!("{width}x{height} {axes:?}");
                let read = halve(&bytes, width, height, axes, None, &gamma).unwrap();
                let slow = slow_halved(&gamma, &pixmap(width, height, &bytes), axes).unwrap();
                assert_eq!((read.width(), read.height()), (slow.width(), slow.height()));
                assert_eq!(read.data(), slow.data(), "read as they are, {at}");

                let mut memo = Premultiply::new();
                let converted =
                    halve(&bytes, width, height, axes, Some(&mut memo), &gamma).unwrap();
                let mut swept = pixmap(width, height, &bytes);
                slow_sweep(&gamma, &mut swept);
                let slow = slow_halved(&gamma, &swept, axes).unwrap();
                assert_eq!(converted.data(), slow.data(), "converted as read, {at}");
            }
        }
    }

    #[test]
    fn halving_a_photo_down_to_a_size_matches_the_slow_chain() {
        let gamma = Gamma::new();
        let bytes = noise(1000, 700, 7);
        let size = [63, 44];
        let mut memo = Premultiply::new();
        let axes = [true, true];
        let mut texels = halve(&bytes, 1000, 700, axes, Some(&mut memo), &gamma).unwrap();
        while texels.width() > size[0] || texels.height() > size[1] {
            let axes = [texels.width() > size[0], texels.height() > size[1]];
            texels = halve(
                texels.data(),
                texels.width(),
                texels.height(),
                axes,
                None,
                &gamma,
            )
            .unwrap();
        }
        let mut swept = pixmap(1000, 700, &bytes);
        slow_sweep(&gamma, &mut swept);
        let slow = slow_chain(&gamma, swept, size);
        assert_eq!([texels.width(), texels.height()], size);
        assert_eq!(texels.data(), slow.data());
    }

    #[test]
    fn halving_refuses_what_it_cannot_halve() {
        let gamma = Gamma::new();
        let bytes = noise(4, 4, 1);
        assert!(halve(&bytes, 4, 4, [true, true], None, &gamma).is_some());
        // Not the texels of that size.
        assert!(halve(&bytes[..60], 4, 4, [true, true], None, &gamma).is_none());
        assert!(halve(&bytes, 4, 3, [true, true], None, &gamma).is_none());
        assert!(halve(&[], 0, 4, [true, true], None, &gamma).is_none());
    }

    #[test]
    fn the_premultiply_memo_equals_the_formula_and_works_each_pair_out_once() {
        let gamma = Gamma::new();
        let mut memo = Premultiply::new();
        assert_eq!(memo.evaluated, 0);
        let formula = |channel: u8, alpha: u8| {
            let straight = (u32::from(channel) * 255 / u32::from(alpha)).min(255) as usize;
            let light = f32::from(gamma.linear[straight]) * u32::from(alpha) as f32 / 255.0;
            gamma.back[light.round().clamp(0.0, 4095.0) as usize]
        };
        for alpha in 1..=254u8 {
            for channel in 0..=255u8 {
                let converted = memo.convert(channel, alpha, &gamma);
                assert_eq!(converted, formula(channel, alpha), "{channel} at {alpha}");
            }
        }
        assert_eq!(memo.evaluated, 254 * 256);
        // Asked again, in the other order: nothing is worked out.
        for alpha in (1..=254u8).rev() {
            for channel in (0..=255u8).rev() {
                assert_eq!(
                    memo.convert(channel, alpha, &gamma),
                    formula(channel, alpha)
                );
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
        let bytes: Vec<u8> = (0..600usize * 600)
            .flat_map(|texel| colours[(texel / 7 + texel / 600) % 4])
            .collect();
        let mut memo = Premultiply::new();
        let halved = halve(&bytes, 600, 600, [true, true], Some(&mut memo), &gamma).unwrap();
        assert!((1..=12).contains(&memo.evaluated), "{}", memo.evaluated);
        let mut swept = pixmap(600, 600, &bytes);
        slow_sweep(&gamma, &mut swept);
        let slow = slow_halved(&gamma, &swept, [true, true]).unwrap();
        assert_eq!(halved.data(), slow.data());
    }

    fn raster(side: u32, used: u64, photo: bool) -> Raster {
        Raster {
            texels: Pixmap::new(side, side).unwrap(),
            used,
            photo,
        }
    }

    fn keys(rasters: &HashMap<(u64, [u32; 2]), Raster>) -> Vec<u64> {
        let mut keys: Vec<u64> = rasters.keys().map(|key| key.0).collect();
        keys.sort_unstable();
        keys
    }

    #[test]
    fn retiring_keeps_what_is_used_and_sheds_the_least_recently_drawn_over_budget() {
        // 16 by 16 texels of 4 bytes: 1,024 bytes each.
        let kept = |entries: &[(u64, u64, bool)], frame: u64, budget: usize| {
            let mut rasters: HashMap<(u64, [u32; 2]), Raster> = entries
                .iter()
                .map(|&(id, used, photo)| ((id, [16, 16]), raster(16, used, photo)))
                .collect();
            retire(&mut rasters, frame, budget);
            keys(&rasters)
        };
        // Under the budget, nothing is shed for it.
        assert_eq!(
            kept(&[(1, 5, true), (2, 6, true), (3, 7, true)], 7, 3 * 1024),
            [1, 2, 3]
        );
        // Over it, the oldest go until it is met.
        assert_eq!(
            kept(&[(1, 5, true), (2, 6, true), (3, 7, true)], 8, 2 * 1024),
            [2, 3]
        );
        assert_eq!(
            kept(&[(1, 5, true), (2, 6, true), (3, 7, true)], 8, 1024),
            [3]
        );
        // The order the entries were made in does not matter, and entries
        // drawn on the same frame go in the order of their keys.
        assert_eq!(
            kept(&[(9, 4, true), (3, 4, true), (5, 4, true)], 8, 2 * 1024),
            [5, 9]
        );
        // What the frame drew stays, even when it alone is over the budget.
        assert_eq!(kept(&[(1, 8, true), (2, 3, true)], 8, 1), [1]);
        // Only photos count toward it: what is not one takes no photo's
        // place.
        assert_eq!(
            kept(&[(1, 5, true), (2, 8, false), (3, 8, false)], 8, 1024),
            [1, 2, 3]
        );
        // A photo goes after 120 frames, anything else after two.
        assert_eq!(kept(&[(1, 8, true), (2, 8, false)], 9, usize::MAX), [1, 2]);
        assert_eq!(kept(&[(1, 8, true), (2, 8, false)], 10, usize::MAX), [1]);
        assert_eq!(kept(&[(1, 8, true)], 8 + 119, usize::MAX), [1]);
        assert!(kept(&[(1, 8, true)], 8 + 120, usize::MAX).is_empty());
        // A frame before the one that used it is not idle.
        assert_eq!(kept(&[(1, 9, true), (2, 9, false)], 8, usize::MAX), [1, 2]);
    }

    fn translucent_photo(width: u32, height: u32) -> Image {
        let mut rgba = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let alpha = match ((x + y) / 40) % 3 {
                    0 => 255,
                    1 if x % 2 == 0 => 128,
                    _ => 200,
                };
                rgba.extend([
                    (x * 255 / width) as u8,
                    (y * 255 / height) as u8,
                    ((x ^ y) & 255) as u8,
                    alpha,
                ]);
            }
        }
        Image::from_rgba8(width, height, rgba).unwrap()
    }

    fn drawn(image: &Image, rect: Rect) -> Primitive {
        let size = image.size();
        Primitive::Image {
            rect,
            image: image.clone(),
            source: Rect {
                x: 0.0,
                y: 0.0,
                width: size.width,
                height: size.height,
            },
            tint: None,
            radius: 0.0,
        }
    }

    #[test]
    fn a_prepared_photo_equals_the_slow_pipeline_byte_for_byte() {
        let mut renderer = renderer();
        let photo = translucent_photo(501, 333);
        // 501 by 333 texels in 40 pixels, and in 24: three halvings and four.
        // The wide and the tall photo are halved across or down only, which
        // the first step has to flag too.
        let wide = translucent_photo(1000, 40);
        let tall = translucent_photo(40, 1000);
        let cases = [
            (&photo, [40.0, 40.0], 1.0),
            (&photo, [12.0, 12.0], 2.0),
            (&wide, [100.0, 40.0], 1.0),
            (&tall, [40.0, 100.0], 1.0),
        ];
        for (photo, [rect_width, rect_height], scale) in cases {
            let scene = Scene {
                size: Size::new(128.0 / scale, 128.0 / scale),
                primitives: vec![drawn(
                    photo,
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: rect_width,
                        height: rect_height,
                    },
                )],
            };
            renderer.render([128, 128], scale, &scene, Color::WHITE);
            let (width, height, pixels) = photo.pixels().unwrap();
            let size = [
                halved_side(width, rect_width * scale),
                halved_side(height, rect_height * scale),
            ];
            assert_ne!(size, [width, height]);
            let mut slow = pixmap(width, height, pixels);
            slow_sweep(&renderer.gamma, &mut slow);
            let slow = slow_chain(&renderer.gamma, slow, size);
            let raster = &renderer.rasters[&(photo.id(), size)];
            assert!(raster.photo);
            assert_eq!(raster.texels.data(), slow.data(), "at {size:?}");
        }
        // A translucent image drawn at its own size is converted and kept the
        // short time.
        let small = Image::from_rgba8(8, 8, [200, 100, 50, 128].repeat(64)).unwrap();
        let scene = Scene {
            size: Size::new(64.0, 64.0),
            primitives: vec![drawn(
                &small,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 8.0,
                    height: 8.0,
                },
            )],
        };
        renderer.render([64, 64], 1.0, &scene, Color::WHITE);
        let (_, _, pixels) = small.pixels().unwrap();
        let mut slow = pixmap(8, 8, pixels);
        slow_sweep(&renderer.gamma, &mut slow);
        let raster = &renderer.rasters[&(small.id(), [8, 8])];
        assert!(!raster.photo);
        assert_eq!(raster.texels.data(), slow.data());
    }
}
