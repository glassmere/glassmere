use glassmere_image::{Loader, decode, load};
use image::codecs::{jpeg::JpegEncoder, png::PngEncoder, webp::WebPEncoder};
use image::{ExtendedColorType, ImageEncoder, ImageFormat, Rgba, RgbaImage};
use moxcms::{ColorProfile, ToneReprCurve};
use std::io::Cursor;
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
