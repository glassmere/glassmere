# Feature coverage

Status of the 62 goals in [the original feature list](feature-list.md) as of 2026-09-21, after milestone 6a. "Done" means the contract exists, is tested and is documented in a milestone report; "Partial" names what exists and what is missing; "Planned" points at the roadmap phase.

| # | Goal | Status | Notes |
| ---: | --- | --- | --- |
| 1 | Immediate-mode ergonomics, retained internals | Done | `ui.button(...)` over a retained tree with persistent focus, scroll, editing and caches. Animations: planned (phase 6). |
| 2 | Stable widget identity | Done | Explicit/positional keys, monotonic `NodeId` shared by layout, input, focus, accessibility, testing and the inspector. |
| 3 | Incremental updates | Partial | Property comparison limits layout/paint work; declarations still visit the whole view each frame. |
| 4 | Incremental layout | Done | Taffy dirty subtrees; same-size text changes skip layout. |
| 5 | Incremental rendering | Partial | Damage tracking, subtree paint caches, retained GPU buffers and text batching exist; presentation still repaints the full surface. |
| 6 | First-class virtualization | Partial | Lists, trees and grids realize only the viewport (milestone 4). Huge documents/editors: planned (phase 6). |
| 7 | Excellent text system | Partial | Unicode, graphemes, fallback, ligatures, emoji, IME, selection, clipboard, undo/redo. Missing: BiDi/RTL caret order, rich text, syntax highlighting, large-document virtualization. |
| 8 | Accessibility from day one | Done | AccessKit roles, labels, actions, keyboard navigation, tree visible in the inspector's Accessibility tab. Screen-reader speech review remains manual. |
| 9 | Powerful focus system | Partial | Keyboard focus, pointer capture, Tab traversal, restoration. Missing: focus scopes, modal focus, directional navigation. |
| 10 | Modern layout engine | Partial | Flexbox, intrinsic sizing, percentages, absolute popups. Missing: grid, min/max, aspect ratio, baseline, custom layout primitives. |
| 11 | Typed styling system | Done | Typed `Style` with builders, Rust classes, stylesheet classes with `extends`, tokens, hover/pressed/focused/disabled overrides, text inheritance and hot reload from disk (milestone 6a). Compile-time validation applies to the Rust API only. |
| 12 | Excellent theming | Done | Dark and light token themes, runtime switching, OS preference via winit, stylesheet token overlays and user sheets (milestone 6a). |
| 13 | Animations in the property system | Planned | Phase 6. |
| 14 | Renderer-independent UI core | Done | Core emits a `Scene` display list; no wgpu types in core. |
| 15 | Multiple rendering backends | Partial | wgpu only. CPU renderer: planned (phase 6). |
| 16 | Efficient GPU architecture | Partial | Instanced quads, glyph atlas, batching, rectangular clipping. Missing: layers, shadows, paths, gradients, custom shaders. |
| 17 | Custom rendering escape hatch | Partial | `CustomWidget` leaves paint primitives; no embedded 2D/3D or direct GPU hook yet. |
| 18 | Rich widget library | Partial | Button, checkbox, radio, slider, text input, combo, menu, tree, virtual list, data grid. Missing: number input, autocomplete, context menus, popovers, tooltips, dialogs, toasts, tabs, split panes, docking, property grid, rich text, code editor. |
| 19 | Professional-grade DataGrid | Partial | Virtual rows, sorting, selection, huge datasets. Missing: virtual columns, filtering, resizing, reordering, frozen rows/columns, inline editing. |
| 20 | Docking/layout system | Planned | Phase 6. |
| 21 | Powerful event system | Partial | Pointer, keyboard, drag/drop, capture. Missing: bubbling/cancellation model, gestures, custom events. |
| 22 | Input abstraction independent of the OS | Done | Core consumes `Event`; winit lives only in the host crate. |
| 23 | Mouse/touch/stylus support | Partial | Mouse only. Multi-pointer, gestures, pressure, smooth scrolling: not started. |
| 24 | First-class keyboard shortcuts | Partial | Commands with platform-primary shortcuts. Missing: scopes, remapping, command palette. |
| 25 | Command architecture | Partial | `Command` shares enabled state between button and shortcut; menus/palette do not consume commands yet. |
| 26 | Clipboard and drag-and-drop | Partial | Text clipboard, in-process item/text drags, incoming OS file drops. Missing: images, custom types, outgoing OS drags, cross-window drags. |
| 27 | Multi-window support | Partial | Independent windows on one device, shared state via `Arc`/mailboxes, DPI scaling. Missing: dialogs, multi-monitor policy. |
| 28 | Strong async integration | Partial | Bounded mailbox and wake handle; no executor is forced. Missing: cancellation, streams, async commands helpers. |
| 29 | Optional reactive state | Not started | Ordinary Rust state is the only model. |
| 30 | Ordinary Rust state | Done | Views borrow `&mut` application state. |
| 31 | Observable dependency graph | Partial | Inspector shows per-node "last declaration changed" flags and repainted nodes; no dependency graph because there is no reactive layer. |
| 32 | Built-in native DevTools | Partial | F12 inspector: tree, box model, computed style, focus/hover state, accessibility view, event log, repaint/damage overlays, frame timings. Missing: render layers, GPU timings, memory statistics. |
| 33 | Performance profiler | Partial | Declare/layout/paint/render CPU timings, dirty flags, created/reused/removed counts, paint cache hits, CSV export, `tracing` spans. Missing: text shaping split, GPU time, draw calls, allocations. |
| 34 | Hot reload | Partial | Stylesheets (classes, tokens, base theme) reload from disk without losing state. UI declarations and assets: planned (phase 7). |
| 35 | Headless UI testing | Done | `rust-gui-testing` harness with real input paths and application state access. |
| 36 | Snapshot testing | Done | Text snapshots for tree, layout, accessibility and scene; opt-in pixel snapshots with a bundled font. |
| 37 | Semantic testing API | Done | `by_label("Save")` then `click`; key paths for disambiguation. |
| 38 | Deterministic rendering/testing | Done | Synthetic measurer for logic tests; bundled Fira Mono for pixel references. Cross-driver tolerance still to be confirmed in CI. |
| 39 | Localization | Not started | |
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
| 56 | Excellent error diagnostics | Partial | Duplicate keys, duplicate shortcuts, invalid models and clip imbalance panic with messages; stylesheet errors carry line numbers and unknown classes surface in the inspector. Missing: font/resource and accessibility diagnostics. |
| 57 | Framework-level diagnostics | Not started | Warnings for relayout, repaint, inaccessible controls and huge unvirtualized collections. |
| 58 | Small core with extension points | Partial | Models and custom leaves are public; third-party controls need a richer widget contract. |
| 59 | Component ecosystem | Not started | |
| 60 | Excellent custom-widget API | Partial | Size, paint and semantics; missing input, state and style contracts. |
| 61 | Clear performance model | Partial | The architecture doc's invalidation table plus inspector change flags. A written performance guide is missing. |
| 62 | Central promise | Partial | Retained incremental engine behind an immediate-style API is in place; capability, styling and tooling breadth continue in phases 6 and later. |
