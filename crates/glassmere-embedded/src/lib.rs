//! A software renderer for microcontroller-class displays: paints a
//! [`glassmere_scene::Scene`] into a plain pixel buffer, with bitmap fonts and
//! a [`TextMeasurer`](glassmere_scene::TextMeasurer) that agrees with them.
//!
//! The crate is `#![no_std]` and uses `alloc` for scratch memory that it keeps
//! between frames. There is no floating-point hardware assumption beyond what
//! `f32` costs on the target; the functions `core` lacks come from `libm`.
//!
//! ```
//! use glassmere_embedded::{EmbeddedMeasurer, EmbeddedRenderer, Framebuffer, Rgb565};
//! use glassmere_scene::{Color, Primitive, Rect, Scene, Size};
//!
//! let scene = Scene {
//!     size: Size::new(32.0, 16.0),
//!     primitives: vec![
//!         Primitive::Quad {
//!             rect: Rect { x: 2.0, y: 2.0, width: 28.0, height: 12.0 },
//!             color: Color::rgb(30, 90, 200),
//!             radius: 4.0,
//!         },
//!         Primitive::Text {
//!             rect: Rect { x: 6.0, y: 2.0, width: 24.0, height: 13.0 },
//!             text: "Hi".into(),
//!             font_size: 13.0,
//!             color: Color::WHITE,
//!         },
//!     ],
//! };
//! let mut pixels = vec![Rgb565::default(); 32 * 16];
//! let mut frame = Framebuffer::new(&mut pixels, 32, 16);
//! let mut renderer = EmbeddedRenderer::default();
//! renderer.render(&scene, &mut frame, Some(Color::rgb(0, 0, 0)));
//! assert_ne!(frame.pixel(16, 8), Some(Rgb565::default()));
//! // Lay the scene out with the renderer's own measurer so text fits.
//! let _measurer: EmbeddedMeasurer = renderer.measurer();
//! ```
//!
//! # What is drawn
//!
//! Positions are device pixels: there is no scale factor, and a scene is
//! laid out for the display it is drawn on. Colours are blended as the sRGB
//! numbers they are, not in linear light, so blended edges are a little
//! darker than on the other renderers; opaque areas are identical.
//!
//! | Primitive | Support |
//! | --- | --- |
//! | `Quad` | Full: anti-aliased rounded corners, a fast path for opaque rows |
//! | `Text`, `StyledText`, `RichText` | Bitmap fonts on a character grid, see below |
//! | `Image` | Raster images: source rectangle, nearest or bilinear, tint, rounded corners. Vector images are skipped |
//! | `Gradient` | Linear and radial, up to eight stops |
//! | `Shadow` | Approximated from the distance to the shape's edge; can be turned off |
//! | `PushClip`, `PopClip` | Rectangles, rounded outward to whole pixels |
//! | `PushLayer`, `PopLayer` | Without an offscreen buffer: the opacity multiplies into everything inside, a move is applied, and scale, rotation and skew are ignored |
//! | `Path` | Fills by the non-zero rule and strokes with mitred or bevelled corners |
//! | `Custom` | Skipped; draw into the buffer yourself afterwards |
//!
//! Text is left to right whatever its style says. The face is the one whose
//! cell height is nearest the style's font size, bold from a weight of 600;
//! the family and italic are ignored. Every character takes one cell, lines
//! break at `\n` and, for wrapping styles, at spaces, and a character the font
//! lacks is drawn as a box. Characters that combine with the one before them
//! share its cell and are not drawn. [`EmbeddedMeasurer`] and the renderer
//! lay text out with the same code, so measured lines are painted lines.
//!
//! # Memory
//!
//! A frame can be rendered in bands or in pieces: [`Framebuffer`] says where
//! on the screen its buffer sits, and [`EmbeddedRenderer::render`] paints the
//! part of the scene that falls in it, giving the pixels a full frame has there.
//! [`EmbeddedRenderer::render_damage`] repaints one rectangle of a buffer.
//!
//! # Fonts
//!
//! [`FontSet::BUILTIN`] has 6x13 and 9x18 faces, regular and bold, about 35 KiB
//! in all. `examples/bdf_convert.rs` makes more from BDF files; see the README.
#![no_std]

extern crate alloc;

mod font;
mod fonts;
mod framebuffer;
mod math;
mod path;
mod pixel;
mod raster;
mod renderer;
mod shader;
mod text;

pub use font::{BitmapFont, FontFace, FontSet, GlyphRange};
pub use framebuffer::Framebuffer;
pub use pixel::{Gray8, Mono, Pixel, Rgb565, Rgb888, Xrgb8888};
pub use renderer::{EmbeddedRenderer, ImageFilter, RenderOptions};
pub use text::EmbeddedMeasurer;
