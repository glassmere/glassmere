//! A view written in `markup.glass` instead of Rust. Edit that file while
//! this runs and the window follows; `glassmere-designer` edits it visually.
use glassmere::markup::{LiveMarkup, Markup, MarkupEvent, Value, Values};
use glassmere::{App, AppOptions, Effects, Ui, run};

const VIEW: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/markup.glass");

struct Example {
    view: LiveMarkup,
    values: Values,
}

impl App for Example {
    type Message = MarkupEvent;

    fn update(&mut self, event: MarkupEvent, _effects: &mut Effects<'_, MarkupEvent>) {
        // Widgets with `bind` store their own value.
        if self.values.apply(&event) {
            return;
        }
        let count = self
            .values
            .get("count")
            .and_then(Value::number)
            .unwrap_or(0.0);
        match event.name.as_str() {
            "increase" => self.values.set("count", count + 1.0),
            "decrease" => self.values.set("count", count - 1.0),
            "reset" => self.values.set("count", 0),
            _ => return,
        };
    }

    fn view(&self, ui: &mut Ui<'_, MarkupEvent>) {
        self.view.show(ui, &self.values, |event| event);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    // The file's `data` block is what the application starts from as well.
    let values = Markup::parse(include_str!("markup.glass"))?
        .defaults()
        .clone();
    run(
        AppOptions {
            title: "Glassmere / Markup".into(),
            frame_limit: smoke.then_some(2),
            ..AppOptions::default()
        },
        Example {
            view: LiveMarkup::new(VIEW),
            values,
        },
    )
}
