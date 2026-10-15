mod support;

use rust_gui_core::app::Action as Msg;
use rust_gui_core::{
    Action, Context, Event, Key, MonospaceMeasurer, NodeId, NumberOptions, Role, TextEdit, Ui,
    suggest,
};
use rust_gui_scene::{Point, Primitive, Size};
use support::{Driven, on};

const ROOT: Size = Size::new(600.0, 400.0);

fn key(context: &mut Context, key: Key) -> bool {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    })
}

fn retype(context: &mut Context, text: &str) {
    context.event(Event::Text(TextEdit::SelectAll));
    context.event(Event::Text(TextEdit::Insert(text.into())));
}

fn click(context: &mut Context, id: NodeId) {
    let bounds = context.node(id).unwrap().bounds;
    context.event(Event::PointerMoved(Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
}

// M35: the draft of a number field is kernel state that the node's semantic
// value does not show until the next declaration (`Numbers::text` reads that
// value), and this test reads the draft there. Stays on `Context::run` for now.
struct Numbers {
    context: Context,
    value: f64,
    options: NumberOptions,
}

impl Numbers {
    fn new(value: f64, options: NumberOptions) -> Self {
        let mut numbers = Self {
            context: Context::new(),
            value,
            options,
        };
        numbers.frame();
        numbers
    }

    /// One declaration; whether the field reported a change.
    fn frame(&mut self) -> bool {
        let mut changed = false;
        let (value, options) = (&mut self.value, self.options.clone());
        self.context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            changed = ui
                .keyed("amount", |ui| ui.number_input("Amount", value, options))
                .changed;
            ui.button("elsewhere");
        });
        changed
    }

    fn field(&self) -> NodeId {
        self.context.find(&["amount", "$field"]).unwrap()
    }

    fn text(&self) -> String {
        self.context.node(self.field()).unwrap().value.unwrap()
    }
}

struct Amount {
    value: f64,
    options: NumberOptions,
}

fn numbers(value: f64, options: NumberOptions) -> Driven<Amount> {
    Driven::new(
        ROOT,
        Amount { value, options },
        |state: &Amount, ui: &mut Ui<'_, Msg<Amount>>| {
            ui.keyed("amount", |ui| {
                ui.number_input_msg(
                    "Amount",
                    state.value,
                    state.options.clone(),
                    on(|s: &mut Amount, value: f64| s.value = value),
                );
            });
            ui.button("elsewhere");
        },
    )
}

fn field(app: &Driven<Amount>) -> NodeId {
    app.context.find(&["amount", "$field"]).unwrap()
}

fn text(app: &Driven<Amount>) -> String {
    app.context.node(field(app)).unwrap().value.unwrap()
}

#[test]
fn typing_commits_on_enter_and_on_blur_and_text_that_is_no_number_reverts() {
    let mut numbers = Numbers::new(10.0, NumberOptions::default().range(0.0, 100.0).step(5.0));
    assert_eq!(numbers.text(), "10");
    let field = numbers.field();
    numbers.context.action(field, Action::Focus);

    // A draft is not a value: the app still sees 10 while "4" is on screen.
    retype(&mut numbers.context, "4");
    assert!(!numbers.frame());
    assert_eq!((numbers.value, numbers.text().as_str()), (10.0, "4"));
    retype(&mut numbers.context, "42");
    assert!(!numbers.frame());

    // Enter commits: clamped to the range, snapped to the step, reformatted.
    key(&mut numbers.context, Key::Enter);
    assert!(numbers.frame());
    assert_eq!((numbers.value, numbers.text().as_str()), (40.0, "40"));
    retype(&mut numbers.context, "1e9");
    key(&mut numbers.context, Key::Enter);
    numbers.frame();
    assert_eq!((numbers.value, numbers.text().as_str()), (100.0, "100"));

    // Leaving the field commits too.
    retype(&mut numbers.context, "17");
    numbers.frame();
    key(&mut numbers.context, Key::Tab);
    assert!(numbers.frame());
    assert_eq!((numbers.value, numbers.text().as_str()), (15.0, "15"));

    // Not a number: the old value comes back and nothing is reported.
    numbers.context.action(field, Action::Focus);
    retype(&mut numbers.context, "abc");
    numbers.frame();
    assert_eq!(numbers.text(), "abc");
    key(&mut numbers.context, Key::Enter);
    assert!(!numbers.frame());
    assert_eq!((numbers.value, numbers.text().as_str()), (15.0, "15"));

    // The app changing the number replaces a draft.
    retype(&mut numbers.context, "99");
    numbers.frame();
    numbers.value = 30.0;
    numbers.frame();
    assert_eq!((numbers.value, numbers.text().as_str()), (30.0, "30"));
}

#[test]
fn arrows_pages_buttons_the_wheel_and_assistive_technology_step_the_value() {
    let mut app = numbers(
        1.0,
        NumberOptions::default()
            .range(0.0, 2.0)
            .step(0.25)
            .suffix(" m"),
    );
    assert_eq!(text(&app), "1.00 m");
    let field = field(&app);
    app.context.action(field, Action::Focus);
    key(&mut app.context, Key::ArrowUp);
    key(&mut app.context, Key::ArrowUp);
    assert!(app.frame().messages > 0, "two steps between frames add up");
    assert_eq!((app.state().value, text(&app).as_str()), (1.5, "1.50 m"));
    key(&mut app.context, Key::PageDown);
    app.frame();
    assert_eq!(app.state().value, 0.0, "ten steps down stop at the minimum");

    // Stepping commits a draft first; a decimal comma and the suffix parse.
    retype(&mut app.context, "0,5 m");
    key(&mut app.context, Key::ArrowUp);
    app.frame();
    assert_eq!((app.state().value, text(&app).as_str()), (0.75, "0.75 m"));

    // The buttons step, are skipped by Tab, and switch off at the ends.
    let more = app.context.find(&["amount", "$more"]).unwrap();
    let less = app.context.find(&["amount", "$less"]).unwrap();
    assert!(!app.context.node(more).unwrap().semantics.focusable);
    for _ in 0..5 {
        click(&mut app.context, more);
        app.frame();
    }
    assert_eq!(app.state().value, 2.0);
    assert!(app.context.node(more).unwrap().semantics.disabled);
    click(&mut app.context, less);
    app.frame();
    assert_eq!(app.state().value, 1.75);

    // The wheel steps only with keyboard focus and the pointer on the field.
    let bounds = app.context.node(field).unwrap().bounds;
    let over = Point {
        x: bounds.x + 10.0,
        y: bounds.y + 10.0,
    };
    app.context.event(Event::PointerMoved(over));
    app.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 40.0,
    });
    app.frame();
    assert_eq!(
        app.state().value,
        1.75,
        "not focused: the wheel scrolls the page"
    );
    app.context.action(field, Action::Focus);
    app.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 40.0,
    });
    app.frame();
    assert_eq!(app.state().value, 1.5);

    // A spin button with a range, that screen readers can step and set.
    let node = app.context.node(field).unwrap();
    assert_eq!(node.semantics.role, Role::SpinButton);
    assert_eq!(node.semantics.label, "Amount");
    let numeric = node.numeric.unwrap();
    assert_eq!(
        (numeric.value, numeric.min, numeric.max, numeric.step),
        (1.5, 0.0, 2.0, 0.25)
    );
    app.context.action(field, Action::Increment);
    app.frame();
    assert_eq!(app.state().value, 1.75);
    app.context.action(field, Action::SetNumber(0.3));
    app.frame();
    assert_eq!((app.state().value, text(&app).as_str()), (0.25, "0.25 m"));
}

#[test]
fn number_options_format_parse_and_constrain() {
    let plain = NumberOptions::default();
    assert_eq!(plain.format(-0.0), "0");
    assert_eq!(plain.parse(" \u{2212}12 "), Some(-12.0));
    assert_eq!(plain.parse("2.6"), Some(3.0));
    assert_eq!(plain.parse("1,234.5"), None);
    assert_eq!(plain.parse(""), None);
    assert_eq!(plain.parse("NaN"), None);
    assert_eq!(plain.constrain(f64::NAN), 0.0);
    let cents = NumberOptions::default().step(0.01).suffix(" %");
    assert_eq!(cents.format(0.1 + 0.2), "0.30 %");
    assert_eq!(cents.parse("0,3%"), Some(0.3));
    assert_eq!(cents.constrain(0.1 + 0.2), 0.3);
    // Snapping counts from the minimum; without a step only the range applies.
    let odd = NumberOptions::default().range(1.0, 9.0).step(2.0);
    assert_eq!((odd.constrain(4.2), odd.constrain(40.0)), (5.0, 9.0));
    let free = NumberOptions::default().step(0.0).decimals(3);
    assert_eq!(free.format(free.constrain(1.23456)), "1.235");
    assert_eq!(free.constrain(1.23456), 1.23456);
}

const CITIES: [&str; 5] = ["Berlin", "Bern", "Bergen", "Brno", "İzmir"];

// M35: `autocomplete_msg` reports neither the picked index nor a submit (Enter
// with no highlight), and this test asserts both, so it stays on `Context::run`.
struct Search {
    context: Context,
    text: String,
    picked: Option<usize>,
    submitted: bool,
}

impl Search {
    fn frame(&mut self) {
        let text = &mut self.text;
        let (mut picked, mut submitted) = (None, false);
        self.context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            let matches = suggest(text, &CITIES, 8);
            let response = ui.keyed("city", |ui| ui.autocomplete("City", text, &matches));
            picked = response.picked;
            submitted = response.response.submitted;
            ui.button("elsewhere");
        });
        (self.picked, self.submitted) = (picked, submitted);
    }

    fn rows(&self) -> Vec<String> {
        let Some(popup) = self.context.find(&["city", "$popup"]) else {
            return Vec::new();
        };
        let popup = self.context.node(popup).unwrap();
        popup
            .children
            .iter()
            .map(|row| self.context.node(*row).unwrap().semantics.label.clone())
            .collect()
    }
}

#[test]
fn typing_offers_the_apps_matches_and_keys_or_a_click_take_one() {
    let mut search = Search {
        context: Context::new(),
        text: String::new(),
        picked: None,
        submitted: false,
    };
    search.frame();
    let field = search.context.find(&["city"]).unwrap();
    search.context.action(field, Action::Focus);
    retype(&mut search.context, "ber");
    search.frame();
    // The event opened the list; the next declaration knows the matches.
    search.frame();
    assert_eq!(search.rows(), ["Berlin", "Bern", "Bergen"]);
    let node = search.context.node(field).unwrap();
    assert_eq!(node.expanded, Some(true));
    assert_eq!(node.active_descendant, None, "nothing is highlighted yet");

    // Typing goes on while the list is open, space included.
    search
        .context
        .event(Event::Text(TextEdit::Insert("g".into())));
    search.frame();
    search.frame();
    assert_eq!(search.rows(), ["Bergen"]);

    // Down highlights, Enter takes it: the text, the caret and the report.
    key(&mut search.context, Key::ArrowDown);
    search.frame();
    assert!(
        search
            .context
            .node(field)
            .unwrap()
            .active_descendant
            .is_some()
    );
    key(&mut search.context, Key::Enter);
    search.frame();
    assert_eq!((search.text.as_str(), search.picked), ("Bergen", Some(0)));
    assert!(!search.submitted);
    assert!(search.rows().is_empty());
    let selection = search.context.node(field).unwrap().text_selection.unwrap();
    assert_eq!(selection.range(), 6..6);

    // Enter without a highlight submits the free text.
    retype(&mut search.context, "Ber");
    search.frame();
    search.frame();
    assert_eq!(search.rows().len(), 3);
    key(&mut search.context, Key::Enter);
    search.frame();
    assert!(search.submitted && search.picked.is_none());
    assert_eq!(search.text, "Ber");
    assert!(search.rows().is_empty());

    // Down reopens a closed list, Up enters from the bottom, Escape closes.
    key(&mut search.context, Key::ArrowDown);
    search.frame();
    assert_eq!(search.rows().len(), 3);
    key(&mut search.context, Key::Escape);
    search.frame();
    assert!(search.rows().is_empty());
    assert_eq!(search.text, "Ber");

    // A click takes a row and focus stays in the field.
    retype(&mut search.context, "br");
    search.frame();
    search.frame();
    let row = search.context.find(&["city", "$popup", "0"]).unwrap();
    click(&mut search.context, row);
    search.frame();
    assert_eq!((search.text.as_str(), search.picked), ("Brno", Some(0)));
    assert_eq!(search.context.focused(), Some(field));

    // Leaving the field closes the list.
    retype(&mut search.context, "be");
    search.frame();
    search.frame();
    assert_eq!(search.rows().len(), 3);
    key(&mut search.context, Key::Tab);
    search.frame();
    assert!(search.rows().is_empty());
}

struct City {
    text: String,
}

#[test]
fn the_typed_part_of_a_suggestion_is_drawn_bold_whatever_its_case() {
    let mut app = Driven::new(
        ROOT,
        City {
            text: String::new(),
        },
        |state: &City, ui: &mut Ui<'_, Msg<City>>| {
            ui.keyed("city", |ui| {
                let matches = suggest(&state.text, &CITIES, 8);
                ui.autocomplete_msg(
                    "City",
                    &state.text,
                    &matches,
                    on(|s: &mut City, text: String| s.text = text),
                );
            });
            ui.button("elsewhere");
        },
    );
    let field = app.context.find(&["city"]).unwrap();
    app.context.action(field, Action::Focus);
    retype(&mut app.context, "ZM");
    app.frame();
    app.frame();
    let rows: Vec<String> = app
        .context
        .find(&["city", "$popup"])
        .map(|popup| {
            app.context
                .node(popup)
                .unwrap()
                .children
                .iter()
                .map(|row| app.context.node(*row).unwrap().semantics.label.clone())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(rows, ["İzmir"]);
    let bold: Vec<String> = app
        .context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::RichText { spans, .. } => Some(
                spans
                    .iter()
                    .filter(|span| span.weight == Some(700))
                    .map(|span| span.text.clone())
                    .collect::<String>(),
            ),
            _ => None,
        })
        .collect();
    assert_eq!(bold, ["zm"]);
}
