//! Search as you type over two hundred thousand records, on a thread. Every
//! keystroke supersedes the search the last one started, so the answer to an
//! older query can never overtake the current one, and the worker stops at
//! its next look rather than running to the end. Results and progress arrive
//! as a stream, a few frames' worth at a time.
use glassmere::{
    App, AppOptions, Button, Color, Effects, FontFamily, Label, Length, Size, Style, TextWrap, Ui,
    run,
};
use std::time::Duration;

const RECORDS: u32 = 200_000;
const WORDS: [&str; 8] = [
    "aurora", "basalt", "cinder", "delta", "ember", "fathom", "granite", "harbour",
];
/// Enough hits to fill any window; a one-letter query matches nearly all of
/// them, and nobody reads the rest.
const MAX_HITS: usize = 500;
const SEARCH: &str = "search";

/// What the worker streams back: how far it has gone, and what it found.
#[derive(Clone)]
enum Found {
    Scanned(u32),
    Hit(String),
}

#[derive(Clone)]
enum Msg {
    Query(String),
    Stop,
    Found(Found),
}

struct Search {
    smoke: bool,
    query: String,
    /// The query the running or last search was started for.
    searched: String,
    hits: Vec<String>,
    scanned: u32,
    running: bool,
    /// The last progress line `--smoke` printed.
    reported: String,
}

fn record(index: u32) -> String {
    format!("{index:06} · {}", WORDS[index as usize % WORDS.len()])
}

impl Search {
    /// Starts a search for the query, superseding the one running.
    fn start(&mut self, effects: &mut Effects<'_, Msg>) {
        self.searched = self.query.clone();
        self.hits.clear();
        self.scanned = 0;
        self.running = !self.searched.is_empty();
        if !self.running {
            effects.cancel(SEARCH);
            return;
        }
        let wanted = self.searched.clone();
        let _ = effects.spawn(
            SEARCH,
            move |out| {
                let mut sent = 0;
                for index in 0..RECORDS {
                    if index % 10_000 == 0 {
                        if out.cancelled() {
                            return;
                        }
                        let _ = out.send(Found::Scanned(index));
                        std::thread::sleep(Duration::from_millis(12));
                    }
                    let line = record(index);
                    if line.contains(&wanted) && sent < MAX_HITS {
                        sent += 1;
                        // Waits for room instead of dropping a hit: the
                        // application drains the channel every frame.
                        if out.send_blocking(Found::Hit(line)).is_err() {
                            return;
                        }
                    }
                }
                let _ = out.send(Found::Scanned(RECORDS));
            },
            Msg::Found,
        );
    }

    fn progress(&self) -> String {
        if self.searched.is_empty() {
            "Nothing to search for".to_owned()
        } else if self.running {
            format!(
                "Searching · {}% · {} hits",
                self.scanned * 100 / RECORDS,
                self.hits.len()
            )
        } else {
            format!("{} hits", self.hits.len())
        }
    }

    /// `--smoke` says on the terminal what the window shows, so an
    /// unattended run leaves evidence behind.
    fn report(&mut self) {
        if !self.smoke {
            return;
        }
        let progress = self.progress();
        if progress != self.reported {
            println!("SEARCH {progress}");
            self.reported = progress;
        }
    }
}

impl App for Search {
    type Message = Msg;

    fn init(&mut self, effects: &mut Effects<'_, Msg>) {
        if self.smoke {
            self.query = "0042".into();
        }
        self.start(effects);
        self.report();
    }

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Query(query) => {
                self.query = query;
                if self.query != self.searched {
                    self.start(effects);
                }
            }
            Msg::Stop => {
                effects.cancel(SEARCH);
                self.running = false;
            }
            Msg::Found(Found::Scanned(index)) => {
                self.scanned = index;
                if index == RECORDS {
                    self.running = false;
                }
            }
            Msg::Found(Found::Hit(line)) => self.hits.push(line),
        }
        self.report();
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        if self.smoke {
            ui.request_redraw();
        }
        let page = Style {
            padding: 24.0,
            gap: 14.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        ui.column(page, |ui| {
            ui.label(Label::new("Background work").style(Style {
                font_size: Some(28.0),
                font_weight: Some(700),
                ..Style::default()
            }));
            ui.label(
                Label::new(
                    "Type to search two hundred thousand records. Each keystroke starts a new \
                 search and stops the one before it.",
                )
                .style(Style {
                    width: Length::Percent(1.0),
                    wrap: TextWrap::Word,
                    ..Style::default()
                }),
            );
            ui.row(Style::default().gap(12.0), |ui| {
                ui.text_input("Search", &self.query, Msg::Query);
                ui.button(
                    Button::new("Stop")
                        .enabled(self.running)
                        .on_click(Msg::Stop),
                );
            });
            ui.label(Label::new(self.progress()).style(Style {
                foreground: Some(Color::rgb(103, 232, 206)),
                ..Style::default()
            }));
            let list = Style {
                height: Length::Px(360.0),
                width: Length::Percent(1.0),
                gap: 2.0,
                padding: 8.0,
                background: Some(Color::rgb(24, 30, 38)),
                radius: 8.0,
                ..Style::default()
            };
            ui.scroll(list, |ui| {
                for (index, hit) in self.hits.iter().enumerate() {
                    ui.keyed(format!("hit-{index}"), |ui| {
                        ui.label(Label::new(hit).style(Style {
                            font_family: Some(FontFamily::Monospace),
                            font_size: Some(13.0),
                            ..Style::default()
                        }))
                    });
                }
            });
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    run(
        AppOptions {
            title: "Glassmere / Background work".into(),
            size: Size::new(720.0, 620.0),
            frame_limit: smoke.then_some(90),
            ..AppOptions::default()
        },
        Search {
            smoke,
            query: String::new(),
            searched: String::new(),
            hits: Vec::new(),
            scanned: 0,
            running: false,
            reported: String::new(),
        },
    )?;
    Ok(())
}
