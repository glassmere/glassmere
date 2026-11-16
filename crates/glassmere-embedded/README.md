# glassmere-embedded

A software renderer for microcontroller displays. It paints a
[`glassmere_scene::Scene`](https://docs.rs/glassmere-scene) into a plain pixel
buffer (a framebuffer, a band of one, or just the part that changed) and comes
with built-in bitmap fonts and a `TextMeasurer` that lays text out on the same
grid the renderer draws it on.

`#![no_std]` with `alloc`; the only dependencies are `glassmere-scene` and
`libm`, plus `glassmere-core` for `EmbeddedUi` (the default `ui` feature),
which runs an application on the display: input in, damaged pixels out. Unsafe
code is forbidden. CI checks the crate for `thumbv7em-none-eabihf`.

```rust
use glassmere_embedded::{EmbeddedMeasurer, EmbeddedRenderer, Framebuffer, Rgb565};
use glassmere_scene::Color;

let mut renderer = EmbeddedRenderer::default();
let mut measurer = renderer.measurer(); // lay the UI out with this
# let scene = glassmere_scene::Scene::default();
let mut pixels = vec![Rgb565::default(); 320 * 240];
let mut frame = Framebuffer::new(&mut pixels, 320, 240);
renderer.render(&scene, &mut frame, Some(Color::rgb(15, 23, 42)));
```

## Memory

Nothing needs a full frame. `Framebuffer::new(&mut band, 320, 30)` is a
30-row buffer, and `set_origin(0, y)` says where on the screen it sits; render
the scene once per band and send each to the display. A band holds exactly the
pixels the full frame would have there. `EmbeddedRenderer::render_damage`
repaints one rectangle of a buffer and leaves the rest alone.

Pixel formats are `Rgb565`, `Rgb888`, `Xrgb8888`, `Gray8` and `Mono`; add one
by implementing `Pixel`.

## What is drawn

Positions are device pixels, with no scale factor. Colours are blended as
sRGB numbers, not in linear light: cheap, and identical to the other renderers
wherever a pixel is opaque and fully covered, a little darker where blending
shows.

| Primitive | Support |
| --- | --- |
| `Quad` | Full: anti-aliased rounded corners, a fast path for opaque rows |
| `Text`, `StyledText`, `RichText` | Bitmap fonts on a character grid |
| `Image` | Raster only: source rectangle, nearest or bilinear, tint, rounded corners. Vector images are skipped |
| `Gradient` | Linear and radial, up to eight stops, interpolated in sRGB |
| `Shadow` | Approximated from the distance to the edge, exact on straight edges; much smaller shapes than the blur come out too dark. `RenderOptions::shadows` turns them off |
| `PushClip`, `PopClip` | Rectangles, rounded outward to whole pixels |
| `PushLayer`, `PopLayer` | No offscreen buffer: opacity multiplies into everything inside (so overlapping parts of a faded group show through each other), a move is applied, and scale, rotation and skew are ignored |
| `Path` | Fill (non-zero) and stroke; mitre joins up to four widths, bevel beyond, square ends |
| `Custom` | Skipped: draw into the buffer yourself afterwards |

Text is left to right whatever the style says. Each character takes one cell
of the face whose height is nearest the style's `font_size`; a weight of 600 or
more selects the bold glyphs, and the family and italic are ignored. Lines break
at `\n` and, for `TextWrap::Word`, at spaces. A character the font lacks is
drawn as a box. Characters that combine with the one before (accents, joiners)
share its cell and are not drawn. Grapheme boundaries follow that simple rule
instead of full Unicode segmentation, which would need another dependency.

## Fonts

`FontSet::BUILTIN` has two faces, each with a regular and a bold font:

| Face | Cell | Glyphs (regular / bold) | Data |
| --- | --- | --- | --- |
| 6x13 | 6 by 13, ascent 11 | 371 / 351 | 4.7 KiB / 4.5 KiB |
| 9x18 | 9 by 18, ascent 14 | 371 / 351 | 13.0 KiB / 12.3 KiB |

That is 35,378 bytes of glyph data and about 1 KiB of range tables. They sit in
`static`s that are linked in only when something refers to `FontSet::BUILTIN`,
which `EmbeddedRenderer::default()` and `EmbeddedMeasurer::default()` do; a
build that gives its own `FontSet` to `EmbeddedRenderer::new` and
`EmbeddedMeasurer::new` does not carry them. Coverage
is U+0020 to U+007E, Latin-1 and Latin Extended-A (U+00A0 to U+017F), typographic
punctuation, arrows, box and check symbols; see `DEFAULT_RANGES` in the
converter. The bold fonts lack some of the symbols, and draw them from the
regular font.

### Provenance

The glyphs come from the `6x13`, `6x13B`, `9x18` and `9x18B` fonts of Markus
Kuhn's [Unicode fixed fonts](https://www.cl.cam.ac.uk/~mgk25/ucs-fonts.html)
(the `ucs-fonts` package, the `misc-fixed` family, files dated 2004 to 2009).
Each BDF file says `COPYRIGHT "Public domain font.  Share and enjoy."`, and the
package README states that its fonts remain in the public domain. The converted
data is therefore public domain too; the crate's code has the workspace
license. The BDF files and the archive are not part of this repository.

To rebuild the data, download the package, extract it, and run from the
repository root:

```sh
for pair in 6x13:fixed_6x13 6x13B:fixed_6x13_bold 9x18:fixed_9x18 9x18B:fixed_9x18_bold; do
  cargo run -p glassmere-embedded --example bdf_convert -- \
    ucs-fonts/${pair%%:*}.bdf crates/glassmere-embedded/src/fonts ${pair##*:}
done
rustfmt --edition 2024 crates/glassmere-embedded/src/fonts/*.rs
```

The converter takes `<font.bdf> <out-dir> <name> [ranges]`, where `ranges` is a
list such as `20-7E,A0-17F,20AC` of hexadecimal code points to keep. It writes
`<name>.bin`, the rows of every glyph (most significant bit leftmost,
`ceil(width / 8)` bytes per row), and `<name>.rs`, a `static` that wraps it in a
`BitmapFont`. Glyphs that are not as wide as the cell are skipped, since text
is laid out on a grid. Another fixed-width BDF font works the same way; build a
`FontSet` from it with `FontFace::new` and pass it to `EmbeddedRenderer::new`
and `EmbeddedMeasurer::new`.

## Looking at frames

The integration tests paint a real UI at 320x240 in RGB565. With
`EMBEDDED_DUMP=<dir>` set they also write each frame as a PPM file there:

```sh
EMBEDDED_DUMP=/tmp/frames cargo test -p glassmere-embedded --tests
```
