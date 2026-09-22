# Milestone 18: gradients, shadows and the application's own drawing

Implemented on 2026-09-22. The renderers could fill rounded rectangles, draw text and draw images. They can now fill with a ramp of colours, cast and inset shadows, and hand a rectangle to the application to draw with its own GPU code.

## Gradients and shadows

```rust
ui.container("card", Style::default()
    .radius(12.0)
    .gradient(Gradient::down(theme.accent, theme.control))
    .shadow(Shadow::lift(Color(0, 0, 0, 150), 10.0, 6.0)), |ui| { .. });
```

| Piece | Contract |
| --- | --- |
| `Primitive::Gradient` | A rounded rectangle filled with a ramp. `from` and `to` are in the rectangle's own units, where (0, 0) is its top left corner and (1, 1) its bottom right; a linear ramp runs from one to the other and a radial one outward from `from` to the distance of `to`, which makes it an ellipse in a rectangle that is not square. Fewer than two stops draws nothing, and only the first eight are kept (`MAX_GRADIENT_STOPS`). |
| `Primitive::Shadow` | A blurred rounded rectangle, behind the node or inside its edges. `blur` is a Gaussian standard deviation in logical pixels, `spread` grows the rectangle before blurring, `offset` moves it, `inset` turns it inward. |
| `Style::gradient`, `Style::shadow` | On any node. The shadow is painted before the node's own fill, or after it when inset; the gradient fills over the flat background and under a background image. |
| Stylesheets | `shadow: 0 4 12 2 #00000080` is offset, blur, an optional spread, a colour and an optional `inset`. `gradient: 0 0 1 1 #ff0000 #00ff00 #0000ff` is the two points and the colours, spread evenly; `radial` first makes it radial; `none` removes either. |
| Colour | Stops are mixed in linear light, as the GPU's sRGB targets blend: half of red and blue is 188, not the 128 an sRGB-space mix gives. |
| The approximation | Shadows are one draw, not a blur pass: an error function integrates the Gaussian along one axis against the rounded profile, sampled four times along the other (Evan Wallace's method). Both renderers use the same formula, so they agree. Measured error against the analytic value: 0.003 of alpha, which is five levels near black and less than one for anything lighter. |

## The application's own drawing

| Piece | Contract |
| --- | --- |
| `Primitive::Custom { rect, id }` | A rectangle the renderer does not draw. Put one in a `CustomWidget` and the node's position moves it, like any other primitive. |
| `Renderer::set_custom_draw(id, Some(callback))` | wgpu only. The callback is given the device, the queue, the render pass, the rectangle in logical pixels, the scale factor and the target size, with the scissor already set to the rectangle's clipped area. Passing `None` removes it. An id with no callback draws nothing and does not disturb the frame. |
| `AppOptions::gpu` | A `GpuSetup` runs once the window's renderer exists, which is where an application registers its callbacks. Nothing runs on a window painted without a graphics adapter. |
| `CpuRenderer` | Leaves a `Custom` rectangle untouched: drawing with wgpu is a wgpu thing. |

## Validation

Five GPU tests cover the new primitives: a linear ramp whose stops mix in linear light (the middle measured exactly 188, against 128 for an sRGB mix) with the ninth stop of a ten-stop ramp proved to be dropped; a radial ramp from its centre to the far corner in all four directions; a shadow that fades outward and ends three deviations out, with spread, offset and the frame reproduced by the shift; an inset shadow that darkens the inside edge and leaves the middle and the outside alone; and a custom draw that receives the right rectangle, scale and target, is confined by the scissor, keeps display-list order, and leaves the frame untouched for an unknown id or after the callback is removed.

The CPU renderer has the same in two tests of its own, and `tests/compare_gpu.rs` now holds a three-stop diagonal ramp with a translucent middle stop, a radial ramp fading to nothing, a dropped shadow and an inset one. Comparing both renderers over that scene at three scale factors found a real bug: the CPU gradient read the pixel centre as `index + 0.5/scale` rather than `(index + 0.5)/scale`, which is the same thing at a scale of one and wrong at 1.25. After the fix the mean difference is 0.16 to 0.56 per channel, with at most 0.39% of pixels differing by more than 32.

Three core tests cover the style side: a shadow painted behind the node and an inset one over its fill, with every field carried through; a gradient over the flat background with its points, stops and radius, and one stop drawing nothing; and stylesheet parsing of both, including `none`, `inset`, `radial`, evenly spread stops and the error messages.

The workspace suite is 284 tests plus 31 opt-in GPU tests; formatting and clippy are clean; all 17 examples pass their smoke run. The theming example now shows a gradient card with a drop shadow, a radial glow and an inset well, and a real window renders them.

## Remaining limits

- No paths: shapes are rounded rectangles, and a real path primitive would need a tessellator on the GPU.
- Conic gradients, gradient transforms and repeating ramps do not exist, and the radial one is an ellipse in the rectangle rather than a circle.
- Gradients and shadows are not animated by `Style::transition`, which interpolates colours and transforms alone.
- A shadow is drawn per node; there is no way to cast one from a group.
- A custom draw is a wgpu callback: there is no software equivalent, and a window painted by the CPU renderer leaves its rectangle empty.
