# Embedded

Glassmere runs on a microcontroller with a display and no operating system. The application is the same `App` with the same widgets; what changes is what is around it:

| On the desk | On a device |
| --- | --- |
| `glassmere` (winit, wgpu, cosmic-text) | `glassmere-core` without its `std` feature, and `glassmere-embedded` |
| A window and its event loop | The firmware's own loop and `EmbeddedUi` |
| System fonts, shaped text | Bitmap fonts built into the binary, on a character grid |
| A swapchain | A pixel buffer you send to the panel: all of it, a band at a time, or the rectangles that changed |

```toml
[dependencies]
glassmere-core = { version = "0.1.7", default-features = false }
glassmere-embedded = "0.1.7"
```

Both are `#![no_std]` with `alloc`, so the firmware needs a global allocator. They are checked in CI for `thumbv7em-none-eabihf`. The target must have atomics with compare-and-swap (`Arc`), which leaves out `thumbv6m`.

## The loop

```rust
let mut ui = EmbeddedUi::new(Thermostat::default(), 320, 240);
let mut band = [Rgb565::default(); 320 * 24];
loop {
    // Input: a touch panel, keys, an encoder.
    match touch.read() {
        Some((x, y)) if !pressed => ui.press(x, y),
        Some((x, y)) => ui.drag(x, y),
        None if pressed => ui.release(),
        None => {}
    }
    // Anything else the app should know is a message.
    if let Some(reading) = sensor.poll() {
        ui.send(Message::Measured(reading));
    }

    if ui.update(timer.now()) {
        for damage in ui.damage().to_vec() {
            // Paint `damage` a band at a time and send each to the panel.
            let mut frame = Framebuffer::new(&mut band, 320, 24).at(0, y);
            ui.render(&mut frame);
        }
        ui.presented();
    }
    if !ui.animating() {
        sleep_until(ui.next_wake());
    }
}
```

`update` takes the time since start as a `Duration`, from a hardware timer or a tick count; animations, transitions and tooltips run on it. It runs `App::update` on what the input produced and declares the view only when something changed. `damage` is the rectangles that changed since `presented`, at most eight, and the whole screen at first.

[`examples/embedded-firmware`](../examples/embedded-firmware) is a whole firmware crate with this loop behind a small `Board` trait, and `cargo run --example embedded` shows the same application on the desk, drawn by the embedded renderer into an RGB565 buffer.

## Memory

- **Frame buffer:** none is needed. A `Framebuffer` is any slice of pixels with a place on the screen (`at(x, y)`), and rendering into it paints what the scene has there, identical to that part of a full frame. 320 x 24 RGB565 pixels are 15 KiB; a whole 320 x 240 frame would be 150 KiB.
- **Pixel formats:** `Rgb565`, `Rgb888`, `Xrgb8888`, `Gray8` and `Mono`, or your own through the `Pixel` trait.
- **Fonts:** `FontSet::BUILTIN` is 6x13 and 9x18, regular and bold, about 35 KiB of flash, covering Latin with diacritics (U+0020 to U+017F), common punctuation, arrows and the symbols the widgets draw. `BitmapFont::new` takes your own, and `examples/bdf_convert.rs` in the crate converts BDF files.
- **Heap:** the tree, the layout and the scene live on the heap, in proportion to the number of widgets declared. The thermostat example runs in a 96 KiB heap that never frees.

## What is different

- Text is on a character grid, left to right, in the face whose cell is nearest the style's font size. The font family and italic are ignored; a weight of 600 or more is bold. `EmbeddedMeasurer` and the renderer share their layout code, so text fits where it was measured.
- Colours blend as sRGB numbers, not in linear light, so anti-aliased edges are slightly darker than on the desktop renderers. Opaque areas are identical.
- A layer applies its opacity and its move; scale, rotation and skew are ignored. Vector images and `Primitive::Custom` are skipped. `RenderOptions::shadows` turns shadows off, which is the cheapest speed-up on a slow chip.
- Without `std`, `glassmere-core` has no threads, files or channels: `Effects::spawn`, `choose_files`, watched files, `Ui::image_file` and `MessageSender` are absent, and so are `text_area` and `code_editor`, whose text storage needs `std`. A stylesheet is parsed from a string (`include_str!`) and may not use `background-image`. Messages from interrupts or other tasks reach the app through `EmbeddedUi::send` from the loop.
- There is no accessibility bridge and no clipboard on a device.

Performance on real hardware has not been measured: the renderer is tested on a host for correctness against the CPU renderer, and built, not run, for a Cortex-M target.
