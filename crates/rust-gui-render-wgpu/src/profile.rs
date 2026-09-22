//! GPU pass timings through timestamp queries, and the renderer's own memory
//! accounting. Timings run only while `Renderer::set_profiling(true)`; the
//! memory counters are plain integers and always on.
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

/// Render passes timed per frame. Passes beyond this (a frame with more than
/// 31 offscreen layers) run untimed and are missing from the profile.
pub const MAX_TIMED_PASSES: usize = 32;
/// Readbacks that may be waiting for the GPU at once.
const IN_FLIGHT: usize = 4;

const FREE: u8 = 0;
const PENDING: u8 = 1;
const READY: u8 = 2;
const FAILED: u8 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuPassTime {
    /// `main` for the target, `layer WIDTHxHEIGHT` for an offscreen layer.
    /// Layer composites are draws inside their parent's pass, not passes.
    pub label: String,
    pub duration: Duration,
}

/// GPU times of the most recent frame whose timestamps have come back, which
/// is usually two or three frames behind the one being drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuProfile {
    /// `RenderStats::frames` of the frame these times belong to.
    pub frame: u64,
    /// From the first timed pass beginning to the last one ending.
    pub total: Duration,
    /// In execution order: layers first, `main` last.
    pub passes: Vec<GpuPassTime>,
}

/// Bytes of GPU memory the renderer itself allocated, by category. Exact for
/// what is listed; it cannot see driver overhead, alignment or other users of
/// the device (see `Renderer::driver_memory`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryStats {
    /// The retained quad and image vertex buffers.
    pub buffers: u64,
    /// glyphon does not expose the size of its glyph atlas, its text vertex
    /// buffers or its uniform buffers, so this is `None`: unknown, not zero.
    /// It counts as zero in `total`.
    pub glyph_atlas: Option<u64>,
    /// Shared image atlas pages.
    pub image_atlas: u64,
    /// Standalone image textures, vector rasters included.
    pub image_textures: u64,
    /// Offscreen layer textures, pooled ones included while pooled.
    pub layers: u64,
    /// The offscreen canvas, which exists only once a host sends damage.
    pub canvas: u64,
    /// Timestamp resolve and readback buffers while profiling.
    pub profiling: u64,
    /// Sum of the categories above.
    pub total: u64,
    /// Highest `total` since creation or `Renderer::reset_peak`.
    pub peak: u64,
}

impl MemoryStats {
    /// Recomputes `total` from the categories and raises `peak` to it.
    pub(crate) fn settle(&mut self) {
        self.total = self.buffers
            + self.glyph_atlas.unwrap_or(0)
            + self.image_atlas
            + self.image_textures
            + self.layers
            + self.canvas
            + self.profiling;
        self.peak = self.peak.max(self.total);
    }
}

/// What the driver's allocator reports for the whole device: every renderer,
/// surface and other user of it, not just this renderer. Only some backends
/// have one (Vulkan and Direct3D 12 through gpu-allocator).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DriverMemory {
    /// Bytes in live allocations.
    pub allocated: u64,
    /// Bytes reserved in memory blocks, unused regions included.
    pub reserved: u64,
    pub allocations: usize,
}

struct Readback {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
    frame: u64,
    labels: Vec<String>,
}

pub(crate) struct Timestamps {
    pub queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    ring: Vec<Readback>,
    /// Nanoseconds per timestamp tick.
    period: f32,
    pub latest: Option<GpuProfile>,
}

const SLOT_BYTES: u64 = (MAX_TIMED_PASSES * 2 * 8) as u64;

/// Durations from resolved begin/end tick pairs. A pair that runs backwards
/// (timestamps are not guaranteed monotonic across resets) counts as zero.
pub(crate) fn profile(frame: u64, labels: &[String], ticks: &[u64], period: f32) -> GpuProfile {
    let nanos = |ticks: u64| Duration::from_nanos((ticks as f64 * f64::from(period)) as u64);
    let pairs = || ticks.as_chunks::<2>().0.iter().take(labels.len());
    let first = pairs().map(|pair| pair[0]).min().unwrap_or(0);
    let last = pairs().map(|pair| pair[1]).max().unwrap_or(0);
    GpuProfile {
        frame,
        total: nanos(last.saturating_sub(first)),
        passes: labels
            .iter()
            .zip(pairs())
            .map(|(label, pair)| GpuPassTime {
                label: label.clone(),
                duration: nanos(pair[1].saturating_sub(pair[0])),
            })
            .collect(),
    }
}

impl Timestamps {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("rust-gui pass times"),
                ty: wgpu::QueryType::Timestamp,
                count: (MAX_TIMED_PASSES * 2) as u32,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rust-gui pass times"),
                size: SLOT_BYTES,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            ring: Vec::new(),
            period: queue.get_timestamp_period(),
            latest: None,
        }
    }

    pub fn bytes(&self) -> u64 {
        SLOT_BYTES * (1 + self.ring.len() as u64)
    }

    /// Collects readbacks the GPU has finished, without waiting for any.
    pub fn collect(&mut self, device: &wgpu::Device) {
        // Runs map callbacks of finished submissions; never blocks.
        let _ = device.poll(wgpu::PollType::Poll);
        for readback in &mut self.ring {
            match readback.state.load(Ordering::Acquire) {
                READY => {
                    let bytes = (readback.labels.len() * 16) as u64;
                    let ticks: Vec<u64> = readback
                        .buffer
                        .slice(..bytes)
                        .get_mapped_range()
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .map(|tick| u64::from_le_bytes(*tick))
                        .collect();
                    readback.buffer.unmap();
                    if self
                        .latest
                        .as_ref()
                        .is_none_or(|latest| latest.frame < readback.frame)
                    {
                        self.latest = Some(profile(
                            readback.frame,
                            &readback.labels,
                            &ticks,
                            self.period,
                        ));
                    }
                    readback.state.store(FREE, Ordering::Release);
                }
                FAILED => readback.state.store(FREE, Ordering::Release),
                _ => {}
            }
        }
    }

    /// A readback buffer for this frame. When all are still waiting for the
    /// GPU, the oldest is dropped and replaced rather than waited for.
    fn slot(&mut self, device: &wgpu::Device) -> usize {
        if let Some(free) = self
            .ring
            .iter()
            .position(|readback| readback.state.load(Ordering::Acquire) == FREE)
        {
            return free;
        }
        let fresh = Readback {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rust-gui pass times readback"),
                size: SLOT_BYTES,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            state: Arc::new(AtomicU8::new(FREE)),
            frame: 0,
            labels: Vec::new(),
        };
        if self.ring.len() < IN_FLIGHT {
            self.ring.push(fresh);
            return self.ring.len() - 1;
        }
        let oldest = (0..self.ring.len())
            .min_by_key(|index| self.ring[*index].frame)
            .unwrap();
        self.ring[oldest] = fresh;
        oldest
    }

    /// Resolves this frame's queries into a readback buffer. Returns the slot
    /// to hand to `submitted` once the encoder has been submitted.
    pub fn resolve(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        frame: u64,
        labels: Vec<String>,
    ) -> usize {
        let slot = self.slot(device);
        let count = labels.len() as u32 * 2;
        encoder.resolve_query_set(&self.queries, 0..count, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(
            &self.resolve,
            0,
            &self.ring[slot].buffer,
            0,
            u64::from(count) * 8,
        );
        self.ring[slot].frame = frame;
        self.ring[slot].labels = labels;
        slot
    }

    pub fn submitted(&mut self, slot: usize) {
        let readback = &self.ring[slot];
        let bytes = (readback.labels.len() * 16) as u64;
        let state = readback.state.clone();
        state.store(PENDING, Ordering::Release);
        readback
            .buffer
            .slice(..bytes)
            .map_async(wgpu::MapMode::Read, move |result| {
                state.store(
                    if result.is_ok() { READY } else { FAILED },
                    Ordering::Release,
                );
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_totals_its_categories_and_the_peak_only_rises() {
        let mut memory = MemoryStats {
            buffers: 256,
            image_atlas: 4096,
            image_textures: 1000,
            layers: 64,
            profiling: 8,
            ..MemoryStats::default()
        };
        memory.settle();
        assert_eq!((memory.total, memory.peak), (5424, 5424));
        memory.image_textures = 0;
        memory.settle();
        assert_eq!((memory.total, memory.peak), (4424, 5424));
        // An unknown glyph atlas counts as nothing; a known one counts.
        memory.glyph_atlas = Some(10_000);
        memory.settle();
        assert_eq!((memory.total, memory.peak), (14_424, 14_424));
    }

    #[test]
    fn tick_pairs_become_pass_times_and_a_frame_span() {
        let labels = ["layer 64x64".to_owned(), "main".to_owned()];
        // Half a nanosecond per tick; the second pair runs backwards.
        let ticks = [1000, 5000, 9000, 7000, 123, 456];
        let profile = profile(7, &labels, &ticks, 0.5);
        assert_eq!(profile.frame, 7);
        assert_eq!(profile.passes[0].duration, Duration::from_nanos(2000));
        assert_eq!(profile.passes[1].label, "main");
        assert_eq!(profile.passes[1].duration, Duration::ZERO);
        // First begin to last end, over the labelled pairs only.
        assert_eq!(profile.total, Duration::from_nanos(3000));
        assert_eq!(super::profile(1, &[], &[], 1.0).total, Duration::ZERO);
    }
}
