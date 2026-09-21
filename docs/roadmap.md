# Development roadmap

The shared discussion describes a multi-phase desktop UI platform. This sequence keeps each step demonstrable and testable.

## 1. Kernel baseline — implemented

- Native window and event-driven host.
- Retained identity, keyed reconciliation and cleanup.
- Basic flex layout and an ordered renderer-independent scene.
- wgpu presentation and shaped label/button text.
- Button, Label, Row, Column and Container.
- Pointer capture, focus, keyboard activation and headless behavior tests.
- Semantic metadata with stable IDs.
- Sequential backend detection, configurable compact allocations and startup memory probes.
- Retained GPU buffers, adjacent text batching and cached exposure redraws.
- Intrinsic-size comparisons that skip layout for same-size text changes.

## 2. Architecture validation — implementation delivered

- [x] AccessKit mapping and actions, tested through the Windows UI Automation bridge.
- [x] Vertical scroll containers with matching paint/hit-test transforms, nesting, keyboard scrolling and focus reveal.
- [x] Subtree paint caches and conservative logical damage bounds; GPU presentation still covers the full surface.
- [x] Coalesced worker/async wakeup handles with explicit shutdown behavior.
- [x] 1,000/10,000-label declaration and mutation measurements with paint/layout counters.
- [x] Application-owned component scopes and a declarative custom leaf contract.
- [ ] Manual screen-reader speech/navigation review, and native Linux/macOS validation.

The [milestone report](milestone-2.md) records the APIs, native checks, measurements and remaining limits. The workbench demonstrates the implemented slice. Passing UI Automation checks verifies the platform tree/actions; it does not substitute for a manual screen-reader review.

## 3. Text and controls — implementation delivered

- [x] Width-aware wrapping, installed generic/named font families, weight and italic.
- [x] Single-line selection, grapheme/word navigation, clipboard, bounded undo/redo and IME event routing.
- [x] TextInput, Checkbox, Radio, Slider, ComboBox and one-level menus.
- [x] Input/focus tests, shaped geometry checks, GPU pixel checks and Windows UI Automation control checks.
- [ ] Real OS IME candidate/dead-key review and native Linux/macOS interaction.
- [ ] Full accessible editable-text selection/navigation and Windows ExpandCollapsePattern support; the pinned AccessKit adapter does not implement the latter.

The [milestone report](milestone-3.md) describes the controls gallery, contracts, validation and limits. The counter remains near its previous memory baseline; editor storage and clipboard initialization are limited to the features that use them.

## 4. Application support — implementation delivered

- [x] Stable logical item identity (`ItemId`, `ListModel`, `ItemOrder`) distinct from realized widget identity.
- [x] Virtualized lists, trees and a sortable DataGrid realizing only the viewport.
- [x] Commands with shortcuts sharing the button's enabled state and one-shot response.
- [x] In-process drag/drop with keyboard and accessibility paths; incoming OS file drops.
- [x] Bounded worker/async mailbox and multi-window `run_windows` on one shared device.
- [x] Headless collection, drag and command tests plus a two-window smoke run.
- [ ] Variable row heights, multi-selection, cell editing, outgoing OS drags and native Linux/macOS multi-window checks.

The [milestone report](milestone-4.md) records the contracts, validation and limits. The application workbench demonstrates the implemented slice.

## 5. Tools — implementation delivered

- [x] `rust-gui-testing`: headless harness, semantic finders, real input paths and text snapshots of tree, layout, accessibility and scene.
- [x] Deterministic pixel snapshots with a bundled font in the opt-in GPU test.
- [x] Per-node change and repaint flags, declare/layout/paint/render timings, `tracing` spans and CSV export.
- [x] F12 inspector window: tree, node details with computed style, accessibility view, event log, frame timeline, selection/damage/repaint overlays.
- [x] Single- and multi-window hosts on one desktop loop.
- [ ] GPU timings, memory statistics, framework diagnostics (relayout/repaint/accessibility warnings) and cross-driver pixel tolerance in CI.

The [milestone report](milestone-5.md) describes the crates, contracts and validation. [Feature coverage](feature-coverage.md) maps all 62 goals of the [original feature list](feature-list.md) to their status.

## 6. Styling, theming and layout depth — implementation delivered

- [x] Theme tokens for every built-in control, dark and light themes, runtime switching and OS appearance ([milestone 6a](milestone-6.md)).
- [x] Inherited text properties, typed style classes, per-state paint overrides.
- [x] Stylesheet format with classes, `extends`, tokens and hot reload from disk.
- [x] Grid layout with tracks and areas, min/max constraints, aspect ratio, alignment and opt-in shrinking ([milestone 6b](milestone-6.md#milestone-6b-layout-depth)).
- [x] Focus scopes with trap/restore/memory and arrow navigation; in-window modal and non-modal dialogs; native modal windows ([milestone 6c](milestone-6.md#milestone-6c-focus-scopes-and-dialogs)).
- [x] Tooltips with a core clock and host wake requests, closable tabs, split panes, context menus and popovers ([milestone 6d](milestone-6.md#milestone-6d-tooltips-tabs-split-panes-context-menus-and-popovers)).
- [x] Transitions, enter/exit effects, layout animations and keyframes on GPU layers with group opacity and transforms ([milestone 6e](milestone-6.md#milestone-6e-animations)).

## 7. Advanced features — in progress

- [x] Docking: split and tab-group layouts, drop indicators, in-window floating groups, detached OS windows, a keyboard panel menu, text and serde persistence ([milestone 7a](milestone-7.md#milestone-7a-docking)).
- [x] Rich text blocks with links, a rope-backed `TextDocument` and a virtualized code editor ([milestone 7b](milestone-7.md#milestone-7b-rich-text-and-the-code-editor-foundation)).
- [x] Code intelligence: syntect highlighting, find and replace, bracket pairing and matching, multiple cursors, diagnostics ([milestone 7c](milestone-7.md#milestone-7c-code-intelligence)).
- [x] The WYSIWYG rich text editor with a Markdown subset for persistence ([milestone 7d](milestone-7.md#milestone-7d-the-wysiwyg-rich-text-editor)).
- [x] Bidirectional caret order: visual or logical arrows, caret affinity and direction flags in all three editors ([milestone 7e](milestone-7.md#milestone-7e-bidirectional-caret-order)).
- [x] Inherited text and layout direction with OS locale detection, mirrored layout and widgets ([milestone 7f](milestone-7.md#milestone-7f-text-direction-and-right-to-left-layout)).
- [x] Hot reload: watched files, and a development runner that rebuilds and restarts with state and window kept ([milestone 7g](milestone-7.md#milestone-7g-hot-reload)).
- [ ] Framework diagnostics.
- CPU rendering, outgoing OS drags, gestures and additional platform adapters.

Each phase needs its own acceptance criteria before expanding the public API. Items that appear only in the feature list are tracked in the coverage table.
