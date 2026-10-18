# Writing a renderer

A renderer takes a `Scene` and produces pixels. Everything above it is done: layout, text shaping, hit testing and paint all happen before the scene exists, and the scene is a flat display list.

## The contract

```rust
pub struct Scene {
    pub size: Size,               // logical
    pub primitives: Vec<Primitive>,
}
```

Primitives are drawn in order. They are:

| Primitive | What it means |
| --- | --- |
| `Quad` | A rounded rectangle in one colour. `radius` of half the shorter side is a circle. |
| `Gradient` | A rounded rectangle filled with a ramp, linear or radial, in the rectangle's own units. At most eight stops. |
| `Shadow` | A blurred rounded rectangle, outside the shape or inset. `blur` is a Gaussian standard deviation in logical pixels. |
| `Text`, `StyledText`, `RichText` | A block of text to shape and draw. The measurer the framework used is the one to shape with, or the geometry will not match the layout. |
| `Image` | A part of an image into a rectangle, with an optional tint and rounded corners. |
| `PushClip` / `PopClip` | A rectangular clip, nested and balanced. |
| `PushLayer` / `PopLayer` | A group drawn to an offscreen image, then composited with an opacity and an affine transform. Renderers may draw a plain one inline. |
| `Custom` | A rectangle the renderer does not draw. See below. |

Coordinates are logical; the renderer multiplies by the scale factor. Nothing in the scene knows about devices.

## Colour

This is the part that takes a day to find. Surfaces are sRGB, and blending happens in **linear light**: half of red over blue is 188, not 128. A renderer that blends in sRGB space comes out visibly wrong wherever anything is translucent.

Images are premultiplied in **sRGB** space, as decoders produce them. Both renderers convert them to the sRGB encoding of their premultiplied *linear* colour before sampling; skipping that makes translucent images dark.

The CPU renderer does it by hand through a twelve-bit gamma table, which round-trips exactly, and uses tiny-skia for geometry only, because tiny-skia mixes premultiplied channels in its own space.

## Damage

`Context::damage_regions` gives the rectangles that changed. A renderer that can, paints only those: the wgpu one keeps a canvas texture, loads it, scissors to the damage and blits; the CPU one seeds its clip stack with the damage union and clears only those pixels. Both fall back to the whole surface when the window is resized or the scene is new.

The way to be sure damage is complete is not to reason about it. `glassmere-render-cpu/tests/damage.rs` paints every frame twice, once incrementally and once in full, and compares byte for byte.

## Two renderers, held to each other

`glassmere-render-cpu/tests/compare_gpu.rs` renders one scene containing every primitive with both renderers at three scale factors and asserts a mean and a percentile bound. It has caught two real defects, including a gradient that sampled `origin + index + 0.5/scale` instead of `origin + (index + 0.5)/scale`, which is identical at a scale of one.

A new renderer should be held to the same scene.

## Text

Shaping belongs to `glassmere-text`, which both renderers share: a `TextSystem` over cosmic-text, with the shaped buffers kept between frames. A renderer that shapes text differently from the measurer the framework used will draw text that does not match its own layout, so use the same `TextMeasurer`.

## An application's own drawing

`Primitive::Custom { rect, id }` is a hole. The wgpu renderer calls whatever `Renderer::set_custom_draw(id, ..)` registered, with the device, the queue, the render pass, the rectangle, the scale and the target size, and the scissor already set. The CPU renderer leaves it empty. This is how an application puts its own 3D or shader work inside a window; `AppOptions::gpu` is where it registers.

## Plugging one in

The desktop host owns the two renderers it has, and chooses between them: `GraphicsBackend::Cpu`, `GLASSMERE_RENDERER=cpu`, or automatically when no adapter works. An application with a renderer of its own drives the loop itself: hold a `Context`, call `run` with a measurer and a view, take `context.scene()`, and present it however it likes. That is what `CpuRenderer` does in the headless tests, and it needs nothing from the windowing crate.
