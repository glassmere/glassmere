use std::fmt;

/// Automatic selection probes one platform-compatible backend at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum GraphicsBackend {
    #[default]
    Auto,
    Direct3D12,
    Vulkan,
    Metal,
    OpenGl,
    /// No graphics adapter: windows are painted by the CPU renderer. Slower,
    /// and how an application runs at all where no driver works.
    Cpu,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum GpuMemoryPolicy {
    /// Small allocation blocks for UI workloads; larger resources still get
    /// dedicated allocations. Backend/driver support for this hint varies.
    #[default]
    Compact,
    /// wgpu's general-purpose memory-saving preset.
    Balanced,
}

impl GpuMemoryPolicy {
    pub(crate) fn hints(self) -> wgpu::MemoryHints {
        match self {
            Self::Compact => wgpu::MemoryHints::Manual {
                suballocated_device_memory_block_size: 4 * 1024 * 1024..16 * 1024 * 1024,
            },
            Self::Balanced => wgpu::MemoryHints::MemoryUsage,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GraphicsOptions {
    /// Explicit choices override WGPU_BACKEND and do not fall back to another API.
    pub backend: GraphicsBackend,
    /// Live non-sampler bindings, not widget count. This controls an up-front
    /// descriptor heap on Direct3D 12. Raise it for applications with many images.
    pub max_resource_bindings: u32,
    /// One refresh favors low-latency, event-driven UI over frame throughput.
    pub maximum_frame_latency: u32,
    pub memory_policy: GpuMemoryPolicy,
}

impl Default for GraphicsOptions {
    fn default() -> Self {
        Self {
            backend: GraphicsBackend::Auto,
            max_resource_bindings: 4096,
            maximum_frame_latency: 1,
            memory_policy: GpuMemoryPolicy::Compact,
        }
    }
}

impl GraphicsOptions {
    pub(crate) fn candidates(
        &self,
        compiled: wgpu::Backends,
        environment: Option<&str>,
    ) -> Vec<wgpu::Backends> {
        use wgpu::Backends as B;
        let requested = match self.backend {
            GraphicsBackend::Auto => environment.map(B::from_comma_list).unwrap_or_else(B::all),
            GraphicsBackend::Direct3D12 => B::DX12,
            GraphicsBackend::Vulkan => B::VULKAN,
            GraphicsBackend::Metal => B::METAL,
            GraphicsBackend::OpenGl => B::GL,
            GraphicsBackend::Cpu => B::empty(),
        } & compiled;
        [B::METAL, B::DX12, B::VULKAN, B::GL]
            .into_iter()
            .filter(|backend| requested.contains(*backend))
            .collect()
    }

    /// Whether windows should be painted without a graphics adapter. The
    /// environment variable is there to try the fallback on a working machine.
    pub(crate) fn wants_cpu(&self, environment: Option<&str>) -> bool {
        self.backend == GraphicsBackend::Cpu
            || environment.is_some_and(|value| value.eq_ignore_ascii_case("cpu"))
    }

    pub(crate) fn limits(&self) -> wgpu::Limits {
        wgpu::Limits {
            max_non_sampler_bindings: self.max_resource_bindings,
            ..Default::default()
        }
    }
}

pub(crate) async fn first_available<T, E: fmt::Display, F, Fut>(
    candidates: Vec<wgpu::Backends>,
    mut attempt: F,
) -> Result<T, String>
where
    F: FnMut(wgpu::Backends) -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut failures = Vec::new();
    for backend in candidates {
        match attempt(backend).await {
            Ok(value) => return Ok(value),
            Err(error) => failures.push(format!("{backend:?}: {error}")),
        }
    }
    if failures.is_empty() {
        Err("No compiled desktop backend matches the requested backend or WGPU_BACKEND".into())
    } else {
        Err(format!(
            "Could not initialize a graphics backend: {}",
            failures.join("; ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::Backends as B;

    #[test]
    fn explicit_backend_overrides_environment_and_cannot_escape_compiled_set() {
        let options = GraphicsOptions {
            backend: GraphicsBackend::Vulkan,
            ..Default::default()
        };
        assert_eq!(
            options.candidates(B::VULKAN | B::DX12, Some("dx12")),
            vec![B::VULKAN]
        );
        assert!(options.candidates(B::DX12, None).is_empty());
        assert_eq!(
            GraphicsOptions::default().candidates(B::VULKAN | B::DX12 | B::GL, Some("vulkan,gl")),
            vec![B::VULKAN, B::GL]
        );
    }

    #[test]
    fn the_cpu_renderer_is_asked_for_by_the_option_or_the_environment() {
        let cpu = GraphicsOptions {
            backend: GraphicsBackend::Cpu,
            ..Default::default()
        };
        assert!(cpu.wants_cpu(None));
        // An explicit choice leaves no adapter to try.
        assert!(cpu.candidates(B::all(), Some("vulkan")).is_empty());
        let auto = GraphicsOptions::default();
        assert!(!auto.wants_cpu(None) && !auto.wants_cpu(Some("vulkan")));
        assert!(auto.wants_cpu(Some("CPU")));
    }

    #[test]
    fn detection_stops_at_first_success_and_releases_failed_attempt() {
        use std::{
            cell::{Cell, RefCell},
            rc::Rc,
        };
        struct Probe(Rc<Cell<usize>>);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.set(self.0.get() - 1);
            }
        }
        let alive = Rc::new(Cell::new(0));
        let attempts = RefCell::new(Vec::new());
        let selected = pollster::block_on(first_available(
            vec![B::DX12, B::VULKAN, B::GL],
            |backend| {
                attempts.borrow_mut().push(backend);
                assert_eq!(
                    alive.get(),
                    0,
                    "failed backend must be dropped before fallback"
                );
                alive.set(1);
                let probe = Probe(alive.clone());
                async move {
                    if backend == B::DX12 {
                        Err("not available")
                    } else {
                        Ok((backend, probe))
                    }
                }
            },
        ))
        .unwrap();
        assert_eq!(selected.0, B::VULKAN);
        assert_eq!(*attempts.borrow(), vec![B::DX12, B::VULKAN]);
        assert_eq!(alive.get(), 1);
        drop(selected);
        assert_eq!(alive.get(), 0);
    }
}
