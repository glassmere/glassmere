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

# Milestone 14b: `rust-gui-render-cpu`

Implemented on 2026-09-22. The same display list, rasterized to pixels without a graphics adapter.

```rust
let mut renderer = CpuRenderer::new();
context.run(size, renderer.text_system(), view);
let pixels = renderer.render([800, 600], 1.0, context.scene(), Color::rgb(16, 23, 34));
```

| Piece | Contract |
| --- | --- |
| `CpuRenderer::render(physical_size, scale, scene, clear) -> &[u8]` | RGBA8 in row order, `width * height * 4` bytes. Geometry is in logical pixels and `scale` is physical pixels per logical pixel, as on the GPU. An empty or unusable target gives an empty slice. The target is kept between frames, so a frame of the same size allocates nothing for it. |
| `text_system()` | The measurer to lay the scene out with, so that text fits what is drawn; `with_text_system` takes one built by `TextSystem::with_fonts`. |
| `CpuStats` | Frames, the time the last one took, the offscreen layers it needed and the glyphs it drew. |
| Display list | The same clip stack, the same layers snapped to the pixel grid with the same resolution cap, the same rounding of clip rectangles, the same nine-slice cropping, vector images rasterized for the pixels they cover and cached per size, and layers left open still composited. |
| Colour | Like an sRGB render target, it blends in linear light: half a colour over another is the same number the GPU writes. tiny-skia supplies geometry only, because its own gamma handling mixes premultiplied channels and comes out visibly dark on a faded layer. |
| Clipping | Each draw is painted into a buffer that covers its visible pixels alone, so no clip mask is needed anywhere. |

## Validation

Seven tests in `tests/render.rs`, all on the bundled font so the pixels are the same on every machine: quads filled to the exact pixel, blended and with soft rounded corners; the scale factor and unusable targets; clips nesting, a stray `PopClip` not unclipping, and clips ending; layers fading as one picture, moving, clipping their content and closing themselves at the end of the list; images scaled, cropped to their source without a neighbour bleeding in, tinted and mirrored; vector images rasterized for the pixels they cover; and text drawn where it was measured, in its colour, clipped, and four times as inked at twice the scale.

`tests/compare_gpu.rs` (opt-in, needs an adapter) renders one scene holding every primitive with both renderers at three scale factors and states how close they are. On this machine (NVIDIA GeForce RTX 3070, Direct3D 12): a mean difference of 0.16 to 0.60 per channel, with at most 1.41% of pixels differing by more than 8 and 0.44% by more than 32. What differs is a thin line around shapes and glyphs; flat areas agree exactly. The test also checks that rendering the same scene twice gives the same bytes.

The workspace suite is 255 tests plus 24 opt-in GPU tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features, and `CpuRenderer` is exported there, so a headless build needs no graphics stack.

## Remaining limits

- A quad's edge is covered by its exact area here and feathered by the GPU's shader there, and what is resampled (images, turned or scaled layers) is filtered on sRGB-encoded values here and in linear light there. Both show only at edges.
- Nothing is cached between frames but the target and the vector rasters: every frame repaints the whole scene, and a layer or a clipped draw allocates a buffer of its own. The renderer is correct and available, not fast; there are no damage rectangles and no threads.
- No `RenderStats` counterpart for draw calls or GPU memory, and the inspector's GPU tab stays empty on this renderer.
