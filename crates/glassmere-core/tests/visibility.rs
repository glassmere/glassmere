//! `Style::hidden` (`visibility: hidden`): the node keeps its place in the
//! layout but is not drawn, hit, focused or reported.
use glassmere_core::{
    Context, Event, Key, Label, Length, MonospaceMeasurer, Style, StyleSheet, Ui,
};
use glassmere_scene::{Point, Primitive, Size};

const ROOT: Size = Size::new(400.0, 300.0);

fn frame(context: &mut Context, hidden: bool) {
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        ui.keyed("page", |ui| {
            ui.column(Style::default().gap(0.0), |ui| {
                ui.keyed("before", |ui| ui.button("Before"));
                ui.keyed("group", |ui| {
                    ui.column(
                        Style::default().hidden(hidden).height(Length::Px(50.0)),
                        |ui| {
                            ui.keyed("inner", |ui| ui.button("Inner"));
                            ui.keyed("note", |ui| ui.label(Label::new("Secret")));
                        },
                    )
                });
                ui.keyed("after", |ui| ui.button("After"));
            })
        });
    });
}

fn drawn(context: &Context, text: &str) -> bool {
    context.scene().primitives.iter().any(|primitive| {
        matches!(primitive,
            Primitive::Text { text: drawn, .. } | Primitive::StyledText { text: drawn, .. }
                if drawn == text)
    })
}

fn tab(context: &mut Context) {
    context.event(Event::KeyDown {
        key: Key::Tab,
        shift: false,
        repeat: false,
    });
    context.event(Event::KeyUp(Key::Tab));
}

fn focused(context: &Context) -> String {
    context
        .focused()
        .and_then(|id| context.details(id))
        .map_or_else(|| "none".into(), |details| details.key)
}

fn node_y(context: &Context, path: &[&str]) -> f32 {
    context.node(context.find(path).unwrap()).unwrap().bounds.y
}

#[test]
fn a_hidden_subtree_keeps_its_space_and_is_not_drawn() {
    let mut shown = Context::new();
    frame(&mut shown, false);
    let mut hidden = Context::new();
    frame(&mut hidden, true);
    assert!(drawn(&shown, "Secret") && drawn(&shown, "Inner"));
    assert!(!drawn(&hidden, "Secret") && !drawn(&hidden, "Inner"));
    assert!(drawn(&hidden, "After"));
    assert_eq!(
        node_y(&shown, &["page", "after"]),
        node_y(&hidden, &["page", "after"])
    );
}

#[test]
fn a_hidden_subtree_is_not_hit_and_is_not_focusable() {
    let mut context = Context::new();
    frame(&mut context, true);
    let inner = context
        .node(context.find(&["page", "group", "inner"]).unwrap())
        .unwrap();
    let at = Point {
        x: inner.bounds.x + 2.0,
        y: inner.bounds.y + 2.0,
    };
    assert_ne!(
        context.hit_test(at),
        context.find(&["page", "group", "inner"])
    );
    tab(&mut context);
    assert_eq!(focused(&context), "before");
    tab(&mut context);
    assert_eq!(focused(&context), "after", "Tab skips the hidden button");
    let reported = context
        .snapshots()
        .iter()
        .any(|snapshot| snapshot.id == context.find(&["page", "group", "inner"]).unwrap());
    assert!(!reported, "accessibility does not see it");
}

#[test]
fn showing_it_again_brings_everything_back() {
    let mut context = Context::new();
    frame(&mut context, true);
    frame(&mut context, false);
    assert!(drawn(&context, "Secret"));
    tab(&mut context);
    tab(&mut context);
    assert_eq!(focused(&context), "inner");
    // And hiding what is focused takes the focus away.
    frame(&mut context, true);
    assert_ne!(focused(&context), "inner");
}

#[test]
fn the_stylesheet_has_visibility() {
    let sheet = StyleSheet::parse(
        "class ghost {\nvisibility: hidden\n}\nclass seen {\nvisibility: visible\n}",
    )
    .unwrap();
    assert!(sheet.class("ghost").unwrap().hidden);
    assert!(!sheet.class("seen").unwrap().hidden);
    assert!(StyleSheet::parse("class bad {\nvisibility: collapse\n}").is_err());
}
