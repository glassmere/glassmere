# Milestone 28: the guides, and the promise

Implemented on 2026-09-22. The last two coverage rows were the documentation the feature list asks for, and the row that sums the rest up.

## The guides

Four were missing. Each is written for somebody with a question rather than as a tour, and every claim in them points at something that exists:

| Guide | What it answers |
| --- | --- |
| [Widgets](widgets.md) | The three ways to add something: a function, a `Component`, or a leaf of your own through `Ui::widget`. What input, focus, semantics, style and painting a widget gets, how to test it headlessly, and what a leaf cannot do. |
| [Renderers](renderers.md) | The scene as a flat display list, what each primitive means, the colour trap that costs a day, how damage works and how it is proved, how the two renderers are held to each other, and how to plug in a third. |
| [State](state.md) | Ordinary Rust state as the model, what the framework keeps and why identity decides it, how answers come back through outboxes, the thread boundary, the optional store, and declaring less. |
| [Accessibility](accessibility.md) | What a node says, how names are given, the keyboard, typing and bidirectional text, and how to test the tree without a screen reader. |

They join [the architecture guide](architecture.md), [the performance guide](performance.md) from milestone 24 and [the stability note](api-stability.md) from milestone 27, and the README now leads with a table of all seven.

## The promise

The last row of the coverage table is the promise the whole list was written around. Where it stands:

| The promise | Where it is |
| --- | --- |
| As easy to start with as egui | `run(options, move |ui| ...)` and a closure that owns its state. The counter example is twenty lines, with no build step, no markup and no macros. |
| As capable of complex applications as GPUI | Docking with detachable windows, a data grid over a million rows, a code editor over a rope with syntax highlighting and multiple cursors, a rich text editor, several windows on one device, background jobs, and the application's own wgpu drawing inside a window. |
| As incremental as Floem or Xilem | A retained tree behind the declarations: property comparison limits layout and paint, `Ui::memo` skips whole subtrees, and presentation redraws only the pixels that changed, proved frame by frame against a full repaint. A million-row table costs half a millisecond a frame. |
| As tooling-focused as Slint | An inspector with the tree, the box model, computed style, the accessibility view, an event log, frame and GPU timings, the scene's layers and memory, a reactive state graph and damage overlays; a profiler with CSV export and `tracing` spans; framework diagnostics that name the node; a headless test harness with semantic finders and snapshots; four benchmarks; and a runner that rebuilds and restarts while keeping the state and the window. |
| As styleable as web UI | Theme tokens that follow the desktop, a stylesheet with classes, per-state overrides and hot reload, checked while compiling, inherited text properties, gradients, shadows, background images with nine-slice frames, transitions, enter and exit effects, layout animations and keyframes. |
| Accessibility and text as core architecture | Semantics on every node, published through AccessKit from the same tree the window draws, with a diagnostic for a control that has no name. Text is shaped by cosmic-text through one system shared by both renderers, with bidirectional carets, IME, rich runs and font fallback, and the layout measures what the renderer draws. |

## Validation

Nothing new to run: the guides are prose, and what they describe is covered by the tests of the milestones that built it. The workspace suite is 320 tests, formatting and clippy with all features are clean, and all 19 default examples pass their smoke run.

With this, every row of [the coverage table](feature-coverage.md) reads Done: 62 of 62.

## Remaining limits

- The guides are written against this machine's experience. The Linux and macOS notes come from the code and the build, not from use.
- There is no API reference site: the signatures are in the sources and in [the surface snapshot](../crates/seaglass/tests/snapshots/public-api.snap), and `cargo doc` renders the rest.
- Being finished against a list is not the same as being finished. Each row names what it does not do, and those notes are the honest map of what is left.
