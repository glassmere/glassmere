# Feature coverage

Status of the 62 goals in [the original feature list](feature-list.md) as of 2026-09-21, after milestone 11b. "Done" means the contract exists, is tested and is documented in a milestone report; "Partial" names what exists and what is missing; "Planned" points at the roadmap phase.

| # | Goal | Status | Notes |
| ---: | --- | --- | --- |
| 1 | Immediate-mode ergonomics, retained internals | Done | `ui.button(...)` over a retained tree with persistent focus, scroll, editing, caches and animation state. |
| 2 | Stable widget identity | Done | Explicit/positional keys, monotonic `NodeId` shared by layout, input, focus, accessibility, testing and the inspector. |
| 3 | Incremental updates | Partial | Property comparison limits layout/paint work; declarations still visit the whole view each frame. |
| 4 | Incremental layout | Done | Taffy dirty subtrees; same-size text changes skip layout. |
| 5 | Incremental rendering | Partial | Damage tracking, subtree paint caches, retained GPU buffers and text batching exist; presentation still repaints the full surface. |
| 6 | First-class virtualization | Partial | Lists, trees and grids realize only the viewport (milestone 4). Huge documents/editors: planned (phase 6). |
| 7 | Excellent text system | Partial | Unicode, graphemes, fallback, ligatures, emoji, IME, selection, clipboard, undo/redo, rich text blocks with links, syntect highlighting, a rope document with virtualized line rendering, a WYSIWYG rich text editor with Markdown persistence. Visual or logical caret order in bidirectional text with caret affinity, right-to-left alignment. Missing: forced paragraph base direction, regular expression search. |
| 8 | Accessibility from day one | Done | AccessKit roles, labels, actions, keyboard navigation, tree visible in the inspector's Accessibility tab. Screen-reader speech review remains manual. |
| 9 | Powerful focus system | Done | Keyboard focus, pointer capture, Tab traversal, focus scopes with trap/restore/memory, modal focus in dialogs, geometric arrow navigation, predictable restoration (milestone 6c). |
| 10 | Modern layout engine | Partial | Flexbox, grid with tracks/areas/repeat/minmax, intrinsic sizing, percentages, min/max, aspect ratio, alignment including text-bottom baseline, absolute popups (milestone 6b). Missing: true font baselines, responsive breakpoints, custom layout primitives. |
| 11 | Typed styling system | Done | Typed `Style` with builders, Rust classes, stylesheet classes with `extends`, tokens, hover/pressed/focused/disabled overrides, text inheritance and hot reload from disk (milestone 6a), background images with fit modes and nine-slice from code or sheets (milestone 8b). Compile-time validation applies to the Rust API only. |
| 12 | Excellent theming | Done | Dark and light token themes, runtime switching, OS preference via winit, stylesheet token overlays and user sheets (milestone 6a). |
| 13 | Animations in the property system | Done | `Style::transition`, `enter`/`exit` effects, `layout_transition`, keyframe `Animation`, easing curves, reduced motion; composited as GPU layers. Missing: spring physics, per-property timing, OS reduced-motion detection. |
| 14 | Renderer-independent UI core | Done | Core emits a `Scene` display list; no wgpu types in core. |
| 15 | Multiple rendering backends | Partial | wgpu only. CPU renderer: planned (phase 6). |
| 16 | Efficient GPU architecture | Partial | Instanced quads, glyph atlas, a shared image atlas with vector images rasterized per size (milestone 8a), offscreen layers, batching, rectangular clipping. Missing: shadows, paths, gradients, custom shaders, mipmaps. |
| 17 | Custom rendering escape hatch | Partial | `CustomWidget` leaves paint primitives; no embedded 2D/3D or direct GPU hook yet. |
| 18 | Rich widget library | Done | Button, checkbox, radio, slider, text input, combo, menu, context menu, tooltip, popover, tabs, split panes, tree, virtual list, data grid, modal and non-modal dialogs, docking, rich text, code editor, rich text editor, images, number input and autocomplete (milestone 9a), toasts (milestone 9b) and a property grid with a derive macro (milestones 9c and 9d). |
| 19 | Professional-grade DataGrid | Done | Virtual rows, sorting, selection, huge datasets; column resizing, reordering and hiding in app-owned state, by drag and from a keyboard menu (milestone 10b); a cell cursor and typed in-place editing with model validation (milestone 10c); a filter row whose texts the model applies (milestone 10d); sideways scrolling with frozen and virtual columns (milestone 10e). Missing: pinned data rows, range selection with copy and paste. |
| 20 | Docking/layout system | Done | `DockLayout` with splits, tab groups, in-window floating groups and detached OS windows; drag onto drop indicators plus a keyboard panel menu; text format and `serde` feature. Missing: cross-window tab drags, persisted detached-window geometry, scrolling tab strips. |
| 21 | Powerful event system | Partial | Pointer (left and right buttons), keyboard, drag/drop, capture, context-menu requests, timed wakes. Missing: bubbling/cancellation model, gestures, custom events. |
| 22 | Input abstraction independent of the OS | Done | Core consumes `Event`; winit lives only in the host crate. |
| 23 | Mouse/touch/stylus support | Partial | Mouse only. Multi-pointer, gestures, pressure, smooth scrolling: not started. |
| 24 | First-class keyboard shortcuts | Partial | Commands with platform-primary shortcuts. Missing: scopes, remapping, command palette. |
| 25 | Command architecture | Partial | `Command` shares enabled state between button and shortcut; menus/palette do not consume commands yet. |
| 26 | Clipboard and drag-and-drop | Partial | Text clipboard, in-process item/text drags, incoming OS file drops. Missing: images, custom types, outgoing OS drags, cross-window drags. |
| 27 | Multi-window support | Partial | Independent windows on one device, native modal dialog windows that block their owner, shared state via `Arc`/mailboxes, DPI scaling. Missing: multi-monitor policy. |
| 28 | Strong async integration | Partial | Bounded mailbox and wake handle; no executor is forced. Missing: cancellation, streams, async commands helpers. |
| 29 | Optional reactive state | Not started | Ordinary Rust state is the only model. |
| 30 | Ordinary Rust state | Done | Views borrow `&mut` application state. |
| 31 | Observable dependency graph | Partial | Inspector shows per-node "last declaration changed" flags and repainted nodes; no dependency graph because there is no reactive layer. |
| 32 | Built-in native DevTools | Partial | F12 inspector: tree, box model, computed style, focus/hover state, accessibility view, event log, repaint/damage overlays, frame timings. Missing: render layers, GPU timings, memory statistics. |
| 33 | Performance profiler | Partial | Declare/layout/paint/render CPU timings, dirty flags, created/reused/removed counts, paint cache hits, CSV export, `tracing` spans. Missing: text shaping split, GPU time, draw calls, allocations. |
| 34 | Hot reload | Done | Stylesheets and themes reload in place; any file through `Ui::watched_bytes`/`watched_text`; Rust code through `rust-gui-dev`, which rebuilds and restarts with app state (`Ui::hot_state`) and window geometry kept (milestone 7g). Image files reload in place too (milestone 8a). Missing: in-process code swapping. |
| 35 | Headless UI testing | Done | `rust-gui-testing` harness with real input paths and application state access. |
| 36 | Snapshot testing | Done | Text snapshots for tree, layout, accessibility and scene; opt-in pixel snapshots with a bundled font. |
| 37 | Semantic testing API | Done | `by_label("Save")` then `click`; key paths for disambiguation. |
| 38 | Deterministic rendering/testing | Done | Synthetic measurer for logic tests; bundled Fira Mono for pixel references. Cross-driver tolerance still to be confirmed in CI. |
| 39 | Localization | Partial | Right-to-left windows and subtrees with mirrored layout and widgets, direction detected from the OS locale (milestone 7f). Fluent message catalogs with plurals, a fallback chain and run-time language switching; the framework's own strings translatable, with Czech and German shipped (milestones 11a and 11b). Missing: locale-aware numbers and dates. |
| 40 | High-DPI support | Done | Per-window scale factor, fractional scaling, DPI-aware accessibility bounds. |
| 41 | Platform integration | Partial | Accessibility APIs, IME, clipboard, window titles. Missing: file dialogs, notifications, native menus, tray, file associations, system appearance. |
| 42 | Native-first desktop focus | Partial | Windows verified natively; Linux/macOS build in CI without interaction checks. |
| 43 | Clean platform abstraction | Done | Host crate isolates winit/wgpu/AccessKit. |
| 44 | Low allocation architecture | Partial | Retained nodes, reused GPU buffers and paint chunks; declarations still allocate strings/elements per frame. |
| 45 | Predictable ownership | Done | Views are plain closures over `&mut` state; no `Rc<RefCell>` in application code. |
| 46 | Good escape hatches | Partial | Custom leaves and models; arbitrary input contracts for custom widgets are missing. |
| 47 | Modular crates | Done | core, scene, render-wgpu, winit, testing, devtools, facade. Layout/text/style/widgets live inside core and scene for now. |
| 48 | No forced proprietary DSL | Done | Plain Rust. |
| 49 | Compile-time checked UI APIs | Partial | Typed styles and models; invalid combinations panic at declaration rather than failing at compile time. |
| 50 | Excellent documentation | Partial | Architecture and milestone reports. Missing: widget-building, custom renderer, state, accessibility and performance guides. |
| 51 | Stable public API | Not started | Prototype API; breaking changes expected before 1.0. |
| 52 | Strong benchmarks | Partial | 10,000-node declaration benchmark, one-million-row list test, 6,000-frame stress. Missing: tree/text/animation benchmarks. |
| 53 | Low idle CPU/GPU usage | Done | Event-driven loop sleeps between events; no continuous redraw. |
| 54 | Fast startup | Partial | Sequential backend probing and compact allocation; startup timing not benchmarked. |
| 55 | Small deployment footprint | Partial | Native binary; footprint not measured. |
| 56 | Excellent error diagnostics | Partial | Duplicate keys, duplicate shortcuts, invalid models and clip imbalance panic with messages; stylesheet, dock layout and watched-file errors carry line numbers or paths, unknown classes surface in the inspector, and controls without an accessible name are reported (milestone 7h). Missing: font/resource diagnostics. |
| 57 | Framework-level diagnostics | Done | Unstable ids, needless relayout, excessive repainting, controls without an accessible name, oversized layers and huge unvirtualized containers, through `Context::diagnostics`, the inspector and `tracing` (milestone 7h). |
| 58 | Small core with extension points | Partial | Models and custom leaves are public; third-party controls need a richer widget contract. |
| 59 | Component ecosystem | Not started | |
| 60 | Excellent custom-widget API | Partial | Size, paint and semantics; missing input, state and style contracts. |
| 61 | Clear performance model | Partial | The architecture doc's invalidation table plus inspector change flags. A written performance guide is missing. |
| 62 | Central promise | Partial | Retained incremental engine behind an immediate-style API is in place; capability, styling and tooling breadth continue in phases 6 and later. |
