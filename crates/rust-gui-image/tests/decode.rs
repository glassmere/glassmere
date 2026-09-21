use image::{ImageFormat, Rgba, RgbaImage};
use rust_gui_image::{Loader, decode, load};
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
    assert_eq!(image.size(), rust_gui_scene::Size::new(20.0, 10.0));
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
    let directory = std::env::temp_dir().join(format!("rust-gui-image-{}", std::process::id()));
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
