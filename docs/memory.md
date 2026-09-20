# Memory investigation — 2026-09-20

The counter's reported 115 MiB was reproducible in an optimized build. The original host initialized all wgpu backends and used its performance-oriented default allocator. This loaded unused graphics stacks and committed substantially more memory than necessary. The font system was a much smaller contribution in the isolated startup probe.

The host now probes one backend at a time and stops after the first successful initialization. It filters by compiled/platform support, requests a window-compatible adapter, and creates only that backend's device. Direct3D 12 is first on Windows, Metal on macOS and Vulkan on Linux; fallbacks run only after failure. An explicit `GraphicsBackend` overrides automatic selection, while `WGPU_BACKEND` restricts automatic candidates. Detecting a DLL alone would not prove adapter, feature or presentation support.

## Native counter results

Windows, Rust 1.98.0, NVIDIA GeForce RTX 3070, NVIDIA driver 616.92. Separate processes, original example UI at its initial state and default window size, sampled four seconds after launch. Both builds were optimized; the `profiling` Cargo profile inherits `release` without additional overrides.

| Configuration | Private resident RAM (MiB) | Total resident RAM (MiB) | Private commit (MiB) |
| --- | ---: | ---: | ---: |
| Original: all backends, default allocations | 115.14 | 183.84 | 385.31 |
| Vulkan only, memory-saving allocations | 94.08 | 161.18 | 178.27 |
| OpenGL only, memory-saving allocations | 86.49 | 124.31 | 136.76 |
| Direct3D 12 only, memory-saving allocations | 78.16 | 130.33 | 170.90 |
| First fix: automatic selection (Direct3D 12), repeat | 74.98 | 115.86 | 165.05 |
| Current release: compact allocation, smaller binding heap, retained/batched buffers | 67.38 | 109.20 | 121.62 |

The current native reading reduces private resident RAM by approximately 41% and private commit by 68% relative to the original. It recorded 0 ms of process CPU during a subsequent two-second idle sample, at the resolution of Windows process counters. This final sample used the release executable without the diagnostics feature. These are individual local samples, not benchmark medians or cross-machine guarantees. Driver caches, shared pages, desktop activity and sampling time affect results. Resident RAM, committed memory, virtual address space and GPU VRAM are different metrics.

The native measurement script reads `PrivateWorkingSetSize`, `WorkingSetSize` and `PrivateUsage` from Windows [PROCESS_MEMORY_COUNTERS_EX2](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex2). It requires a Windows version supporting that structure. It launches its own counter process and closes that process after sampling.

After milestone 2 added scroll/component support, paint caches, wakeups and the AccessKit bridge, a new ordinary release counter sample measured **67.16 MiB private resident**, **114.79 MiB total resident**, and **122.75 MiB private commit**, with 0 ms process CPU over two idle seconds. Accessibility was not active in this sample. The earlier table and stress run remain historical measurements of the memory-optimization pass, rather than fresh measurements of every milestone-2 configuration.

## Attribution probe

A separate process pauses between startup phases so the parent can measure it. With all backends enabled, total resident memory reached approximately 127 MiB at device creation, before creating our renderer or font system. Adding the renderer and rendering one label reached approximately 131 MiB. A separate fonts-only process grew from 7.45 to 8.56 MiB after system font discovery and shaping one sample string.

These results identify graphics initialization as the dominant measured cost in that probe. They are not exact heap allocation accounting and do not reproduce window presentation, the full demo, or long-running interaction. In particular, Vulkan looked smaller than Direct3D in the offscreen probe, but Direct3D used less private RAM for the actual native counter. The window measurements determined the Windows default.

## Native startup attribution and further reductions

The optional `diagnostics` feature adds checkpoints inside the actual window host. These pause for the measuring parent and wait for GPU work before reporting allocator statistics. They are absent from ordinary builds. A current Direct3D 12 stress-process startup measured:

| Stage | Private resident RAM (MiB) |
| --- | ---: |
| Native window created | 2.07 |
| Compatible adapter detected | 31.33 |
| GPU device created | 46.03 |
| Renderer and font system created | 46.65 |
| First layout finished | 46.96 |
| First frame presented | 67.68 |

Most measured resident growth precedes our widget layout. The large remaining costs are associated with driver/device initialization and first-frame rendering/presentation, rather than the counter's state or retained tree. These stage deltas include driver caches, library mappings and allocations; they are not exact heap ownership accounting.

The second optimization pass made these changes:

- **Smaller Direct3D binding heap.** wgpu 29.0.4 defaults `Limits::max_non_sampler_bindings` to 1,000,000, and its Direct3D backend reserves a descriptor heap up front. Requesting 4,096 cut private commit by approximately 30 MiB in paired native probes; resident RAM barely changed from this adjustment alone. The public option can be raised for resource-heavy applications. It limits live GPU bindings, not widgets.
- **Compact allocation blocks.** The default now hints at 4–16 MiB device-memory blocks. wgpu's `MemoryUsage` preset uses 8–64 MiB device blocks and 4–32 MiB host blocks in the inspected allocator configuration. The compact hint reduced initial reported GPU allocator reservation from 12 to 8 MiB. This report excludes driver-owned allocations and is not the process working set.
- **Maximum frame latency of one.** This requests a shorter presentation queue for an event-driven UI; actual swapchain allocation depends on the backend. Compact blocks and the shorter queue brought initial private resident RAM into the high 60s. The limit is configurable because continuous animation can favor more frames in flight.
- **Retained rendering data.** Quad vertices reuse a growable GPU buffer. Unchanged exposure redraws skip glyph/geometry preparation and uploads. Adjacent text shares batches while retaining per-area clipping and painter order. A bounded pool avoids recreating text renderers when sections toggle.
- **Less layout work.** Changed text is measured, but layout is invalidated only when its intrinsic size changes. Paint and semantic labels still update. This reduces work for counters with equal-width digits without claiming material startup RAM savings.

## Changing-scene stress run

The example's stress mode increments the count each declaration, toggles details and reorders keyed labels every 30 declarations. A 6,000-frame run with current rendering/default allocation policy measured:

| Checkpoint | Private resident RAM (MiB) | Private commit (MiB) | GPU allocated (MiB) | GPU reserved (MiB) |
| --- | ---: | ---: | ---: | ---: |
| First presentation | 67.68 | 124.27 | 1.5625 | 8 |
| Frame 60 | 69.20 | 125.25 | — | — |
| Frame 600 | 71.29 | 126.54 | 1.75 | 12 |
| Frame 6,000 | 59.36 | 114.64 | 1.75 | 12 |

The renderer reported one 2,048-byte quad-buffer allocation and seven text-renderer allocations throughout the sampled stress checkpoints, versus 14 separate text batches before batching. The later GPU allocation total and allocation count (15) were identical at frames 600 and 6,000. Initial growth is consistent with glyph-atlas warmup as more digits appear; memory then leveled off in this workload. The later decline in process memory reflects runtime/driver/OS behavior, so 59 MiB is not presented as the normal idle baseline. This run preceded the final intrinsic-size layout optimization; that optimization was covered separately by the kernel tests and final release run.

Remaining work includes a lightweight CPU-rendering path to avoid this GPU/driver baseline, and measuring larger temporary scenes and font/glyph diversity. Quad buffers and CPU vector capacities retain their high-water allocation, and font/raster caches need separate stress coverage. This run does not establish a minimum possible footprint or prove that every workload is free of unbounded cache growth.

## Reproduce

Build an optimized counter separately from any running release executable:

```powershell
cargo build -p rust-gui --example counter --profile profiling --locked
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-window-memory.ps1 -Executable target/profiling/examples/counter.exe
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-window-memory.ps1 -Executable target/profiling/examples/counter.exe -Backend vulkan
```

The execution-policy option applies only to that PowerShell invocation. It does not change the system policy.

For startup checkpoints, allocation-policy comparisons and changing-scene stress, build with diagnostics:

```powershell
cargo build -p rust-gui --example counter --profile profiling --features diagnostics --locked
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-window-memory.ps1 -Executable target/profiling/examples/counter.exe -TraceStages
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-window-memory.ps1 -Executable target/profiling/examples/counter.exe -TraceStages -Stress -StressFrames 6000
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-window-memory.ps1 -Executable target/profiling/examples/counter.exe -TraceStages -MemoryProfile memory -FrameLatency 2 -BindingLimit 1000000
```

The last command restores the earlier allocation settings for comparison, keeping single-backend selection. Memory-profile, frame-latency and binding-limit script overrides require the diagnostics build; normal builds use `AppOptions::graphics`. The script scopes environment changes to its child process and reads real process counters without forcing working-set trimming. Checkpoint output includes CPU time and GPU allocator reports; pauses and GPU waits make this a memory probe, not a throughput benchmark.

To isolate graphics/font initialization without a window:

```powershell
cargo build -p rust-gui-render-wgpu --example memory_probe --release --locked
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-memory.ps1 -Mode fonts
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-memory.ps1 -Mode all
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-memory.ps1 -Mode vulkan -MemoryHint memory
```

The stage probe reports total working set and private commit, not private working set. Use the native-window script when comparing against the native results above.

Validation after the changes: 11 kernel tests and 2 backend-policy tests; workspace all-target/all-feature tests; Clippy with warnings denied; the facade without desktop features; and GPU pixel-readback tests on Direct3D 12 and Vulkan. The GPU tests also check clipping across merged text batches, retained-buffer reuse and DPI invalidation. Native probes in the investigation created, presented and closed counter windows on Vulkan, Direct3D 12 and OpenGL. The final release and the extended stress run used Direct3D 12. No OS or driver settings were changed.
