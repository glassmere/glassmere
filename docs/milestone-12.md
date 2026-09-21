# Milestone 12: GPU timings and memory statistics in the inspector

Implemented on 2026-09-21. The renderer can time its passes on the GPU's own clock and accounts for what it allocates; the F12 inspector shows both in a new GPU tab, and the CSV export carries them per frame.

Decisions taken before the work: timestamp queries with the CPU render time as the fallback; the framework's own accounting plus the driver's report where a backend gives one; measurements only while an inspector is open; a new tab plus CSV columns.

| Piece | Contract |
| --- | --- |
| `Renderer::set_profiling(bool)` | Off by default and free when off: no query set, no readback buffers. Switching it off drops both, including results still on their way. `profiling()` and `timestamps_supported()` report the state; the latter is true only when the host created the device with `wgpu::Features::TIMESTAMP_QUERY`. |
| `Renderer::gpu_profile()` | The most recent frame whose times have come back: `GpuProfile { frame, total, passes: Vec<GpuPassTime { label, duration }> }`. `frame` is the value `RenderStats::frames` had for that draw, so a host can tell which draw it was. Results arrive a few frames late and never block: at most four readbacks are in flight, and a frame that finds none free is not timed. Passes are `main` and `layer WxH`; at most 32 per frame are timed. |
| `RenderStats` | Gained `frames` (every draw, cached replays included), `passes`, `draw_calls`, `quads` and `memory`. |
| `MemoryStats` | Bytes the renderer itself allocated: `buffers`, `image_atlas`, `image_textures`, `layers`, `profiling`, their `total` and the `peak` since `Renderer::reset_peak()`. `glyph_atlas` is an `Option` and always `None` today: glyphon does not expose the size of its atlas, and an invented number would be worse than a missing one. |
| `Renderer::driver_memory(&device)` | The driver allocator's `allocated`, `reserved` and `allocations` for the whole device, where wgpu reports it (Direct3D 12 and Vulkan), else `None`. |
| `rust_gui_devtools::GpuSnapshot` | Plain data, so the inspector still has no renderer dependency: adapter name, whether timestamps exist, passes of the last timed frame, counters, memory by category, peak, driver report. `Recorder::gpu(snapshot, completed)` stores it and lands a late GPU time on the frame it belongs to by frame number; a time for a frame that has left the ring is dropped. |
| `FrameRecord` and CSV | `gpu: Option<Duration>`, `draw_calls`, `gpu_memory`; the CSV gained `gpu_us,draw_calls,gpu_memory_bytes`. An unknown GPU time is an empty cell, not a zero that would read as a measurement. The Frames tab gained a gpu column. |
| GPU tab | Adapter and backend; last and average GPU frame time; draw calls, quads, text batches and render passes; a bar per pass; memory by category with bars, total and peak; the driver's report. Without timestamp queries it says that GPU time is unknown and that the render column is CPU time, which measures the driver's queue and not the GPU. |
| Host | Requests the timestamp feature when the adapter has it. Profiling is on while any inspector is open, for every window of the device, because the renderer is shared and switching it per window would drop readbacks in flight. A completed time is routed to the window that drew that renderer frame, which is often not the window drawing when it arrives. Only inspected windows build snapshots. |

## Validation

Renderer: unit tests for the accounting, and GPU tests (23 in the suite now, opt-in as before) covering timed frames with the labels of layer passes, profiling off releasing everything and leaving replays identical, a device created without the timestamp feature, memory by category with a peak that survives eviction, and counters against a known scene. Devtools: three tests, for a late time landing on its frame and in the CSV, the content of the tab, and the messages without timestamps or without a report.

The workspace suite is 235 tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features; all 16 examples pass their smoke run, two of them also with the inspector open; `scripts/test-theming.ps1` and `scripts/test-controls.ps1` pass.

On a real window (images example, NVIDIA GeForce RTX 3070, Direct3D 12) the tab showed a main pass of 0.026 ms, 39 draw calls, 16.0 MiB of image atlas, 0.3 MiB of image textures, and a driver report of 28.7 MiB allocated and 52.0 MiB reserved.

## Remaining limits

- The glyph atlas is not counted, so the renderer's total is lower than the truth by that texture.
- Timestamps were checked on one adapter. The path without them is covered by tests that build a device without the feature, not by a real adapter that lacks it.
- Pass times only: no timing of individual draws, no pipeline statistics.
- The driver's report covers the whole device, including the surfaces, and is not split by window.
- An inspected window that is idle does not redraw, so its numbers are those of its last frame.
- CPU allocations and the text shaping split are still not measured.
