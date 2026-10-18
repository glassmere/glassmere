use glassmere::{
    App, AppOptions, Button, Color, Component, CustomWidget, Effects, Label, Length, MessageSender,
    Primitive, Rect, Role, Semantics, Size, Style, Ui, UiEvent, WidgetBuilder, WidgetInput,
    WindowCommand, run,
};
use std::{sync::mpsc::Sender, time::Duration};

struct Counter {
    name: &'static str,
    value: u32,
}

#[derive(Clone)]
struct Increase;

impl Component for Counter {
    type Message = Increase;

    fn update(&mut self, _: Increase, _: &mut Effects<'_, Increase>) {
        self.value += 1;
    }

    fn view(&self, ui: &mut Ui<'_, Increase>) {
        ui.label(format!("{} count: {}", self.name, self.value));
        ui.button(Button::new(format!("Increase {}", self.name)).on_click(Increase));
    }
}

#[derive(Clone)]
enum Message {
    Left(Increase),
    Right(Increase),
    Select(usize),
    Custom,
    Tick(u32),
}

struct Workbench {
    smoke: bool,
    /// Hands the worker's channel to `main`, which checks that it is turned
    /// away once the event loop has closed.
    closed_probe: Sender<MessageSender<u32>>,
    left: Counter,
    right: Counter,
    custom_count: u32,
    selection: String,
    tick: u32,
}

impl App for Workbench {
    type Message = Message;

    fn init(&mut self, effects: &mut Effects<'_, Message>) {
        if self.smoke {
            println!("WAKE_UPDATE {}", self.tick);
        }
        let sender = effects.channel(16, Message::Tick);
        self.closed_probe.send(sender.clone()).unwrap();
        let smoke = self.smoke;
        std::thread::spawn(move || {
            for value in 1..=if smoke { 3 } else { 3600 } {
                std::thread::sleep(Duration::from_millis(if smoke { 150 } else { 1000 }));
                if sender.send_blocking(value).is_err() {
                    break;
                }
            }
        });
    }

    fn update(&mut self, message: Message, effects: &mut Effects<'_, Message>) {
        match message {
            Message::Left(message) => self.left.update(message, &mut effects.map(Message::Left)),
            Message::Right(message) => self.right.update(message, &mut effects.map(Message::Right)),
            Message::Select(index) => self.selection = format!("{index:03}"),
            Message::Custom => self.custom_count += 1,
            Message::Tick(tick) => {
                self.tick = tick;
                if self.smoke {
                    println!("WAKE_UPDATE {tick}");
                    if tick == 3 {
                        println!("WAKE_SMOKE_OK");
                        effects.window_command(WindowCommand::Close);
                    }
                }
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Message>) {
        let tick = self.tick;
        let page = Style {
            padding: 28.0,
            gap: 16.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        let rows = Style {
            width: Length::Percent(1.0),
            height: Length::Px(240.0),
            padding: 8.0,
            gap: 4.0,
            background: Some(Color::rgb(24, 35, 49)),
            ..Style::default()
        };
        let row_button = Style {
            width: Length::Percent(0.97),
            height: Length::Px(36.0),
            padding: 7.0,
            ..Style::button()
        };
        ui.column(page, |ui| {
            ui.label(Label::new("Architecture workbench").style(Style {
                font_size: Some(28.0),
                ..Style::default()
            }));
            ui.label(format!("Background updates: {tick}"));
            ui.label(
                "Wheel to scroll. Tab reveals focused rows. Arrow keys, Page Up/Down, Home/End scroll.",
            );
            ui.row(Style::default().gap(36.0), |ui| {
                ui.component(Style::default(), &self.left, Message::Left);
                ui.component(Style::default(), &self.right, Message::Right);
            });
            let custom_count = self.custom_count;
            ui.widget(
                Style::default(),
                WidgetBuilder::new(move |_: &WidgetInput| custom_action(custom_count)).on_event(
                    |event| match event {
                        UiEvent::Activate => Some(Message::Custom),
                        // A pointer click arrives as its release over the widget.
                        UiEvent::PointerUp(at)
                            if (0.0..230.0).contains(&at.x) && (0.0..44.0).contains(&at.y) =>
                        {
                            Some(Message::Custom)
                        }
                        _ => None,
                    },
                ),
            );
            ui.button(Button::new("Unavailable action").enabled(false));
            ui.label(format!("Selected row: {}", self.selection));
            ui.scroll(rows, |ui| {
                for index in 0..250 {
                    ui.keyed(index.to_string(), |ui| {
                        ui.button(
                            Button::new(format!("Row {index:03}"))
                                .style(row_button.clone())
                                .on_click(Message::Select(index)),
                        )
                    });
                }
            });
            ui.label(
                Label::new(
                    "Native accessibility shares the same IDs and actions as pointer and keyboard input.",
                )
                .style(Style {
                    font_size: Some(13.0),
                    ..Style::default()
                }),
            );
        });
    }
}

fn custom_action(count: u32) -> CustomWidget {
    CustomWidget {
        size: Size::new(230.0, 44.0),
        semantics: Semantics {
            role: Role::Button,
            label: format!("Custom action: {count}"),
            focusable: true,
            ..Semantics::default()
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
        ..CustomWidget::default()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let (closed_probe, probe) = std::sync::mpsc::channel();
    run(
        AppOptions {
            title: "Glassmere / Architecture workbench".into(),
            size: Size::new(920.0, 820.0),
            ..AppOptions::default()
        },
        Workbench {
            smoke,
            closed_probe,
            left: Counter {
                name: "left",
                value: 0,
            },
            right: Counter {
                name: "right",
                value: 10,
            },
            custom_count: 0,
            selection: String::from("none"),
            tick: 0,
        },
    )?;
    if smoke {
        let sender = probe.recv()?;
        assert!(!sender.wake(), "closed event loop must reject wakeups");
        println!("WAKE_CLOSED_OK");
    }
    Ok(())
}
