# Milestone 32: a box the text sits in the middle of, and a rectangle that is cleared

Implemented on 2026-09-22. Two things the last milestone left behind: caption buttons that were square where the system's are wider, and a menu that left its pixels on the window after it closed.

## Text in a box

A text leaf was drawn from the corner of its content box, so a box larger than its text put the text in that corner. `Ui::window_buttons` had to make each button hug its glyph to centre it, which is why they came out square.

The properties a container places its children with now place a leaf's own text: `justify_content` along the line and `align_items` across it, with `Start` as before. Right-to-left text without an opinion still goes to the end, exactly as it did. A wrapped block is still aligned line by line by the shaper; this places the block.

So the caption buttons are the shell's own box, 46 by 32 at a scale factor of one, with the glyph in the middle of it.

## A damaged rectangle is cleared

Closing the menu in the title bar left the menu on the screen, with the page's text redrawn over it. The damage the framework reported was right, and the CPU renderer painted it correctly; the GPU renderer did not.

Its damaged frames begin with `LoadOp::Load`, keeping the last frame outside the damage, and scissor every draw to the damaged rectangle. Nothing cleared the rectangle itself, so whatever it held stayed underneath. Where the new frame drew over all of it, nothing showed; where the new frame drew *nothing*, which is what a window's plain background is, the old pixels stayed. A popup over a bare background is exactly that case.

The clear colour now goes down first, over the damaged rectangle, with a pipeline that replaces rather than blends, so a translucent clear colour lands as a clear and not as a wash.

## Validation

- `crates/rust-gui-render-wgpu/tests/gpu.rs` gained a test that establishes a canvas, removes a popup and names only its rectangle as damage, and checks both what replaces it and what is left when nothing does. Removing the clear makes it fail on the second: `pixel (40, 30) is [0, 0, 255], expected [255, 255, 255]`.
- `crates/rust-gui-core/tests/layout.rs` gained one for the text placement: the same box with `Start`, `Center` and `End`, where the end moves the text by twice what centring does, and padding comes first either way.
- `crates/rust-gui-render-cpu/tests/damage.rs` gained the menu sequence itself, opened and closed by its own button, declared twice per frame as a host does.
- Photographed: `target/caption-*.png` for the buttons at rest, hovered and held, and `target/menu-closed-again.png` for the window after the menu closes, which is now clean.

The workspace suite is 328 tests plus 31 opt-in GPU tests, all passing; formatting and clippy with all features are clean; all 20 default examples pass their smoke run, and `scripts/test-titlebar.ps1` still drives a real window end to end.

## Remaining limits

- The placement uses the measured size of the text, which a control that is not a text leaf does not keep, so a slider's label still sits where it did.
- Clearing the damaged rectangle costs one full-surface quad per damaged frame, scissored, which is cheaper than the redraw it makes correct.
