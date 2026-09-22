//! Search as you type over two hundred thousand records, on a thread. Every
//! keystroke supersedes the search the last one started, so the answer to an
//! older query can never overtake the current one, and the worker stops at
//! its next look rather than running to the end. Results and progress arrive
//! as a stream, a few frames' worth at a time.
use rust_gui::{AppOptions, Color, FontFamily, Length, Size, Style, TextWrap, run_with_wakeup};
use std::time::Duration;

const RECORDS: u32 = 200_000;
const WORDS: [&str; 8] = [
    "aurora", "basalt", "cinder", "delta", "ember", "fathom", "granite", "harbour",
];
/// Enough hits to fill any window; a one-letter query matches nearly all of
/// them, and nobody reads the rest.
const MAX_HITS: usize = 500;

/// What the worker streams back: how far it has gone, and what it found.
enum Found {
    Scanned(u32),
    Hit(String),
}

fn record(index: u32) -> String {
    format!("{index:06} · {}", WORDS[index as usize % WORDS.len()])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    run_with_wakeup(
        AppOptions {
            title: "Rust GUI / Background work".into(),
            size: Size::new(720.0, 620.0),
            frame_limit: smoke.then_some(90),
            ..AppOptions::default()
        },
        move |wake| {
            let mut tasks = wake.tasks::<&'static str, Found>(256);
            let mut query = String::new();
            let mut searched = String::new();
            let mut hits: Vec<String> = Vec::new();
            let mut scanned = 0;
            let mut first = true;
            let mut reported = String::new();
            move |ui| {
                if smoke {
                    ui.request_redraw();
                    if first {
                        query = "0042".into();
                    }
                }
                // A new query supersedes the running search. Nothing has to be
                // undone: what the old worker had already sent is dropped when
                // drained, because the view no longer listens to it.
                if query != searched || first {
                    searched = query.clone();
                    hits.clear();
                    scanned = 0;
                    first = false;
                    if !searched.is_empty() {
                        let wanted = searched.clone();
                        tasks.spawn("search", move |out| {
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
                                    // Waits for room instead of dropping a
                                    // hit: the view drains every frame.
                                    if out.send_blocking(Found::Hit(line)).is_err() {
                                        return;
                                    }
                                }
                            }
                            let _ = out.send(Found::Scanned(RECORDS));
                        });
                    }
                }
                for (_, message) in tasks.drain(128) {
                    match message {
                        Found::Scanned(index) => scanned = index,
                        Found::Hit(line) => hits.push(line),
                    }
                }
                let running = tasks.running(&"search");
                let page = Style {
                    padding: 24.0,
                    gap: 14.0,
                    width: Length::Percent(1.0),
                    ..Style::default()
                };
                ui.column("page", page, |ui| {
                    ui.label_styled(
                        "title",
                        "Background work",
                        Style {
                            font_size: Some(28.0),
                            font_weight: Some(700),
                            ..Style::default()
                        },
                    );
                    ui.label_styled(
                        "intro",
                        "Type to search two hundred thousand records. Each keystroke starts a new \
                         search and stops the one before it.",
                        Style {
                            width: Length::Percent(1.0),
                            wrap: TextWrap::Word,
                            ..Style::default()
                        },
                    );
                    ui.row("controls", Style::default().gap(12.0), |ui| {
                        ui.text_input("query", "Search", &mut query);
                        if ui
                            .button_styled("stop", "Stop", running, Style::button())
                            .clicked()
                        {
                            tasks.cancel(&"search");
                        }
                    });
                    let progress = if searched.is_empty() {
                        "Nothing to search for".to_owned()
                    } else if running {
                        format!(
                            "Searching · {}% · {} hits",
                            scanned * 100 / RECORDS,
                            hits.len()
                        )
                    } else {
                        format!("{} hits", hits.len())
                    };
                    // `--smoke` says on the terminal what the window shows,
                    // so an unattended run leaves evidence behind.
                    if smoke && progress != reported {
                        reported = progress.clone();
                        println!("SEARCH {reported}");
                    }
                    ui.label_styled(
                        "progress",
                        progress,
                        Style {
                            foreground: Some(Color::rgb(103, 232, 206)),
                            ..Style::default()
                        },
                    );
                    let list = Style {
                        height: Length::Px(360.0),
                        width: Length::Percent(1.0),
                        gap: 2.0,
                        padding: 8.0,
                        background: Some(Color::rgb(24, 30, 38)),
                        radius: 8.0,
                        ..Style::default()
                    };
                    ui.scroll("hits", list, |ui| {
                        for (index, hit) in hits.iter().enumerate() {
                            ui.label_styled(
                                format!("hit-{index}"),
                                hit,
                                Style {
                                    font_family: Some(FontFamily::Monospace),
                                    font_size: Some(13.0),
                                    ..Style::default()
                                },
                            );
                        }
                    });
                });
            }
        },
    )
}
