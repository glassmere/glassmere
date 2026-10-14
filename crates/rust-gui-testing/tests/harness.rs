use rust_gui_core::Button;
use rust_gui_core::{Key, Length, Style, Ui};
use rust_gui_scene::Size;
use rust_gui_testing::{Harness, assert_snapshot};

const SNAPSHOTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots");

fn counter(count: &mut i32, ui: &mut Ui<'_>) {
    ui.keyed("page", |ui| {
        ui.column(
            Style {
                padding: 8.0,
                gap: 8.0,
                width: Length::Percent(1.0),
                ..Style::default()
            },
            |ui| {
                ui.keyed("count", |ui| ui.label(format!("Count: {count}")));
                ui.keyed("actions", |ui| {
                    ui.row(Style::default(), |ui| {
                        if ui.keyed("increment", |ui| ui.button("Increase")).clicked() {
                            *count += 1;
                        }
                        if ui.keyed("decrement", |ui| ui.button("Decrease")).clicked() {
                            *count -= 1;
                        }
                        ui.keyed("reset", |ui| {
                            ui.button(Button::new("Reset").enabled(false).style(Style::button()))
                        });
                    })
                });
            },
        )
    });
}

fn harness() -> Harness<i32> {
    Harness::new(Size::new(320.0, 200.0), 0, counter)
}

#[test]
fn semantic_finders_drive_pointer_and_keyboard_through_real_input() {
    let mut harness = harness();
    let increase = harness.by_label("Increase");
    assert_eq!(harness.find(&["page", "actions", "increment"]), increase);
    harness.click(increase);
    assert_eq!(*harness.state(), 1);
    assert_eq!(
        harness.by_label("Count: 1"),
        harness.find(&["page", "count"])
    );
    // The click focused Increase; Tab moves on, Shift+Tab returns, Enter activates.
    let decrease = harness.by_label("Decrease");
    harness.press(Key::Tab);
    assert_eq!(harness.context().focused(), Some(decrease));
    harness.press(Key::Enter);
    assert_eq!(*harness.state(), 0);
    harness.press_shift(Key::Tab);
    assert_eq!(harness.context().focused(), Some(increase));
    harness.press(Key::Enter);
    assert_eq!(*harness.state(), 1);
    let reset = harness.by_label("Reset");
    harness.click(reset);
    assert_eq!(*harness.state(), 1);
    assert!(harness.query_label("Missing").is_none());
}

#[test]
fn snapshots_are_stable_after_one_click() {
    let mut harness = harness();
    let increase = harness.by_label("Increase");
    harness.click(increase);
    assert_snapshot(SNAPSHOTS, "counter-tree", &harness.snapshot_tree());
    assert_snapshot(SNAPSHOTS, "counter-layout", &harness.snapshot_layout());
    assert_snapshot(
        SNAPSHOTS,
        "counter-accessibility",
        &harness.snapshot_accessibility(),
    );
    assert_snapshot(SNAPSHOTS, "counter-scene", &harness.snapshot_scene());
}

#[test]
#[should_panic(expected = "ambiguous")]
fn ambiguous_labels_report_both_key_paths() {
    let harness = Harness::new(Size::new(200.0, 200.0), (), |_, ui| {
        ui.keyed("a", |ui| {
            ui.row(Style::default(), |ui| {
                ui.keyed("x", |ui| ui.button("Same"));
            })
        });
        ui.keyed("b", |ui| {
            ui.row(Style::default(), |ui| {
                ui.keyed("y", |ui| ui.button("Same"));
            })
        });
    });
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| harness.by_label("Same")));
    let message = *result.unwrap_err().downcast::<String>().unwrap();
    assert!(
        message.contains("a/x") && message.contains("b/y"),
        "{message}"
    );
    panic!("{message}");
}

#[test]
fn mismatching_snapshots_name_the_first_differing_line() {
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        return; // updating rewrites instead of comparing
    }
    let dir = std::env::temp_dir().join(format!("rust-gui-testing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sample.snap"), "one\r\ntwo\r\n").unwrap();
    assert_snapshot(&dir, "sample", "one\ntwo\n");
    let result = std::panic::catch_unwind(|| assert_snapshot(&dir, "sample", "one\nthree\n"));
    let message = *result.unwrap_err().downcast::<String>().unwrap();
    assert!(
        message.contains("line 2") && message.contains("three"),
        "{message}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
