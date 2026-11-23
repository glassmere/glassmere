use glassmere_core::{App, Effects, Ui};
use glassmere_markup::{Markup, MarkupEvent, Node, Value, Values};
use glassmere_scene::{Point, Size};
use glassmere_testing::Harness;

const COUNTER: &str = r##"
// A counter, with the values it is previewed with.
data { count: 2, name: "Ada", agreed: false, items: [{ title: "One" }, { title: "Two" }] }

component Row(title, position) {
    label "{position}. {title}"
}

column { padding: 12; gap: 8
    label.title "Hello, {name}"
    label "Clicked {count} times"
    row { gap: 8
        button "Decrease" { on-click: decrease; enabled: count > 0 }
        button "Increase" { on-click: add(5, name); background: #16695f }
    }
    input "Name" { bind: name }
    checkbox "Agree" { bind: agreed }
    if agreed { label "Thank you." } else if count > 100 { label "Much." } else { label "Please agree." }
    for item, index in items {
        Row { title: item.title; position: index + 1 }
    }
}
"##;

struct Counter {
    view: Markup,
    values: Values,
    events: Vec<MarkupEvent>,
}

impl Counter {
    fn new(source: &str) -> Self {
        let view = Markup::parse(source).unwrap();
        Self {
            values: view.defaults().clone(),
            view,
            events: Vec::new(),
        }
    }
}

impl App for Counter {
    type Message = MarkupEvent;

    fn update(&mut self, event: MarkupEvent, _effects: &mut Effects<'_, MarkupEvent>) {
        if self.values.apply(&event) {
            return;
        }
        let count = self
            .values
            .get("count")
            .and_then(Value::number)
            .unwrap_or(0.0);
        match event.name.as_str() {
            "decrease" => self.values.set("count", count - 1.0),
            "add" => self
                .values
                .set("count", count + event.args[0].number().unwrap()),
            _ => &mut self.values,
        };
        self.events.push(event);
    }

    fn view(&self, ui: &mut Ui<'_, MarkupEvent>) {
        self.view.show(ui, &self.values, |event| event);
    }
}

fn harness(source: &str) -> Harness<Counter> {
    Harness::app(Size::new(480.0, 640.0), Counter::new(source))
}

#[test]
fn a_view_shows_its_values_and_sends_its_events() {
    let mut harness = harness(COUNTER);
    harness.by_label("Hello, Ada");
    harness.by_label("Clicked 2 times");
    harness.by_label("1. One");
    harness.by_label("2. Two");
    harness.by_label("Please agree.");

    let increase = harness.by_label("Increase");
    harness.click(increase);
    harness.by_label("Clicked 7 times");
    let event = harness.state().events.last().unwrap().clone();
    assert_eq!(event.name, "add");
    assert_eq!(event.args, [Value::Number(5.0), Value::Text("Ada".into())]);

    let decrease = harness.by_label("Decrease");
    for _ in 0..7 {
        harness.click(decrease);
    }
    harness.by_label("Clicked 0 times");
    // Disabled at zero: another click changes nothing.
    harness.click(decrease);
    harness.by_label("Clicked 0 times");
}

#[test]
fn bound_widgets_write_back_through_apply() {
    let mut harness = harness(COUNTER);
    let agree = harness.by_label("Agree");
    harness.click(agree);
    harness.by_label("Thank you.");
    assert_eq!(
        harness.state().values.get("agreed"),
        Some(&Value::Bool(true))
    );

    let name = harness.by_label("Name");
    harness.focus(name);
    harness.type_text("!");
    assert_eq!(
        harness.state().values.get("name"),
        Some(&Value::Text("Ada!".into()))
    );
    harness.by_label("Hello, Ada!");
    // Bound changes are stored, not handed to the application to handle.
    assert!(harness.state().events.is_empty());
}

#[test]
fn widgets_keep_their_identity_when_a_condition_changes_their_siblings() {
    let mut harness = harness(COUNTER);
    let before = harness.by_label("Increase");
    let agree = harness.by_label("Agree");
    harness.click(agree);
    assert_eq!(harness.by_label("Increase"), before);
    assert_eq!(harness.by_label("Agree"), agree);
}

#[test]
fn a_point_on_screen_leads_back_to_its_element() {
    let harness = harness(COUNTER);
    let increase = harness.by_label("Increase");
    let bounds = harness.context().node(increase).unwrap().bounds;
    let picked = Markup::pick(
        harness.context(),
        Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        },
    )
    .unwrap();
    let element = harness.state().view.element(picked).unwrap();
    assert_eq!(element.kind, "button");
    assert_eq!(element.text.as_ref().unwrap().raw, "Increase");
    assert_eq!(&COUNTER[element.span.clone()][..17], "button \"Increase\"");
    let background = element
        .properties
        .iter()
        .find(|property| property.name == "background")
        .unwrap();
    assert_eq!(&COUNTER[background.value_span.clone()], "#16695f");
}

#[test]
fn a_file_says_which_values_it_reads() {
    let view = Markup::parse(COUNTER).unwrap();
    let names: Vec<_> = view.references().into_iter().collect();
    assert_eq!(names, ["agreed", "count", "items", "name"]);
    assert_eq!(
        view.defaults().get("items.1.title"),
        Some(&Value::Text("Two".into()))
    );
    assert!(matches!(view.nodes(), [Node::Element(root)] if root.kind == "column"));
}

#[test]
fn expressions() {
    let source = r#"
        data { a: 3, b: 4, word: "hi", list: [1, 2, 3] }
        column {
            label "{a + b * 2} {(a + b) * 2} {a - b} {b / 2} {b % 3}"
            label "{a < b && !(a == b) ? "less" : "more"} {word + a} {len(list)} {len(word)}"
            label "{{braces}} and \"quotes\" {list.1} {missing}{none}."
            label "{a >= 3 || missing} {-a} {word == "hi"} {list}"
        }
    "#;
    let harness = harness(source);
    harness.by_label("11 14 -1 2 1");
    harness.by_label("less hi3 3 2");
    harness.by_label("{braces} and \"quotes\" 2 .");
    harness.by_label("true -3 true [1, 2, 3]");
}

#[test]
fn errors_name_the_line_and_what_is_wrong() {
    let error = |source: &str| Markup::parse(source).unwrap_err();
    let unknown = error("column {\n    buton \"Go\"\n}");
    assert_eq!((unknown.line, unknown.column), (2, 5));
    assert!(
        unknown.message.starts_with("unknown element `buton`"),
        "{unknown}"
    );

    let style = error("column {\n  padding: wide\n}");
    assert_eq!((style.line, style.column), (2, 12));

    assert!(
        error("label \"x\" { on-click: go }")
            .message
            .contains("sends no events")
    );
    assert!(
        error("button \"x\" { on-press: go }")
            .message
            .contains("it has on-click")
    );
    assert!(
        error("column {\n label \"x\"")
            .message
            .contains("never closed")
    );
    assert!(error("label \"x").message.contains("no closing quote"));
    assert!(error("label \"{a\"").message.contains("no closing"));
    assert!(
        error("label { bind: a }")
            .message
            .contains("no value to bind")
    );
    assert!(
        error("Card \"x\"")
            .message
            .contains("unknown component `Card`")
    );
    assert!(
        error("component Card(a) { label \"{a}\" }\nCard { b: 1 }")
            .message
            .contains("no parameter `b`")
    );
    assert!(error("tab \"x\"").message.contains("inside `tabs`"));
    assert!(
        error("label \"x\" { label \"y\" }")
            .message
            .contains("holds no elements")
    );
    assert!(
        error("if a { padding: 2 }")
            .message
            .contains("is a property")
    );
    assert!(
        error("label \"{a +}\"")
            .message
            .contains("expected a value")
    );
    assert_eq!(error("column {\n\n  row { gap: 1 } }\n}").line, 4);
}

#[test]
fn the_other_widgets_declare() {
    let source = r#"
        data { volume: 30, choice: 1, page: 0, open: true, level: 0.5, size: 2, colours: ["Red", "Green"] }
        scroll { height: 100%
            tabs { bind: page
                tab "First" {
                    slider "Volume" { bind: volume; max: 50 }
                    number "Size" { bind: size; min: 0; max: 10; suffix: " px" }
                    radio "Left" { bind: choice; option: 0 }
                    radio "Right" { bind: choice; option: 1 }
                    switch "Dark" { checked: open; on-toggle: dark }
                    combo "Colour" { items: colours; bind: choice }
                    progress "Loading" { value: level }
                    spinner "Working"
                    link "More" { on-click: more; tooltip: "Opens more" }
                    collapsible "Details" { bind: open
                        grid { columns: 1fr 1fr
                            box { background: #223344; width: {volume}% }
                            spacer
                        }
                    }
                }
                tab "Second" { label "Second page" }
            }
        }
    "#;
    let mut harness = harness(source);
    let left = harness.by_label("Left");
    harness.click(left);
    assert_eq!(
        harness.state().values.get("choice"),
        Some(&Value::Number(0.0))
    );
    let dark = harness.by_label("Dark");
    harness.click(dark);
    assert_eq!(harness.state().events.last().unwrap().name, "dark");
    assert_eq!(
        harness.state().events.last().unwrap().value,
        Value::Bool(false)
    );
    let second = harness.by_label("Second");
    harness.click(second);
    harness.by_label("Second page");
    assert_eq!(
        harness.state().values.get("page"),
        Some(&Value::Number(1.0))
    );
}

#[test]
fn a_file_may_start_with_a_byte_order_mark() {
    let view = Markup::parse(&format!("\u{feff}{COUNTER}")).unwrap();
    assert!(matches!(view.nodes(), [Node::Element(root)] if root.kind == "column"));
}

#[test]
fn a_number_whose_maximum_is_below_its_minimum_declares() {
    // A range as it stands halfway through typing `max: 50`.
    let harness = harness(
        r#"
        data { size: 7 }
        column { number "Size" { bind: size; min: 10; max: 5 } }
    "#,
    );
    harness.by_label("Size");
}
