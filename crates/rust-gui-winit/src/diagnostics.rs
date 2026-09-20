//! Explicitly enabled profiling hooks; absent from ordinary builds.

pub(crate) fn checkpoint(stage: &str, device: Option<&wgpu::Device>) {
    #[cfg(feature = "diagnostics")]
    {
        use std::io::{self, Write};
        if std::env::var_os("RUST_GUI_MEMORY_TRACE").is_none() {
            return;
        }
        if let Some(device) = device {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            if let Some(report) = device.generate_allocator_report() {
                eprintln!(
                    "GPU_MEMORY {stage} allocated={} reserved={} allocations={}",
                    report.total_allocated_bytes,
                    report.total_reserved_bytes,
                    report.allocations.len()
                );
            }
        }
        println!("MEMORY_STAGE {stage}");
        let _ = io::stdout().flush();
        let _ = io::stdin().read_line(&mut String::new());
    }
    #[cfg(not(feature = "diagnostics"))]
    let _ = (stage, device);
}

pub(crate) fn memory_hints(default: wgpu::MemoryHints) -> wgpu::MemoryHints {
    #[cfg(feature = "diagnostics")]
    if std::env::var("RUST_GUI_MEMORY_PROFILE").as_deref() == Ok("compact") {
        return wgpu::MemoryHints::Manual {
            suballocated_device_memory_block_size: 4 * 1024 * 1024..16 * 1024 * 1024,
        };
    }
    #[cfg(feature = "diagnostics")]
    if std::env::var("RUST_GUI_MEMORY_PROFILE").as_deref() == Ok("memory") {
        return wgpu::MemoryHints::MemoryUsage;
    }
    default
}

pub(crate) fn frame_latency(default: u32) -> u32 {
    #[cfg(feature = "diagnostics")]
    if let Ok(value) = std::env::var("RUST_GUI_FRAME_LATENCY")
        && let Ok(value) = value.parse::<u32>()
    {
        return value.max(1);
    }
    default
}

pub(crate) fn device_limits(default: wgpu::Limits) -> wgpu::Limits {
    #[cfg(feature = "diagnostics")]
    if let Ok(limit) = std::env::var("RUST_GUI_BINDING_LIMIT")
        && let Ok(limit) = limit.parse()
    {
        return wgpu::Limits {
            max_non_sampler_bindings: limit,
            ..default
        };
    }
    default
}

pub(crate) fn render_statistics(stage: &str, stats: rust_gui_render_wgpu::RenderStats) {
    #[cfg(feature = "diagnostics")]
    if std::env::var_os("RUST_GUI_MEMORY_TRACE").is_some() {
        eprintln!("RENDER_STATS {stage} {stats:?}");
    }
    #[cfg(not(feature = "diagnostics"))]
    let _ = (stage, stats);
}
