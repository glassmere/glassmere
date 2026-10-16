mod support;

use rust_gui_core::{Context, MonospaceMeasurer, Style};
use rust_gui_devtools::{GpuSnapshot, Inspector, InspectorModel, Recorder, Tab};
use rust_gui_scene::Size;
use std::time::Duration;
use support::Shown;

const TARGET: Size = Size::new(400.0, 300.0);

fn declare(context: &mut Context, recorder: &mut Recorder) {
    let stats = context.run(TARGET, &mut MonospaceMeasurer, |ui| {
        ui.keyed("page", |ui| {
            ui.column(Style::default(), |ui| ui.label("content"))
        });
    });
    recorder.frame(
        stats,
        Duration::from_micros(500),
        Duration::from_micros(900),
    );
}

fn snapshot(timestamps: bool) -> GpuSnapshot {
    GpuSnapshot {
        adapter: "Test adapter (Dx12)".into(),
        software: false,
        timestamps,
        passes: if timestamps {
            vec![
                ("main".into(), Duration::from_micros(1500)),
                ("layer 640x480".into(), Duration::from_micros(500)),
            ]
        } else {
            Vec::new()
        },
        draw_calls: 12,
        quads: 340,
        text_batches: 3,
        render_passes: 2,
        memory: vec![
            ("Buffers".into(), 1 << 20),
            ("Image atlas".into(), 16 << 20),
            ("Layers".into(), 3 << 20),
        ],
        memory_peak: 32 << 20,
        driver: timestamps.then_some((64 << 20, 96 << 20)),
    }
}

/// Labels of the inspector's GPU tab for a model.
fn shown(model: InspectorModel) -> Vec<String> {
    let mut shown = Shown::new(Inspector::showing(Tab::Gpu), model);
    shown.frame();
    let context = shown.context;
    context
        .snapshots()
        .into_iter()
        .map(|node| node.semantics.label)
        .filter(|label| !label.is_empty())
        .collect()
}

#[test]
fn gpu_times_arrive_late_and_land_on_their_frame_and_the_csv_says_what_is_unknown() {
    let mut context = Context::new();
    let mut recorder = Recorder::new(8);
    for _ in 0..3 {
        declare(&mut context, &mut recorder);
        recorder.gpu(snapshot(true), None);
    }
    // The time of frame 1 arrives while frame 3 is the latest.
    recorder.gpu(snapshot(true), Some((1, Duration::from_micros(2000))));
    let frames: Vec<_> = recorder.frames().iter().collect();
    assert_eq!(frames[0].gpu, Some(Duration::from_micros(2000)));
    assert_eq!((frames[1].gpu, frames[2].gpu), (None, None));
    assert_eq!(frames[2].draw_calls, 12);
    assert_eq!(frames[2].gpu_memory, 20 << 20);
    // A time for a frame that has left the ring is dropped quietly.
    recorder.gpu(snapshot(true), Some((999, Duration::from_micros(1))));

    let csv = recorder.csv();
    let mut lines = csv.lines();
    assert!(
        lines
            .next()
            .unwrap()
            .ends_with(",gpu_us,draw_calls,gpu_memory_bytes")
    );
    assert!(lines.next().unwrap().ends_with(",2000,12,20971520"));
    // Unknown is empty, not zero.
    assert!(lines.next().unwrap().ends_with(",,12,20971520"));
}

#[test]
fn the_gpu_tab_shows_times_passes_counters_and_memory() {
    let mut context = Context::new();
    let mut recorder = Recorder::new(8);
    declare(&mut context, &mut recorder);
    recorder.gpu(snapshot(true), Some((1, Duration::from_micros(2000))));
    let model = InspectorModel::capture(&context, "Target", TARGET, &recorder);
    let labels = shown(model);
    let has = |needle: &str| labels.iter().any(|label| label.contains(needle));
    assert!(has("Test adapter (Dx12)"));
    assert!(has("last 2.00 ms") && has("average 2.00 ms over 1 frames"));
    assert!(has("12 draw calls") && has("340 quads") && has("2 render passes"));
    assert!(has("layer 640x480") && has("0.500 ms"));
    assert!(has("20.0 MiB") && has("peak 32.0 MiB"), "{labels:?}");
    assert!(has("Image atlas") && has("16.0 MiB"));
    assert!(has("64.0 MiB allocated and 96.0 MiB reserved"));
}

#[test]
fn without_timestamps_or_without_a_report_the_tab_says_so() {
    let mut context = Context::new();
    let mut recorder = Recorder::new(8);
    declare(&mut context, &mut recorder);
    let model = InspectorModel::capture(&context, "Target", TARGET, &recorder);
    assert!(
        shown(model)
            .iter()
            .any(|label| label.contains("does not report GPU statistics"))
    );

    recorder.gpu(snapshot(false), None);
    let model = InspectorModel::capture(&context, "Target", TARGET, &recorder);
    let labels = shown(model);
    let has = |needle: &str| labels.iter().any(|label| label.contains(needle));
    assert!(has("no timestamp queries") && has("not the GPU"));
    assert!(!has("Passes of the last timed frame"));
    assert!(has("does not report the driver's allocator"));
    // Memory and counters do not depend on timestamps.
    assert!(has("12 draw calls") && has("20.0 MiB"));

    // A window painted without an adapter says so and claims nothing else.
    recorder.gpu(
        GpuSnapshot {
            adapter: "CPU renderer, no graphics adapter".into(),
            software: true,
            ..GpuSnapshot::default()
        },
        None,
    );
    let model = InspectorModel::capture(&context, "Target", TARGET, &recorder);
    let labels = shown(model);
    let has = |needle: &str| labels.iter().any(|label| label.contains(needle));
    assert!(has("CPU renderer, no graphics adapter"));
    assert!(has("painted by the CPU renderer"));
    assert!(!has("draw calls") && !has("MiB") && !has("timestamp"));
}
