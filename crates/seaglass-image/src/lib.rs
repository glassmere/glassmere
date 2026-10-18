//! Decoding for `seaglass_scene::Image`: PNG, JPEG, WebP and GIF (first frame)
//! through the `image` crate, SVG through `resvg`, and a background loader so
//! files never decode on the UI thread.
use resvg::{tiny_skia, usvg};
use seaglass_scene::{Image, Size, VectorImage};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};

/// Decodes an image file's bytes. The format is detected from the content;
/// anything that is not a known raster format is tried as SVG.
pub fn decode(bytes: &[u8]) -> Result<Image, String> {
    if image::guess_format(bytes).is_ok() {
        raster(bytes)
    } else {
        svg(bytes)
    }
}

/// Reads and decodes a file; errors name the path.
pub fn load(path: &Path) -> Result<Image, String> {
    std::fs::read(path)
        .map_err(|error| error.to_string())
        .and_then(|bytes| decode(&bytes))
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn raster(bytes: &[u8]) -> Result<Image, String> {
    use image::ImageDecoder;
    let text = |error: image::ImageError| error.to_string();
    // The reader's default limits cap allocations for hostile files.
    let mut decoder = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?
        .into_decoder()
        .map_err(text)?;
    // Cameras store rotation as metadata instead of rotating the pixels.
    let orientation = decoder.orientation().map_err(text)?;
    let mut decoded = image::DynamicImage::from_decoder(decoder).map_err(text)?;
    decoded.apply_orientation(orientation);
    let rgba = decoded.into_rgba8();
    let (width, height) = rgba.dimensions();
    Image::from_rgba8(width, height, rgba.into_raw()).ok_or_else(|| "empty image".to_owned())
}

struct Svg(usvg::Tree);

impl VectorImage for Svg {
    fn size(&self) -> Size {
        Size::new(self.0.size().width(), self.0.size().height())
    }

    fn rasterize(&self, width: u32, height: u32) -> Vec<u8> {
        let Some(mut pixmap) = tiny_skia::Pixmap::new(width, height) else {
            return Vec::new();
        };
        let size = self.0.size();
        let scale = tiny_skia::Transform::from_scale(
            width as f32 / size.width(),
            height as f32 / size.height(),
        );
        resvg::render(&self.0, scale, &mut pixmap.as_mut());
        // tiny-skia pixmaps are premultiplied RGBA8, which is what scenes hold.
        pixmap.take()
    }
}

fn svg(bytes: &[u8]) -> Result<Image, String> {
    let mut options = usvg::Options::default();
    // System fonts are scanned once, and only for SVGs with text;
    // icons never pay for it. Bundle a font database here if apps need
    // identical SVG text on every machine.
    if bytes.windows(5).any(|window| window == b"<text") {
        static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
        options.fontdb = FONTS
            .get_or_init(|| {
                let mut fonts = usvg::fontdb::Database::new();
                fonts.load_system_fonts();
                Arc::new(fonts)
            })
            .clone();
    }
    let tree = usvg::Tree::from_data(bytes, &options)
        .map_err(|error| format!("not a supported image: {error}"))?;
    Ok(Image::from_vector(Svg(tree)))
}

/// Loads image files on one background thread. `done` runs on that thread
/// after each file, to wake the UI thread, which then drains `finished`.
// One worker; a pool if photo galleries need parallel decoding.
pub struct Loader {
    requests: Sender<PathBuf>,
    results: Receiver<(PathBuf, Result<Image, String>)>,
}

impl Loader {
    pub fn new(done: impl Fn() + Send + 'static) -> Self {
        let (requests, inbox) = channel::<PathBuf>();
        let (outbox, results) = channel();
        std::thread::Builder::new()
            .name("seaglass-image".into())
            .spawn(move || {
                // Ends when the loader is dropped and the channel closes.
                for path in inbox {
                    let result = load(&path);
                    if outbox.send((path, result)).is_err() {
                        break;
                    }
                    done();
                }
            })
            .expect("image loader thread");
        Self { requests, results }
    }

    pub fn request(&self, path: PathBuf) {
        // The worker only stops with the loader, so this cannot fail.
        let _ = self.requests.send(path);
    }

    /// Files loaded since the last call, in request order.
    pub fn finished(&self) -> impl Iterator<Item = (PathBuf, Result<Image, String>)> + '_ {
        self.results.try_iter()
    }
}
