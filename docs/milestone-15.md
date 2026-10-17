# Milestone 15: declarations and presentation become incremental

Implemented on 2026-09-22. Two promises in the coverage table were only half kept: a declaration visited the whole view every frame, and presentation repainted the whole surface however little had changed. Both now do the work the frame needs.

## Declarations: `Ui::memo`

```rust
ui.memo("list", &rows, Style::default(), |ui| {
    for row in &rows { ui.label_keyed(&row.id, &row.text); }
});
```

| Piece | Contract |
| --- | --- |
| `Ui::memo(key, state, style, content) -> bool` | A column whose content is declared again only when the hash of `state` differs from the frame before. Returns whether the content ran. |
| What is kept | The retained subtree: it still paints, scrolls, animates, holds what was typed and takes part in layout, so a skipped frame looks exactly like a declared one. Nodes keep their identity. |
| Input | A frame carrying input for anything inside the subtree always declares it, however unchanged the state is, so a button in a memoized panel is never deaf. The ancestors of everything with input waiting are gathered once per frame, so asking is a lookup. |
| Nesting | Memos nest. A skipped outer one is not descended into at all, so the inner ones cost nothing. |
| `FrameStats::skipped` | Subtrees kept without declaring, for the inspector and the CSV. |
| The catch | `state` must cover everything the content reads, and a `Response` from inside is not delivered on a skipped frame. It is for a large subtree over stable data, not for a handful of widgets. |

## Presentation: damage

The core has always computed conservative damage rectangles; both renderers now use them.

| Piece | Contract |
| --- | --- |
| `Renderer::render_cached(.., damage: &[Rect])` | wgpu. Empty means everything, and is exactly the old path: a host that never sends damage allocates nothing new. With damage, frames are drawn into a persistent canvas texture with `LoadOp::Load` and the union of the damage as a scissor rectangle, and the canvas is copied to the swapchain by the existing layer pipeline. A first frame, a change of size or scale, and a canvas that cannot be allocated each force a full redraw from a flag rather than a guess. |
| `CpuRenderer::render_damaged(.., damage)` | The same contract. Only the damaged pixels are cleared and painted; the clip stack starts at the damage, so every draw is already bounded by it and nothing else was needed. |
| `RenderStats::redrawn`, `CpuStats::redrawn` | Physical pixels the last frame redrew. |
| `MemoryStats::canvas` | What the canvas texture costs, in the inspector's GPU tab. |
| Host | Sends `Context::damage_regions()` only for a frame it declared, for this window, and with no inspector overlay change, because those are the cases where the previous picture in the renderer is this window's own. |

## Validation

Four tests in `seaglass-core/tests/memo.rs`: a subtree kept without declaring, painting the same scene and keeping its ids; a skipped subtree that still moves when the heading above it grows; a click reaching a button in a subtree that was not declared, and the frames around it; and nested memos with a text field inside, where typing reaches the field, the inner subtree is kept when the outer one declares, and the outer one hides the inner one when it is skipped.

`seaglass-render-cpu/tests/damage.rs` is the proof that the damage is complete: a window with a field, a checkbox, a slider, a combo box and a scrolling list is driven through hovering, pressing, toggling, focusing, typing, dragging the slider, opening and choosing in the combo box, scrolling, adding a row and losing focus. Every frame is painted twice, once from the damage into a renderer that keeps its frames and once in full into one that does not, and the two must agree byte for byte. They do, while hovering redraws under a quarter of the window and typing under a third.

The wgpu side has two GPU tests of its own: damage keeping the previous frame outside its rectangles while matching a full redraw inside them, empty damage redrawing everything, two rectangles redrawing their union, and a resize forcing a full redraw however small the damage.

The workspace suite is 268 tests plus 26 opt-in GPU tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features; all 17 examples pass their smoke run; `scripts/test-theming.ps1` and `scripts/test-controls.ps1` pass, and a real window renders the same with damage as without on both renderers.

## Remaining limits

- The damage is the union of the rectangles, not the rectangles themselves, so two changes far apart redraw the box between them.
- A memo hashes with `DefaultHasher`, so state that is expensive to hash is hashed every frame; hash a version number instead where that matters.
- A frame that changed nothing at all still presents in full, because an empty damage list means everything.
- The canvas texture is kept for the renderer's life once allocated.
