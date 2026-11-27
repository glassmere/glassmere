//! A view that is skipped. `view_state` promises that the view reads
//! nothing but what it returns and the sources of its bound labels, so a
//! click that only changes a signal does not run the view: the label's text
//! is replaced in the retained tree and nothing else is declared.
//!
//! The "view ran" line counts declarations. Click Add and watch the count
//! stay where it is while the total moves; the second row of buttons
//! changes what the view lists, which does run it.
use glassmere::{
    App, AppOptions, Button, Effects, Label, Memo, Signal, Size, Store, Style, Ui, run,
};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone)]
enum Msg {
    Add,
    Reset,
    MoreRows,
    FewerRows,
    /// Sent once at startup under `--smoke`: click Add three times.
    Smoke,
}

struct Tally {
    count: Signal<u32>,
    total: Memo<String>,
    views: Rc<Cell<u32>>,
    /// What the view lists, so it is what `view_state` covers.
    rows: u32,
    smoke: bool,
}

impl Tally {
    fn new(smoke: bool) -> Self {
        let store = Store::new();
        let count = store.signal("count", 0u32);
        let read = count.clone();
        let total = store.memo("total", move || format!("Total: {}", read.get()));
        Self {
            count,
            total,
            views: Rc::new(Cell::new(0)),
            rows: 3,
            smoke,
        }
    }
}

impl App for Tally {
    type Message = Msg;

    fn init(&mut self, effects: &mut Effects<'_, Msg>) {
        if self.smoke {
            let start = effects.channel(1, |()| Msg::Smoke);
            let _ = start.try_send(());
        }
    }

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Add => self.count.update(|count| *count += 1),
            Msg::Reset => self.count.set(0),
            Msg::MoreRows => self.rows += 1,
            Msg::FewerRows => self.rows = self.rows.saturating_sub(1),
            Msg::Smoke => {
                for _ in 0..3 {
                    self.count.update(|count| *count += 1);
                }
                println!(
                    "{} (view ran {} time(s))",
                    self.total.get(),
                    self.views.get()
                );
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        self.views.set(self.views.get() + 1);
        ui.column(Style::default().padding(24.0).gap(12.0), |ui| {
            ui.keyed("total", |ui| ui.bound_label(&self.total));
            ui.row(Style::default().gap(8.0), |ui| {
                ui.keyed("add", |ui| ui.button(Button::new("Add").on_click(Msg::Add)));
                ui.keyed("reset", |ui| {
                    ui.button(Button::new("Reset").on_click(Msg::Reset))
                });
            });
            ui.label(Label::new(format!("View ran {} time(s)", self.views.get())));
            ui.row(Style::default().gap(8.0), |ui| {
                ui.keyed("more", |ui| {
                    ui.button(Button::new("More rows").on_click(Msg::MoreRows))
                });
                ui.keyed("fewer", |ui| {
                    ui.button(Button::new("Fewer rows").on_click(Msg::FewerRows))
                });
            });
            for row in 0..self.rows {
                ui.keyed(format!("row-{row}"), |ui| {
                    ui.label(format!("Row {}", row + 1))
                });
            }
        });
    }

    /// The view reads the number of rows, and the rest through its bound
    /// label. The line that shows how many times it ran is only as current
    /// as the last time it did.
    fn view_state(&self) -> Option<u64> {
        Some(u64::from(self.rows))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    run(
        AppOptions {
            title: "Glassmere / Bound labels".into(),
            size: Size::new(420.0, 360.0),
            frame_limit: smoke.then_some(20),
            ..AppOptions::default()
        },
        Tally::new(smoke),
    )
}
