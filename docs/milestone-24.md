# Milestone 24: measuring it

Implemented on 2026-09-22. Six coverage rows were waiting on numbers rather than on features: the profiler was missing text shaping and allocation, the inspector was missing the scene's layers and what the window holds, the benchmarks covered one of the four cases the feature list names, and startup and footprint had never been measured at all.

## What was added

| Piece | Contract |
| --- | --- |
| `FrameStats::shaping` | CPU time the text measurer spent. The application's measurer is wrapped rather than its call sites, so every path through the framework is counted, including the ones layout takes on its own. It is part of declare, layout and paint, not extra to them. |
| `FrameStats::elements`, `element_bytes` | What the declaration allocated: one element per widget declared, and the bytes of key and text they carry. Exact, and free of an allocator's own behaviour. |
| Element pooling | The vectors elements live in are kept and refilled instead of being allocated every frame. |
| `Context::memory` | What the window holds on the CPU: nodes and their flat size, text and keys, scene primitives, paint caches and decoded images. It walks the tree, so it is for an open inspector rather than for every frame. It says what it cannot see: the layout engine's arena, the allocator's overhead, and the glyph atlas, which lives on the GPU behind glyphon. |
| Inspector, Scene tab | Those numbers, and the layers of the last frame with their bounds, opacity, transform and primitive count. A layer costs a texture and a pass, so a view that makes many of them is worth seeing. |
| Profiler columns | Shaping joins declare, layout, paint, render and GPU in the Frames tab and the summary, and the CSV export gained shaping, elements and element bytes. |
| `RUST_GUI_STARTUP` | Prints how far into startup each step was reached, and how long the first window took to show something. |
| `bench` example | The four cases the feature list names, headless. |

The overlay checkboxes moved to a row of their own: with seven tabs there was no width left beside the strip, and they were being pushed off the edge.

## What the numbers say

The full tables are in [the performance guide](performance.md), which is the other half of this milestone. In short:

- A table of a million rows declares 353 elements and 2.3 kB and takes 0.53 ms a frame; jumping to a random row costs 1.4 ms. A tree of a hundred thousand nodes is 0.18 ms. A 3.2 MB text file opens in 47 µs and takes 40 µs a frame while typing. Four hundred animating elements cost 0.9 ms.
- The one slow case is collapsing a branch of the hundred-thousand-node tree, at 6 ms, and that is the example's own model rebuilding its visible order, not the framework.
- Startup to the first frame is 491 ms, of which the framework's own share is 56 ms: the same window painted by the CPU renderer, which skips the graphics stack, is up in 56 ms. Between the graphics instance and the adapter there are 278 ms of the driver enumerating adapters.
- A whole application is 15.6 MB with every default feature and 12.2 MB with none of them. On Windows the debug information is in a separate file, so there is nothing to strip.
- Element pooling was worth a few percent on declaration-heavy frames, which is about the size of the run-to-run noise here. It was kept because it makes the per-frame allocation bounded rather than merely small.

## Validation

The Scene tab has a test that declares a window with an opacity in it, checks that the memory report counts the nodes, the scene and the text, that the one layer is found with its opacity, and that the tab draws both. The real inspector window was driven through UI Automation over the animations example and its rows read back through the accessibility tree:

```
ROW 26 nodes  ·  34.9 kB flat  ·  0.5 kB of text and keys
ROW 26 paint caches holding 336 primitives
ROW 44x44 at 40,228  ·  3 primitives  ·  opacity 1.00  ·  transformed
```

The screenshot is `target/inspector-scene.png`. The workspace suite is 306 tests, formatting and clippy with all features are clean, and every example still passes its smoke run.

## Remaining limits

- Allocation is counted as the framework's own declarations, not as calls to the allocator. Counting those needs a global allocator, and this workspace forbids unsafe code.
- The glyph atlas is not in any memory total: glyphon does not report its size, and it is the one piece of GPU memory the framework cannot account for.
- Startup is measured from the host being asked to run. Loading the executable and the dynamic libraries before that is not in the number.
- The window waits for the graphics stack before it shows anything. Painting the first frame with the CPU renderer while the GPU initializes would hide most of the 491 ms, and is not done.
- The benchmark is one machine and one run. There is no regression gate on these numbers.
