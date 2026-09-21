# Milestone 8: images and assets

The seven roadmap phases are delivered; milestone 8 starts the work that comes from the coverage table. Slices: 8a images and 8b background images with nine-slice (both below). Milestone 8 is complete.

# Milestone 8a: images

Implemented on 2026-09-21. Images are a scene-level value that any widget can draw, a widget with fit modes, tinting and accessible names, a decoder crate for PNG, JPEG, WebP, GIF and SVG, and image files that the desktop host decodes on a background thread, shows a placeholder for and reloads when they change on disk. The GPU renderer packs small images into a shared atlas and rasterizes vector images at the size they are drawn.

## Run

```powershell
cargo run -p rust-gui --example images --release
```

The example shows one PNG in the four fit modes, SVG icons that take the text colour, one SVG badge at three sizes (each rasterized sharp), pixels made in code blending over a panel, and the placeholder of a missing file. Edit or replace a file under `crates/rust-gui/examples/assets` while it runs and it reloads within half a second.

## Use

```rust
// A file: decoded in the background, a placeholder until then, reloaded on change.
ui.image_file(
    "photo",
    "assets/photo.jpg",
    ImageOptions::new("Hills at dusk")            // accessible name; empty = decorative
        .fit(ImageFit::Cover)
        .style(Style::default().width(Length::Px(240.0)).height(Length::Px(160.0)).radius(12.0)),
);

// An icon that follows the theme, hover and disabled colours.
ui.image_file("search", "assets/search.svg", ImageOptions::default().tint(ImageTint::Foreground));

// Pixels or a decoded value the app owns.
let logo = rust_gui::image::decode(include_bytes!("logo.png"))?;   // once, not per frame
ui.image("logo", &logo, ImageOptions::new("Acme"));
```

## Contracts

| Piece | Contract |
| --- | --- |
| `Image` (scene) | Immutable, cheap to clone, equal by identity. `from_rgba8` takes straight RGBA as decoders produce and premultiplies; `from_premultiplied` takes it as is; both return `None` for a zero side or a wrong buffer length. `from_vector` wraps a `VectorImage`. `id()` is what renderers key caches on. |
| `VectorImage` (scene) | `size()` in logical pixels and `rasterize(width, height) -> premultiplied RGBA8`. Renderers call it for the device pixels an image covers and cache the result per image and size. |
| `Primitive::Image { rect, image, source, tint, radius }` | Draws the `source` part of the image (its own units) into `rect` with linear filtering. `tint` replaces the colour and keeps alpha. `radius` rounds `rect`. Fit modes and nine-slice are expressed through `rect` and `source`, so custom widgets can emit it too. |
| `Ui::image(key, &Image, ImageOptions)` | Natural size without a style size; with only a width or only a height the other side follows the aspect ratio (an explicit `aspect_ratio` wins); with both, `fit` decides: `Contain` (default), `Cover`, `Fill`, `None`. Padding insets the image, `radius` rounds it, `opacity` and transforms apply as to any node. |
| `ImageTint` | `None`, `Foreground` (the inherited text colour, so icons follow themes and states) or `Color`. |
| Accessibility | A non-empty `alt` makes the node `Role::Image` with that name. An empty `alt` marks decoration: a plain group without a name. Images are never focusable. |
| `Ui::image_file(key, path, options)` | Asks the host for the file. Until it arrives the node paints a placeholder box (give the style a size to keep the layout still); once it arrives the node sizes like `Ui::image`. If the first load fails the placeholder stays and shows `alt`; if a reload fails the last good image stays. `Context::image_error(path)` holds the reason. |
| Host protocol | `Context::image_paths()` lists what the last declaration asked for, `image_known(path)` whether it was delivered, `set_image_file(path, Result<Image, String>)` delivers. Test harnesses and other hosts call these directly. Decoded files that no declaration asks for any more are kept until they exceed 64 MiB, then dropped together. |
| Desktop host | One background thread per window, started by the first image file, reads and decodes; the event loop is woken when a file is done and the next frame shows it. Files are checked for a new modification time on the same 500 ms tick as stylesheets and watched files. Without the `images` feature every image file reports an error. |
| `rust-gui-image` (`rust_gui::image`, feature `images`, on by default) | `decode(bytes)`, `load(path)`, `Loader`. PNG, JPEG, WebP and GIF (first frame) through the `image` crate with its default allocation limits and EXIF orientation applied; anything else is tried as SVG (also `.svgz`) through `resvg`. System fonts are scanned once and only for SVGs that contain text. |

## Renderer

| Piece | Behavior |
| --- | --- |
| Atlas | Images whose larger side is at most 512 texels share 2048 by 2048 pages (clamped to the device limit), shelf-packed, each with a 1 px extruded border. The shader also clamps sampling to texel centres inside `source`, so neighbours and nine-slice pieces never bleed. Larger images get their own texture; images beyond the device limit are halved on the CPU until they fit. |
| Vector images | Rasterized for `rect` times the surface scale, cached per image and pixel size, so one SVG is sharp at every size and DPI. |
| Colour | Textures are `Rgba8UnormSrgb`: an opaque texel reads back byte-identical to a `Quad` of the same colour. Translucent texels are re-encoded at upload so filtering and blending are gamma-correct and match layers. Blending is premultiplied. A tint replaces RGB with the tint times the image's alpha, scaled by the tint's alpha. |
| Order and batching | Images interleave with quads, text and layers in painter's order, work inside offscreen layers and respect clips. Consecutive images on one texture and clip batch into one draw. A cached replay touches neither the cache nor the GPU upload path. |
| Memory | Upload once per image. A texture not drawn for 120 preparations is dropped. `Renderer::set_image_budget` (default 256 MiB): over budget, whatever the previous preparation did not draw goes first, then atlas pages are dropped and refilled. Images on screen are never dropped. |
| `RenderStats` | `image_draws`, `image_uploads` (cumulative), `atlas_pages`, `image_textures` (standalone), `image_bytes`. |

## Validation

Nineteen new tests. Scene: premultiplication, validation and identity (1). Decoder crate: the four raster formats to premultiplied pixels, SVG as a vector image rasterized at the requested size, garbage and truncated input as errors, and the background loader reporting results and failures by path (4). Core: fit placement for all modes (1 unit test); natural size, one-sided aspect ratio, cover cropping, tint, radius and roles; foreground tint following inherited colour; the placeholder, first-load failure showing the name, delivery, and a failed reload keeping the last good image; sizing after load and the 64 MiB unused budget (4). Renderer: shelf packing, translucent re-encoding and halving (3 unit tests) and six GPU tests: texels and sub-rectangles in exactly the colours quads have; tint and rounded corners; no bleeding between atlas neighbours, batched into one draw; standalone textures and budget eviction; vector rasterization per size with byte-identical cached replay; images inside a half-opaque layer, under clips and in painter's order. The existing pixel reference is unchanged.

The full suite is 173 tests plus 17 GPU tests; clippy is clean with all features, and the facade builds without default features and with `desktop` alone (no decoder). All thirteen examples pass `--smoke` and the theming script reports `THEMING_OK`.

Checked by eye in a release build: the four fit modes, tinted icons, one SVG sharp at 24, 64 and 128 px, translucent pixels over a panel, and the missing-file placeholder with its name. Live reload was exercised for real: `search.svg` was overwritten while the example ran and the new picture was on screen at the next capture three seconds later.

## Remaining limits

- No mipmaps: an image drawn far below its natural size aliases. Resize large photos before showing thumbnails, or use an SVG.
- Animated GIF and WebP show their first frame.
- Colour profiles are ignored; pixels are treated as sRGB.
- Vector images rasterize on the render thread the first time each size is drawn; an SVG animated through many sizes re-rasterizes at each.
- One decoder thread per window; a gallery of large photos loads one file at a time.
- Image files are polled by modification time, like other watched files; there is no OS file notification.
- The clipboard still carries text only.

# Milestone 8b: background images and nine-slice

Implemented on 2026-09-21. Any node can paint an image over its background colour and under its content, from a decoded `Image` or from a file the host loads, fitted like an image widget or nine-sliced so that one small frame image skins boxes of any size. Stylesheets describe the same thing with four `background-` properties. This completes milestone 8.

## Use

```rust
// A photo behind a card's content.
let card = Style::default()
    .radius(14.0)
    .background_image(BackgroundImage::file("assets/dusk.png"));          // Cover by default

// A 24 px skin stretched over a button of any size: corners keep their shape.
let skinned = Style::default()
    .background_image(BackgroundImage::new(&skin).slice(8.0, 8.0, 8.0, 8.0));
```

```text
class hero {
    background-image: url(assets/dusk.png)     // relative to the stylesheet's own file
    background-fit: cover                      // cover | contain | fill | none
}
class frame {
    background-image: url(assets/frame.svg)
    background-slice: 12                       // 1, 2 (vertical horizontal) or 4 (top right bottom left) numbers, or none
    background-tint: foreground                // foreground | #rrggbb[aa] | none
}
```

## Contracts

| Piece | Contract |
| --- | --- |
| `Style::background_image: Option<BackgroundImage>` | Painted across the node's bounds after the background colour and before text, controls and children. Without a slice it is placed by `fit` (default `Cover`) and rounded by the node's `radius`; opacity, transforms and clipping apply as to the rest of the node. |
| `ImageSource::{Image, File}` | A file is requested from the host exactly like `Ui::image_file` (same background decoding, reload on change, error reporting and cache) and nothing is painted until it arrives. It is resolved when the element is declared, so the arrival is an ordinary style change that repaints just that node. |
| `slice: [top, right, bottom, left]` | Insets in image units. The four corners are drawn at their own size, the four edges stretch along one axis, the centre along both. Insets that overlap in the image, or borders wider than the box, are scaled down proportionally; pieces with no area are skipped. `radius` is ignored: the image brings its own shape. Works for vector images too. |
| `tint` | As for image widgets, including `Foreground`, so one frame image follows themes and states. |
| Stylesheet | `background-image: url(file)` or `none`; `background-fit`, `background-slice` and `background-tint` need an image before them and are kept when a derived class replaces only the image. `StyleSheet::rebase(directory)` makes relative files relative to the sheet; the desktop host calls it for `AppOptions::stylesheet`, so sheets work wherever the app is started from. Errors carry line numbers like all other properties. |
| Renderer | Images with `radius` 0 are no longer anti-aliased at their edges: abutting pieces each covered the shared boundary halfway and left a visible seam. The rasterizer now gives every boundary pixel to exactly one piece. Rounded images are anti-aliased as before. |

The `images` example gained a row with a stylesheet photo card, one SVG frame over three box sizes and a 24 px raster skin made in code.

## Validation

Five new tests. Core: nine-slice geometry (corners fixed, edges and centre stretched, a box narrower than its borders, insets larger than the image); a background painted between the colour and the content, `Cover` and the node's radius by default, nine tinted square pieces for a slice; a background file requested from the host, invisible until delivered and painted after; stylesheet parsing with `rebase`, inheritance between classes, and four error messages. Renderer: square images that abut at whole and fractional device pixels, at scale 1 and 1.25, leave no seam (GPU test).

The seam was found by eye in the example, not by a test: the first nine-slice screenshot showed dark lines at every slice boundary. The fix and its GPU test followed, and the second screenshot is clean for the SVG frame at three sizes and the raster skin.

While repeating the GPU suite a second problem showed up: with eighteen tests the default harness (one thread per core, 24 here) created eighteen Direct3D 12 devices at once and the run hung about half the time, at device creation, before any test finished. Subsets and `--test-threads=4` never hung. GPU tests now take turns behind one lock; eight consecutive default runs passed in 9 s each. Apps create one device and are not affected.

The full suite is 177 tests plus 18 GPU tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features. All thirteen examples pass `--smoke` and the theming script reports `THEMING_OK`.

## Remaining limits

- One background image per node; no tiling (`repeat`), no position other than centred, no gradients.
- Pseudo-states cannot swap the image (`hover.background-image`); tint through `foreground` state colours instead.
- A nine-sliced vector image is rasterized once per distinct piece scale, so a frame used at many box sizes fills the cache with edge rasters until they age out.
- Square images snap to device pixels at their edges instead of fading; a lone unrounded image moving by fractions of a pixel steps rather than glides.
- Background files share the image widget's limits from 8a: no mipmaps, first frame only, sRGB assumed.
