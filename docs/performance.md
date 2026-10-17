# The performance model

What makes a frame expensive here, what it costs, and how to see it. The numbers are from this machine (Windows 11, Ryzen 24 threads, RTX 3070) and are meant as orders of magnitude, not guarantees.

## The five phases of a frame

A frame runs the view, reconciles what it declared against the retained tree, lays out what moved, paints what changed and hands the scene to a renderer. Each phase can be skipped, and skipping is the whole design.

| Phase | Runs when | What makes it expensive | What to do about it |
| --- | --- | --- | --- |
| Declare | Every frame the host asks for one: input arrived, a message arrived, an animation is running, or the application asked. | The view itself, plus one `Element` per widget it declares. | `Ui::memo` skips a subtree whose state has not changed. Virtualized collections declare only what is visible, whatever the model's size. |
| Reconcile | With the declaration. | Matching keys against the retained children, one map lookup per widget. | Stable keys. A key that changes every frame destroys and recreates the node, and its scroll, focus and animation state with it. |
| Layout | Only when a declaration changed something layout depends on: size, spacing, text that measures differently. | Taffy over the dirty subtree, plus the text measurer. | Do not change sizes for a cosmetic state. A colour change alone never reaches layout. |
| Paint | When anything changed that is drawn: layout, colour, text, focus, hover. | One chunk per node; unchanged subtrees reuse the chunk they had. | The inspector's Repaints overlay shows what repainted. |
| Present | When the scene changed. | Uploading changed buffers and drawing; with damage, only the changed pixels are cleared and drawn. | Nothing usually. Damage is automatic. |

Text shaping happens inside declare, layout and paint rather than beside them, and is timed separately, so a frame that is slow because of text says so.

## What a frame costs

From `cargo run -p seaglass-core --example bench --release`, which runs the four hardest cases headlessly, without a GPU or a font stack:

| Case | Frame | Declared |
| --- | --- | --- |
| A table of 1,000,000 rows, still | 0.53 ms | 353 elements, 2.3 kB |
| The same table, scrolled a page at a time | 1.15 ms | 373 elements, 2.5 kB |
| The same table, jumping to a random row each frame | 1.40 ms | 373 elements, 2.6 kB |
| A tree of 100,000 nodes, all expanded | 0.18 ms | 131 elements, 1.1 kB |
| The same tree, scrolled | 0.36 ms | 139 elements, 1.3 kB |
| The same tree, collapsing and expanding a branch | 6.0 ms | 131 elements, 1.1 kB |
| A 3.2 MB text file, opening it | 47 µs | 2 elements |
| The same file, typing in it | 40 µs | 2 elements |
| 400 elements, all animating | 0.9 ms | 401 elements, 3.1 kB |

Two things to read out of that table. First, size costs nothing that is not on screen: a million rows and a hundred thousand nodes declare a few hundred elements, because only the viewport is realized. Second, the one slow case is the tree's collapse, and that is the application's model rebuilding its visible order over 100,000 nodes, not the framework.

## Startup

`SEAGLASS_STARTUP=1` prints how far into startup each step was reached. A release build of the counter example on this machine:

| Step | Reached at |
| --- | --- |
| Window and graphics instance | 23 ms |
| Adapter chosen | 301 ms |
| Device created | 353 ms |
| Surface configured | 356 ms |
| Renderer, pipelines and fonts | 444 ms |
| First frame on screen | 491 ms |

The framework's own share is small: the same example with `SEAGLASS_RENDERER=cpu`, which skips the graphics stack entirely, shows its first frame at 56 ms. The 278 ms between the instance and the adapter is the driver enumerating adapters, which nothing here can shorten. An application that wants to start faster than its driver allows can paint with the CPU renderer.

## Footprint

Release builds of the counter example, which is a whole application:

| Build | Size |
| --- | --- |
| Default features: desktop, syntax, images, derive, fluent, regex | 15.8 MB |
| `--no-default-features --features desktop` | 12.3 MB |

There is nothing to install beside it and no runtime to ship. On Windows the debug information is in a separate `.pdb`, so `-C strip=symbols` changes nothing: both numbers above are what is deployed. The 3.5 MB difference is syntax highlighting, image decoding, translation catalogs and regular expressions, each of which is a feature an application turns off if it does not use it.

## Allocation

A declaration is thrown away every frame, so what it allocates is what a frame costs in allocation. `FrameStats::elements` and `element_bytes` count it exactly: one element per widget declared, and the bytes of the keys and text they carry. The vectors those elements live in are pooled and reused between frames.

Everything else is retained: nodes, layout, paint chunks, GPU buffers, decoded images and shaped text all survive frames and are reused by identity. There are no widget trait objects anywhere; a widget is a variant of an enum, and an application's own widget is a leaf of primitives.

## Seeing it on a running window

F12 opens the inspector. Its tabs answer different questions:

- **Frames** is the profiler: declare, layout, paint, shaping, render and GPU time per frame, with what was created, reused and repainted. `Export CSV` writes the ring buffer, with the element and byte counts in it.
- **Scene** is what the window holds: nodes, text, scene primitives, paint caches and decoded images, and the layers of the last frame with their opacities and transforms. Layers cost a texture and a pass each.
- **GPU** is the renderer: pass times from the GPU's own clock where the adapter has timestamp queries, draw calls, and memory by category beside the driver's own report.
- **State** is the reactive graph, when the window observes a store: what each derived value read and which one moved last.
- **Damage** and **Repaints** draw the changed regions and the repainted nodes over the window itself.

`tracing` spans cover the same phases for an application that already collects them.
