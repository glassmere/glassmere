use rust_gui::{
    AppOptions, Color, Component, CustomWidget, Length, Primitive, Rect, Role, Semantics, Size,
    Style, Ui, run_with_wakeup,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};

struct Counter {
    name: &'static str,
    value: u32,
}

impl Component for Counter {
    fn ui(&mut self, ui: &mut Ui<'_>) {
        ui.label_keyed("value", format!("{} count: {}", self.name, self.value));
        if ui
            .button_keyed("increment", format!("Increase {}", self.name))
            .clicked()
        {
            self.value += 1;
        }
    }
}

fn custom_action(count: u32) -> CustomWidget {
    CustomWidget {
        size: Size::new(230.0, 44.0),
        semantics: Semantics {
            role: Role::Button,
            label: format!("Custom action: {count}"),
            disabled: false,
            focusable: true,
        },
        primitives: vec![
            Primitive::Quad {
                rect: Rect {
                    x: 4.0,
                    y: 4.0,
                    width: 222.0,
                    height: 36.0,
                },
                color: Color::rgb(22, 105, 99),
                radius: 8.0,
            },
            Primitive::Text {
                rect: Rect {
                    x: 14.0,
                    y: 10.0,
                    width: 205.0,
                    height: 30.0,
                },
                text: format!("Custom action: {count}"),
                font_size: 16.0,
                color: Color::WHITE,
            },
        ],
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let (handle_sender, handle_receiver) = std::sync::mpsc::channel();
    run_with_wakeup(
        AppOptions {
            title: "Rust GUI / Architecture workbench".into(),
            size: Size::new(920.0, 820.0),
            ..AppOptions::default()
        },
        move |wake| {
            handle_sender.send(wake.clone()).unwrap();
            let updates = Arc::new(AtomicU32::new(0));
            let ready = Arc::new(AtomicBool::new(false));
            let worker_updates = updates.clone();
            let worker_ready = ready.clone();
            let worker_wake = wake.clone();
            std::thread::spawn(move || {
                while !worker_ready.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(10));
                }
                for value in 1..=if smoke { 3 } else { 3600 } {
                    std::thread::sleep(Duration::from_millis(if smoke { 150 } else { 1000 }));
                    worker_updates.store(value, Ordering::Release);
                    if !worker_wake.request_redraw() {
                        break;
                    }
                }
            });
            let mut left = Counter {
                name: "left",
                value: 0,
            };
            let mut right = Counter {
                name: "right",
                value: 10,
            };
            let mut custom_count = 0;
            let mut selection = String::from("none");
            let mut last_update = u32::MAX;
            move |ui: &mut Ui<'_>| {
                let tick = updates.load(Ordering::Acquire);
                if smoke && tick != last_update {
                    println!("WAKE_UPDATE {tick}");
                }
                last_update = tick;
                ui.column("page", Style { padding: 28.0, gap: 16.0, width: Length::Percent(1.0), ..Style::default() }, |ui| {
                ui.label_styled("title", "Architecture workbench", Style { font_size: 28.0, ..Style::default() });
                ui.label_keyed("background", format!("Background updates: {tick}"));
                ui.label_keyed("instructions", "Wheel to scroll. Tab reveals focused rows. Arrow keys, Page Up/Down, Home/End scroll.");
                ui.row("components", Style { gap: 36.0, ..Style::default() }, |ui| {
                    ui.component("left", Style::default(), &mut left);
                    ui.component("right", Style::default(), &mut right);
                });
                if ui.custom("custom", Style::default(), custom_action(custom_count)).clicked() { custom_count += 1; }
                ui.button_styled("disabled", "Unavailable action", false, Style::button());
                ui.label_keyed("selection", format!("Selected row: {selection}"));
                ui.scroll("rows", Style {
                    width: Length::Percent(1.0), height: Length::Px(240.0),
                    padding: 8.0, gap: 4.0, background: Color::rgb(24, 35, 49), ..Style::default()
                }, |ui| {
                    for index in 0..250 {
                        if ui.button_styled(index.to_string(), format!("Row {index:03}"), true, Style {
                            width: Length::Percent(0.97), height: Length::Px(36.0), padding: 7.0, ..Style::button()
                        }).clicked() { selection = format!("{index:03}"); }
                    }
                });
                ui.label_styled("footer", "Native accessibility shares the same IDs and actions as pointer and keyboard input.", Style { font_size: 13.0, ..Style::default() });
            });
                ready.store(true, Ordering::Release);
                if smoke && tick == 3 {
                    println!("WAKE_SMOKE_OK");
                    wake.request_exit();
                }
            }
        },
    )?;
    if smoke {
        let handle = handle_receiver.recv()?;
        assert!(
            !handle.request_redraw(),
            "closed event loop must reject wakeups"
        );
        assert!(
            !handle.request_exit(),
            "closed event loop must reject exit requests"
        );
        println!("WAKE_CLOSED_OK");
    }
    Ok(())
}
