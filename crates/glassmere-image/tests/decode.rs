use glassmere_image::{Loader, decode, load};
use image::codecs::gif::{GifEncoder, Repeat};
use image::codecs::{jpeg::JpegEncoder, png::PngEncoder, webp::WebPEncoder};
use image::{Delay, ExtendedColorType, Frame, ImageEncoder, ImageFormat, Rgba, RgbaImage};
use moxcms::{ColorProfile, ToneReprCurve};
use std::io::Cursor;
use std::num::NonZeroU32;
use std::sync::mpsc::channel;
use std::time::Duration;

fn encoded(format: ImageFormat) -> Vec<u8> {
    let mut picture = RgbaImage::from_pixel(3, 2, Rgba([255, 0, 0, 255]));
    picture.put_pixel(2, 1, Rgba([0, 0, 255, 255]));
    let mut bytes = Vec::new();
    if format == ImageFormat::Jpeg {
        image::DynamicImage::ImageRgba8(picture)
            .into_rgb8()
            .write_to(&mut Cursor::new(&mut bytes), format)
            .unwrap();
    } else {
        picture
            .write_to(&mut Cursor::new(&mut bytes), format)
            .unwrap();
    }
    bytes
}

#[test]
fn raster_formats_decode_to_premultiplied_pixels() {
    for format in [ImageFormat::Png, ImageFormat::Gif, ImageFormat::WebP] {
        let image = decode(&encoded(format)).unwrap();
        let (width, height, pixels) = image.pixels().unwrap();
        assert_eq!((width, height), (3, 2), "{format:?}");
        assert_eq!(pixels[..4], [255, 0, 0, 255], "{format:?}");
        assert_eq!(pixels[20..], [0, 0, 255, 255], "{format:?}");
    }
    // Lossy: only the size and the rough colour survive.
    let jpeg = decode(&encoded(ImageFormat::Jpeg)).unwrap();
    let (width, height, pixels) = jpeg.pixels().unwrap();
    assert_eq!((width, height), (3, 2));
    assert!(pixels[0] > 150 && pixels[3] == 255);

    let mut translucent = Vec::new();
    RgbaImage::from_pixel(1, 1, Rgba([255, 128, 0, 128]))
        .write_to(&mut Cursor::new(&mut translucent), ImageFormat::Png)
        .unwrap();
    let image = decode(&translucent).unwrap();
    assert_eq!(image.pixels().unwrap().2, [128, 64, 0, 128]);
}

#[test]
fn svg_is_a_vector_image_rasterized_at_the_requested_size() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
        <rect width="10" height="10" fill="#f00"/></svg>"##;
    let image = decode(svg).unwrap();
    assert!(image.is_vector());
    assert_eq!(image.size(), glassmere_scene::Size::new(20.0, 10.0));
    let pixels = image.vector().unwrap().rasterize(40, 20);
    assert_eq!(pixels.len(), 40 * 20 * 4);
    let at = |x: usize, y: usize| &pixels[(y * 40 + x) * 4..][..4];
    assert_eq!(at(10, 10), [255, 0, 0, 255]);
    assert_eq!(at(30, 10), [0, 0, 0, 0]);
}

#[test]
fn unknown_content_is_an_error_not_a_panic() {
    assert!(decode(b"").is_err());
    assert!(decode(b"definitely not an image").is_err());
    // A truncated PNG: the signature is recognised, the content is not there.
    assert!(decode(&encoded(ImageFormat::Png)[..20]).is_err());
}

#[test]
fn the_loader_decodes_in_the_background_and_reports_failures_by_path() {
    let directory = std::env::temp_dir().join(format!("glassmere-image-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let good = directory.join("good.png");
    let missing = directory.join("missing.png");
    std::fs::write(&good, encoded(ImageFormat::Png)).unwrap();
    assert!(load(&good).is_ok());

    let (woken, wakes) = channel();
    let loader = Loader::new(move || woken.send(()).unwrap());
    loader.request(good.clone());
    loader.request(missing.clone());
    for _ in 0..2 {
        wakes.recv_timeout(Duration::from_secs(20)).unwrap();
    }
    let finished: Vec<_> = loader.finished().collect();
    assert_eq!(finished[0].0, good);
    assert_eq!(finished[0].1.as_ref().unwrap().size().width, 3.0);
    assert_eq!(finished[1].0, missing);
    assert!(finished[1].1.as_ref().unwrap_err().contains("missing.png"));
    assert!(loader.finished().next().is_none());
    std::fs::remove_dir_all(&directory).unwrap();
}

fn encode_png(
    pixels: &[u8],
    (width, height): (u32, u32),
    color: ExtendedColorType,
    icc: Option<&[u8]>,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = PngEncoder::new(&mut bytes);
    if let Some(icc) = icc {
        encoder.set_icc_profile(icc.to_vec()).unwrap();
    }
    encoder.write_image(pixels, width, height, color).unwrap();
    bytes
}

fn display_p3() -> Vec<u8> {
    ColorProfile::new_display_p3().encode().unwrap()
}

fn pixels_of(bytes: &[u8]) -> Vec<u8> {
    decode(bytes).unwrap().pixels().unwrap().2.to_vec()
}

fn srgb_to_linear(encoded: f64) -> f64 {
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(linear: f64) -> f64 {
    let linear = linear.clamp(0.0, 1.0);
    if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// Display P3 to sRGB: both use the sRGB curve, so only the primaries move.
/// The matrix is the published one for linear light at D65.
fn p3_to_srgb(p3: [u8; 3]) -> [u8; 3] {
    const MATRIX: [[f64; 3]; 3] = [
        [1.2249401, -0.2249404, 0.0],
        [-0.0420569, 1.0420571, 0.0],
        [-0.0196376, -0.0786361, 1.0982735],
    ];
    let linear = p3.map(|code| srgb_to_linear(f64::from(code) / 255.0));
    MATRIX.map(|row| {
        let sum = row[0] * linear[0] + row[1] * linear[1] + row[2] * linear[2];
        (linear_to_srgb(sum) * 255.0).round() as u8
    })
}

fn premultiplied(rgb: [u8; 3], alpha: u8) -> [u8; 3] {
    rgb.map(|code| (f32::from(code) * f32::from(alpha) / 255.0).round() as u8)
}

/// Opaque pixels with primaries, greys and odd codes.
const PALETTE: [[u8; 3]; 9] = [
    [0, 0, 0],
    [255, 255, 255],
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [200, 100, 50],
    [1, 2, 3],
    [250, 251, 252],
    [30, 60, 90],
];

fn palette_rgba() -> Vec<u8> {
    PALETTE
        .iter()
        .flat_map(|[r, g, b]| [*r, *g, *b, 255])
        .collect()
}

#[track_caller]
fn assert_close(actual: &[u8], expected: &[u8]) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(a, e)| a.abs_diff(*e) <= 2),
        "{actual:?} is not within 2 of {expected:?}"
    );
}

#[test]
fn an_embedded_display_p3_profile_is_converted_to_srgb() {
    // Pure P3 red is outside sRGB and clips to pure red; the others move.
    let colours = [[255, 0, 0], [200, 100, 50], [64, 160, 224]];
    let mut pixels: Vec<u8> = colours
        .iter()
        .flat_map(|[r, g, b]| [*r, *g, *b, 255])
        .collect();
    pixels.extend([200, 100, 50, 128]);
    let size = (4, 1);

    let mut webp = Vec::new();
    let mut encoder = WebPEncoder::new_lossless(&mut webp);
    encoder.set_icc_profile(display_p3()).unwrap();
    encoder
        .write_image(&pixels, size.0, size.1, ExtendedColorType::Rgba8)
        .unwrap();
    let png = encode_png(&pixels, size, ExtendedColorType::Rgba8, Some(&display_p3()));

    for bytes in [png, webp] {
        let out = pixels_of(&bytes);
        for (index, colour) in colours.into_iter().enumerate() {
            assert_close(&out[index * 4..][..3], &p3_to_srgb(colour));
            assert_eq!(out[index * 4 + 3], 255);
        }
        // Alpha is kept; the scene image premultiplies after the conversion.
        assert_close(
            &out[12..15],
            &premultiplied(p3_to_srgb([200, 100, 50]), 128),
        );
        assert_eq!(out[15], 128);
    }
    assert_eq!(p3_to_srgb([200, 100, 50]), [215, 93, 31]);
}

#[test]
fn sixteen_bit_images_are_converted_with_a_sixteen_bit_transform() {
    let samples = [200u16 * 257, 100 * 257, 50 * 257, u16::MAX];
    let pixels: Vec<u8> = samples
        .iter()
        .flat_map(|sample| sample.to_ne_bytes())
        .collect();
    let png = encode_png(
        &pixels,
        (1, 1),
        ExtendedColorType::Rgba16,
        Some(&display_p3()),
    );
    let out = pixels_of(&png);
    assert_close(&out[..3], &p3_to_srgb([200, 100, 50]));
    assert_eq!(out[3], 255);
}

#[test]
fn a_greyscale_profile_is_applied_to_greyscale_images() {
    let profile = ColorProfile::new_gray_with_gamma(1.8).encode().unwrap();
    let png = encode_png(
        &[64, 255, 192, 128],
        (2, 1),
        ExtendedColorType::La8,
        Some(&profile),
    );
    let out = pixels_of(&png);
    // Gamma 1.8 grey (the old Mac default) is lighter than the same codes in sRGB.
    for (index, (code, alpha)) in [(64u8, 255u8), (192, 128)].into_iter().enumerate() {
        let linear = (f64::from(code) / 255.0).powf(1.8);
        let grey = (linear_to_srgb(linear) * 255.0).round() as u8;
        let expected = premultiplied([grey; 3], alpha);
        assert_close(&out[index * 4..][..3], &expected);
        assert_eq!(out[index * 4 + 3], alpha);
    }
    assert!(out[0].abs_diff(64) >= 10, "the grey must have moved");
}

#[test]
fn a_jpeg_with_a_display_p3_profile_is_converted_to_srgb() {
    let flat: Vec<u8> = [200, 100, 50].repeat(8 * 8);
    let mut with_profile = Vec::new();
    let mut encoder = JpegEncoder::new(&mut with_profile);
    encoder.set_icc_profile(display_p3()).unwrap();
    encoder
        .write_image(&flat, 8, 8, ExtendedColorType::Rgb8)
        .unwrap();
    let mut plain = Vec::new();
    JpegEncoder::new(&mut plain)
        .write_image(&flat, 8, 8, ExtendedColorType::Rgb8)
        .unwrap();

    // Lossy, so compare against what the same file decodes to without the profile.
    let untagged = pixels_of(&plain);
    let converted = pixels_of(&with_profile);
    assert_close(
        &converted[..3],
        &p3_to_srgb([untagged[0], untagged[1], untagged[2]]),
    );
    assert!(converted[0].abs_diff(untagged[0]) > 8);
}

#[test]
fn srgb_missing_and_unusable_profiles_leave_the_pixels_alone() {
    let pixels = palette_rgba();
    let decode_with = |icc: Option<&[u8]>| {
        pixels_of(&encode_png(
            &pixels,
            (PALETTE.len() as u32, 1),
            ExtendedColorType::Rgba8,
            icc,
        ))
    };
    assert_eq!(decode_with(None), pixels, "no profile");

    let srgb = ColorProfile::new_srgb();
    assert_eq!(decode_with(Some(&srgb.encode().unwrap())), pixels, "sRGB");

    // Most tools write sRGB with a sampled curve instead of a formula.
    let table: Vec<u16> = (0..1024)
        .map(|index| (srgb_to_linear(f64::from(index) / 1023.0) * 65535.0).round() as u16)
        .collect();
    let mut sampled = ColorProfile::new_srgb();
    sampled.cicp = None;
    for curve in [
        &mut sampled.red_trc,
        &mut sampled.green_trc,
        &mut sampled.blue_trc,
    ] {
        *curve = Some(ToneReprCurve::Lut(table.clone()));
    }
    assert_eq!(
        decode_with(Some(&sampled.encode().unwrap())),
        pixels,
        "sRGB with a sampled curve"
    );

    let p3 = display_p3();
    for (what, icc) in [
        ("garbage", vec![0xAB; 300]),
        ("a truncated profile", p3[..200].to_vec()),
        (
            "a profile for another colour space",
            ColorProfile::new_lab().encode().unwrap(),
        ),
        (
            "a grey profile on colour pixels",
            ColorProfile::new_gray_with_gamma(1.8).encode().unwrap(),
        ),
    ] {
        assert_eq!(decode_with(Some(&icc)), pixels, "{what}");
    }
}

/// Solid frames of 3 by 2 pixels.
fn frame_image(color: Rgba<u8>) -> RgbaImage {
    RgbaImage::from_pixel(3, 2, color)
}

const RED: Rgba<u8> = Rgba([255, 0, 0, 255]);
const GREEN: Rgba<u8> = Rgba([0, 255, 0, 255]);
const BLUE: Rgba<u8> = Rgba([0, 0, 255, 255]);

fn gif(frames: &[(Rgba<u8>, u32)], repeat: Repeat) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = GifEncoder::new(&mut bytes);
    encoder.set_repeat(repeat).unwrap();
    for (color, millis) in frames {
        let delay = Delay::from_numer_denom_ms(*millis, 1);
        encoder
            .encode_frame(Frame::from_parts(frame_image(*color), 0, 0, delay))
            .unwrap();
    }
    drop(encoder);
    bytes
}

fn milliseconds(image: &glassmere_scene::Image) -> Vec<u128> {
    image
        .frames()
        .iter()
        .map(|frame| frame.delay.as_millis())
        .collect()
}

/// The first pixel of each frame.
fn first_pixels(image: &glassmere_scene::Image) -> Vec<[u8; 4]> {
    image
        .frames()
        .iter()
        .map(|frame| frame.image.pixels().unwrap().2[..4].try_into().unwrap())
        .collect()
}

#[test]
fn an_animated_gif_keeps_its_frames_delays_and_loop_count() {
    let frames = [(RED, 100), (GREEN, 200), (BLUE, 300)];
    let image = decode(&gif(&frames, Repeat::Infinite)).unwrap();
    assert_eq!(image.frames().len(), 3);
    assert_eq!(milliseconds(&image), [100, 200, 300]);
    assert_eq!(image.loops(), None);
    assert_eq!(
        first_pixels(&image),
        [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
    );
    // The animation stands in for its first frame everywhere else.
    assert_eq!(image.size(), glassmere_scene::Size::new(3.0, 2.0));
    assert_eq!(image.pixels().unwrap().2[..4], [255, 0, 0, 255]);

    // A GIF counts the repeats after the first play.
    let twice = decode(&gif(&frames, Repeat::Finite(2))).unwrap();
    assert_eq!(twice.loops(), NonZeroU32::new(3));

    // A frame that asks for next to nothing lasts a tenth of a second.
    let fast = decode(&gif(&[(RED, 0), (GREEN, 10), (BLUE, 20)], Repeat::Infinite)).unwrap();
    assert_eq!(milliseconds(&fast), [100, 100, 20]);
}

#[test]
fn a_one_frame_gif_is_a_still_image_as_before() {
    let image = decode(&gif(&[(RED, 100)], Repeat::Infinite)).unwrap();
    assert!(image.frames().is_empty() && image.loops().is_none());
    assert_eq!(image.pixels().unwrap().2[..4], [255, 0, 0, 255]);
    assert_eq!(image.size(), glassmere_scene::Size::new(3.0, 2.0));
}

fn lossless_webp(image: &RgbaImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::WebP)
        .unwrap();
    bytes
}

fn riff_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = kind.to_vec();
    chunk.extend((data.len() as u32).to_le_bytes());
    chunk.extend(data);
    chunk.resize(chunk.len() + data.len() % 2, 0);
    chunk
}

/// An animated WebP of `count` copies of the one lossless frame in the corner
/// of a `canvas`, with `delays`
/// cycled, which no encoder in the `image` crate writes.
fn animated_webp(
    frame: &RgbaImage,
    canvas: (u32, u32),
    delays: &[u32],
    count: usize,
    loops: u16,
) -> Vec<u8> {
    let (width, height) = frame.dimensions();
    let still = lossless_webp(frame);
    assert_eq!(&still[12..16], b"VP8L");
    let three = |value: u32| value.to_le_bytes()[..3].to_vec();
    let mut header = vec![0x12, 0, 0, 0];
    header.extend(three(canvas.0 - 1));
    header.extend(three(canvas.1 - 1));
    let mut body = b"WEBP".to_vec();
    body.extend(riff_chunk(b"VP8X", &header));
    let animation = [&[0u8; 4][..], &loops.to_le_bytes()].concat();
    body.extend(riff_chunk(b"ANIM", &animation));
    for index in 0..count {
        let mut data = three(0);
        data.extend(three(0));
        data.extend(three(width - 1));
        data.extend(three(height - 1));
        data.extend(three(delays[index % delays.len()]));
        data.push(0);
        data.extend(&still[12..]);
        body.extend(riff_chunk(b"ANMF", &data));
    }
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((body.len() as u32).to_le_bytes());
    bytes.extend(body);
    bytes
}

#[test]
fn an_animated_webp_keeps_its_frames_delays_and_loop_count() {
    let frame = frame_image(GREEN);
    let image = decode(&animated_webp(&frame, (3, 2), &[50, 120], 3, 4)).unwrap();
    assert_eq!(image.frames().len(), 3);
    assert_eq!(milliseconds(&image), [50, 120, 50]);
    // WebP counts plays, and 0 is forever.
    assert_eq!(image.loops(), NonZeroU32::new(4));
    // Compositing a frame over the canvas can be a level off.
    for pixel in first_pixels(&image) {
        assert!(pixel[1] >= 254 && pixel[0] == 0 && pixel[2] == 0 && pixel[3] == 255);
    }
    let forever = decode(&animated_webp(&frame, (3, 2), &[50], 2, 0)).unwrap();
    assert_eq!(forever.loops(), None);

    // A still WebP is not an animation.
    let still = decode(&lossless_webp(&frame)).unwrap();
    assert!(still.frames().is_empty());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend(kind);
    chunk.extend(data);
    chunk.extend(crc32(&chunk[4..]).to_be_bytes());
    chunk
}

/// The chunks of a PNG file as (kind, data).
fn png_chunks(bytes: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut chunks = Vec::new();
    let mut at = 8;
    while at < bytes.len() {
        let length = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let kind = bytes[at + 4..at + 8].try_into().unwrap();
        chunks.push((kind, bytes[at + 8..at + 8 + length].to_vec()));
        at += length + 12;
    }
    chunks
}

/// An APNG whose animation starts with its default image, `plays` times.
fn animated_png(frames: &[(Rgba<u8>, u16)], plays: u32) -> Vec<u8> {
    let mut bytes = vec![137, 80, 78, 71, 13, 10, 26, 10];
    let mut sequence = 0u32;
    for (index, (color, millis)) in frames.iter().enumerate() {
        let mut file = Vec::new();
        frame_image(*color)
            .write_to(&mut Cursor::new(&mut file), ImageFormat::Png)
            .unwrap();
        let chunks = png_chunks(&file);
        let data: Vec<u8> = chunks
            .iter()
            .filter(|(kind, _)| kind == b"IDAT")
            .flat_map(|(_, data)| data.clone())
            .collect();
        if index == 0 {
            bytes.extend(png_chunk(b"IHDR", &chunks[0].1));
            let mut control = (frames.len() as u32).to_be_bytes().to_vec();
            control.extend(plays.to_be_bytes());
            bytes.extend(png_chunk(b"acTL", &control));
        }
        let mut control = sequence.to_be_bytes().to_vec();
        for value in [3u32, 2, 0, 0] {
            control.extend(value.to_be_bytes());
        }
        control.extend(millis.to_be_bytes());
        control.extend(1000u16.to_be_bytes());
        control.extend([0, 0]);
        bytes.extend(png_chunk(b"fcTL", &control));
        sequence += 1;
        if index == 0 {
            bytes.extend(png_chunk(b"IDAT", &data));
        } else {
            let mut frame = sequence.to_be_bytes().to_vec();
            frame.extend(data);
            bytes.extend(png_chunk(b"fdAT", &frame));
            sequence += 1;
        }
    }
    bytes.extend(png_chunk(b"IEND", &[]));
    bytes
}

#[test]
fn an_animated_png_keeps_its_frames_delays_and_loop_count() {
    let image = decode(&animated_png(&[(RED, 40), (BLUE, 250)], 2)).unwrap();
    assert_eq!(milliseconds(&image), [40, 250]);
    assert_eq!(image.loops(), NonZeroU32::new(2));
    assert_eq!(first_pixels(&image), [[255, 0, 0, 255], [0, 0, 255, 255]]);
    let forever = decode(&animated_png(&[(RED, 40), (BLUE, 40)], 0)).unwrap();
    assert_eq!(forever.loops(), None);

    // An ordinary PNG is not an animation.
    let plain = decode(&encoded(ImageFormat::Png)).unwrap();
    assert!(plain.frames().is_empty());
}

#[test]
fn frames_beyond_the_memory_cap_leave_the_first_frame_as_a_still() {
    // A 2000 by 2000 canvas is 16 MB a frame: two fit in 32 MiB, three do not.
    let (frame, canvas) = (RgbaImage::from_pixel(1, 1, RED), (2000, 2000));
    let fits = decode(&animated_webp(&frame, canvas, &[100], 2, 0)).unwrap();
    assert_eq!(fits.frames().len(), 2);
    let too_long = decode(&animated_webp(&frame, canvas, &[100], 3, 0)).unwrap();
    assert!(too_long.frames().is_empty());
    assert_eq!(too_long.size(), glassmere_scene::Size::new(2000.0, 2000.0));
    assert!(too_long.pixels().unwrap().2[0] >= 254);
}
