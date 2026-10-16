use rust_gui::{App, AppOptions, Button, Color, Effects, Label, Length, Style, Ui, run};

const MOTD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/counter-motd.txt");

#[derive(Clone)]
enum Msg {
    Increase,
    Decrease,
    Reset,
    ToggleDetails,
    Reorder,
    /// Sent from a ticking thread under `--stress`: one per frame, as the
    /// stress run was when the view advanced the state itself.
    Tick,
}

struct Counter {
    count: i32,
    show_details: bool,
    reversed: bool,
    hook: bool,
    stress: bool,
}

impl Counter {
    /// Under `rust-gui-dev` the previous run's state comes back after a rebuild.
    fn restore(saved: &str, hook: bool, stress: bool) -> Self {
        let mut fields = saved.split_whitespace();
        Self {
            count: fields
                .next()
                .and_then(|field| field.parse().ok())
                .unwrap_or(0),
            show_details: fields.next() != Some("false"),
            reversed: fields.next() == Some("true"),
            hook,
            stress,
        }
    }
}

impl App for Counter {
    type Message = Msg;

    fn init(&mut self, effects: &mut Effects<'_, Msg>) {
        if !self.stress {
            return;
        }
        // The thread waits for the mailbox to drain, so it sends about one
        // tick per frame and never floods the event loop.
        let ticks = effects.channel(1, |()| Msg::Tick);
        std::thread::spawn(move || while ticks.send_blocking(()).is_ok() {});
    }

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Increase => self.count = self.count.saturating_add(1),
            Msg::Decrease => self.count = self.count.saturating_sub(1),
            Msg::Reset => self.count = 0,
            Msg::ToggleDetails => self.show_details = !self.show_details,
            Msg::Reorder => self.reversed = !self.reversed,
            Msg::Tick => {
                self.count += 1;
                self.show_details = self.count % 60 < 30;
                self.reversed = (self.count / 30) % 2 == 0;
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        if self.hook {
            // A leaf that says "the application draws this part".
            ui.widget(
                rust_gui::Style::default(),
                rust_gui::WidgetBuilder::new(|_| rust_gui::CustomWidget {
                    size: rust_gui::Size::new(120.0, 60.0),
                    semantics: rust_gui::Semantics {
                        label: "Application drawing".into(),
                        ..rust_gui::Semantics::default()
                    },
                    primitives: vec![rust_gui::Primitive::Custom {
                        rect: rust_gui::Rect {
                            x: 0.0,
                            y: 0.0,
                            width: 120.0,
                            height: 60.0,
                        },
                        id: 7,
                    }],
                    ..rust_gui::CustomWidget::default()
                }),
            );
        }
        let message = ui.watched_text(MOTD);
        ui.column(Style { padding: 36.0, gap: 24.0, width: Length::Percent(1.0), ..Style::default() }, |ui| {
            ui.label(Label::new("RUST GUI  /  FIRST MILESTONE").style(Style { font_size: Some(12.0), foreground: Some(Color::rgb(103, 232, 206)), ..Style::default() }));
            ui.label(Label::new("A small API. A retained engine.").style(Style { font_size: Some(30.0), ..Style::default() }));
            ui.label("Native window, stable identity, flex layout and GPU rendering.");
            ui.label(Label::new(message.as_deref().unwrap_or("").trim_end()).style(Style { font_size: Some(13.0), foreground: Some(Color::rgb(153, 171, 193)), ..Style::default() }));
            ui.container(Style { padding: 24.0, gap: 18.0, radius: 14.0, background: Some(Color::rgb(24, 35, 49)), width: Length::Percent(1.0), ..Style::default() }, |ui| {
                ui.label(Label::new(format!("Counter   {}", self.count)).style(Style { font_size: Some(36.0), ..Style::default() }));
                ui.row(Style::default(), |ui| {
                    ui.button(Button::new("- Decrease").on_click(Msg::Decrease));
                    ui.button(Button::new("+ Increase").style(Style { background: Some(Color::rgb(22, 105, 99)), ..Style::button() }).on_click(Msg::Increase));
                    ui.button(Button::new("Reset").enabled(self.count != 0).on_click(Msg::Reset));
                });
                ui.label(Label::new("Tab / Shift+Tab to focus. Enter or Space to activate.").style(Style { font_size: Some(13.0), foreground: Some(Color::rgb(153, 171, 193)), ..Style::default() }));
            });
            ui.row(Style::default(), |ui| {
                ui.button(Button::new(if self.show_details { "Hide details" } else { "Show details" }).on_click(Msg::ToggleDetails));
                ui.button(Button::new("Reorder keyed items").on_click(Msg::Reorder));
            });
            if self.show_details {
                ui.column(Style { gap: 8.0, ..Style::default() }, |ui| {
                    let items = if self.reversed { [("paint", "03  Scene commands are independent of widgets."), ("layout", "02  Unchanged declarations skip layout and paint."), ("identity", "01  Keys preserve identity when siblings move.")] }
                        else { [("identity", "01  Keys preserve identity when siblings move."), ("layout", "02  Unchanged declarations skip layout and paint."), ("paint", "03  Scene commands are independent of widgets.")] };
                    for (key, text) in items { ui.keyed(key, |ui| ui.label(text)); }
                });
            }
            ui.label(Label::new("Kernel prototype / text editing, scrolling and native accessibility are next.").style(Style { font_size: Some(12.0), foreground: Some(Color::rgb(127, 145, 166)), ..Style::default() }));
        });
    }

    fn save_state(&self) -> Option<String> {
        Some(format!(
            "{} {} {}",
            self.count, self.show_details, self.reversed
        ))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    // `--gpu-hook` draws one rectangle with the application's own wgpu code,
    // through `AppOptions::gpu` and `Primitive::Custom`.
    let hook = std::env::args().any(|arg| arg == "--gpu-hook");
    let drawn = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = drawn.clone();
    let stress = std::env::args().any(|arg| arg == "--stress");
    let arguments: Vec<_> = std::env::args().collect();
    let stress_frames = arguments
        .windows(2)
        .find(|pair| pair[0] == "--frames")
        .map(|pair| pair[1].parse::<u64>())
        .transpose()?
        .unwrap_or(600);
    let saved = rust_gui::hot::saved_state().unwrap_or_default();
    let app = Counter::restore(&saved, hook, stress);
    run(
        AppOptions {
            title: "Rust GUI / Kernel playground".into(),
            gpu: hook.then(|| {
                rust_gui::GpuSetup::new(move |renderer| {
                    let counted = counted.clone();
                    renderer.set_custom_draw(
                        7,
                        Some(Box::new(move |draw| {
                            // A real callback would draw here; this one only
                            // reports that it was asked to, and where.
                            let seen = counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if seen == 0 {
                                println!(
                                    "GPU_HOOK rect={:?} scale={} target={:?}",
                                    draw.rect, draw.scale, draw.target_size
                                );
                            }
                        })),
                    );
                })
            }),
            // `--centered` opens it in the middle of the screen, or of the
            // one named after `--screen`.
            placement: match std::env::args().position(|arg| arg == "--screen") {
                Some(at) => {
                    rust_gui::Placement::Named(std::env::args().nth(at + 1).unwrap_or_default())
                }
                None if std::env::args().any(|arg| arg == "--centered") => {
                    rust_gui::Placement::Centered
                }
                None => rust_gui::Placement::default(),
            },
            frame_limit: if stress {
                Some(stress_frames)
            } else {
                smoke.then_some(2)
            },
            ..AppOptions::default()
        },
        app,
    )
}
