# Milestone 14: a CPU renderer

Decisions taken on 2026-09-21, before the work: headless rendering first, then a fallback in the desktop host; tiny-skia, which is already in the tree through resvg, accepting that it blends in sRGB space where the GPU blends in linear light, with the two renderers compared on the same scenes within a stated tolerance; text shaping extracted into a crate both renderers share; the host uses the CPU renderer when asked to and, with a warning, when no GPU works.

Slices: 14a the shared text crate (below), 14b `rust-gui-render-cpu`, 14c the window fallback.

# Milestone 14a: `rust-gui-text`

Implemented on 2026-09-21. Fonts, shaping, measuring and text geometry moved out of `rust-gui-render-wgpu` into a crate of their own, so that a second renderer lays out and draws text from exactly the same shaped blocks, and a CPU-only build does not compile the GPU stack.

| Piece | Contract |
| --- | --- |
| `rust-gui-text` | `TextSystem` (the font system and the cache of shaped blocks), its `TextMeasurer` implementation, `TextSystem::with_fonts`, and what a renderer needs to draw: `TextKey`, `ensure`, `ensure_styled`, `ensure_rich`, `retain`, and the `fonts` and `buffers` fields. Depends on `cosmic-text` 0.18, the version glyphon uses, so both share one font system type; `cosmic_text` is re-exported. |
| `rust-gui-render-wgpu` | Uses the new crate and still exports `TextSystem` under its old path, so hosts and apps did not change. It no longer depends on `unicode-segmentation`. |

The move is mechanical: the file went over with its history and its tests, `glyphon::` paths became `cosmic_text::`, and four methods and two fields became public.

## Validation

Nothing may change, and nothing did: the workspace suite is still 248 tests, the four text tests now run in the new crate, and all 23 GPU tests pass, including the pixel comparison against the bundled-font reference. Formatting and clippy with all features are clean; the facade builds without default features.
