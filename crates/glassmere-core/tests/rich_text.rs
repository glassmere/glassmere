mod support;

use glassmere_core::{
    Context, Event, FontFamily, Key, Length, MonospaceMeasurer, RichRun, RichText, Role, Style,
    TextWrap,
};
use glassmere_scene::{Point, Primitive, Rect, Size};
use support::{Driven, on};

const ROOT: Size = Size::new(600.0, 400.0);
const ADVANCE: f32 = 16.0 * 0.6;
const LINE: f32 = 16.0 * 1.4;

fn sample() -> RichText {
    RichText::new()
        .text("See ")
        .link("docs", "https://example.test/docs")
        .text(" or ")
        .link("source", "https://example.test/src")
        .text(": ")
        .bold("bold")
        .italic(" slanted ")
        .code("code()")
        .run(RichRun::new(" gone").strikethrough())
}

/// Declares `text` at `width` with no message wired, for paint checks.
fn frame(context: &mut Context, text: &RichText, width: f32) {
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        ui.keyed("text", |ui| {
            ui.rich_text(
                text,
                Style::default()
                    .width(Length::Px(width))
                    .wrap(TextWrap::Word),
                |_: String| (),
            );
        });
        ui.button("after");
    });
}

fn bounds(context: &Context) -> Rect {
    context
        .node(context.find(&["text"]).unwrap())
        .unwrap()
        .bounds
}

struct State {
    text: RichText,
    width: f32,
    /// Every link a click or Enter followed, in order.
    followed: Vec<String>,
}

fn driven(text: RichText) -> Driven<State> {
    Driven::new(
        ROOT,
        State {
            text,
            width: 580.0,
            followed: Vec::new(),
        },
        |state, ui| {
            ui.keyed("text", |ui| {
                ui.rich_text(
                    &state.text,
                    Style::default()
                        .width(Length::Px(state.width))
                        .wrap(TextWrap::Word),
                    on(|s: &mut State, url: String| s.followed.push(url)),
                );
            });
            ui.button("after");
        },
    )
}

#[test]
fn runs_become_styled_spans_with_backgrounds_and_rules_and_the_plain_text_is_the_label() {
    let mut context = Context::new();
    let text = sample();
    frame(&mut context, &text, 580.0);
    let node = context.node(context.find(&["text"]).unwrap()).unwrap();
    assert_eq!(node.semantics.role, Role::Label);
    assert_eq!(
        node.semantics.label,
        "See docs or source: bold slanted code() gone"
    );
    let primitives = &context.scene().primitives;
    let Some(Primitive::RichText { spans, .. }) = primitives
        .iter()
        .find(|primitive| matches!(primitive, Primitive::RichText { .. }))
    else {
        panic!("no rich block");
    };
    let shape: Vec<_> = spans
        .iter()
        .map(|span| {
            (
                span.text.as_str(),
                span.color.is_some(),
                span.weight,
                span.italic,
                span.family.clone(),
            )
        })
        .collect();
    assert_eq!(shape[1], ("docs", true, None, None, None));
    assert_eq!(shape[5], ("bold", false, Some(700), None, None));
    assert_eq!(shape[6], (" slanted ", false, None, Some(true), None));
    assert_eq!(
        shape[7],
        ("code()", false, None, None, Some(FontFamily::Monospace))
    );
    // One line: a code background, two link underlines and one strikethrough.
    let quads: Vec<Rect> = primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Quad { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    let code_x = "See docs or source: bold slanted ".len() as f32 * ADVANCE;
    assert!(quads.iter().any(|rect| (rect.x - code_x).abs() < 0.5
        && (rect.width - 6.0 * ADVANCE).abs() < 0.5
        && rect.height == LINE));
    let rules = quads.iter().filter(|rect| rect.height == 1.0).count();
    assert_eq!(rules, 3);
}

#[test]
fn rich_blocks_wrap_to_their_width_and_relayout_when_runs_change() {
    let mut context = Context::new();
    let text = sample();
    frame(&mut context, &text, 580.0);
    assert_eq!(bounds(&context).height, LINE);
    frame(&mut context, &text, 120.0);
    let wrapped = bounds(&context).height;
    assert!(wrapped >= LINE * 3.0, "{wrapped}");
    // Underlines follow the wrapped lines.
    let longer = sample().text(" and a much longer tail that needs more lines");
    frame(&mut context, &longer, 120.0);
    assert!(bounds(&context).height > wrapped);
}

#[test]
fn links_follow_by_click_and_keyboard_and_plain_blocks_take_no_focus() {
    let mut app = driven(sample());
    // "source" starts after "See docs or ".
    let point = Point {
        x: 13.0 * ADVANCE,
        y: LINE / 2.0,
    };
    app.context.event(Event::PointerMoved(point));
    app.context.event(Event::PointerDown);
    app.context.event(Event::PointerUp);
    app.frame();
    assert_eq!(app.state().followed, ["https://example.test/src"]);
    app.frame();
    assert_eq!(app.state().followed.len(), 1, "one click, one message");
    // A click beside the links follows nothing.
    app.context
        .event(Event::PointerMoved(Point { x: 2.0, y: 5.0 }));
    app.context.event(Event::PointerDown);
    app.context.event(Event::PointerUp);
    app.frame();
    assert_eq!(app.state().followed.len(), 1);

    let press = |app: &mut Driven<State>, key: Key| {
        app.context.event(Event::KeyDown {
            key,
            shift: false,
            repeat: false,
        });
        app.context.event(Event::KeyUp(key));
    };
    // The block is focused from the click; the clicked link is the active one.
    assert_eq!(app.context.focused(), app.context.find(&["text"]));
    press(&mut app, Key::ArrowRight);
    press(&mut app, Key::Enter);
    app.frame();
    assert_eq!(
        app.state().followed.last().map(String::as_str),
        Some("https://example.test/docs")
    );
    assert_eq!(app.state().followed.len(), 2);

    let mut app = driven(RichText::new().text("no links ").bold("here"));
    press(&mut app, Key::Tab);
    assert_ne!(app.context.focused(), app.context.find(&["text"]));
}
