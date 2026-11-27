//! `text_overflow: Ellipsis`: a line that is wider than its box ends with `…`.
use glassmere_core::{
    Context, Label, Length, MonospaceMeasurer, Role, Style, StyleSheet, TextOverflow, Ui,
};
use glassmere_scene::{Primitive, Size, TextMeasurer, TextStyle};

const LONG: &str = "The quick brown fox jumps over the lazy dog";

fn show(context: &mut Context, text: &str, width: f32, overflow: TextOverflow) {
    let text = text.to_owned();
    context.run(
        Size::new(500.0, 100.0),
        &mut MonospaceMeasurer,
        move |ui: &mut Ui<'_>| {
            ui.keyed("label", |ui| {
                ui.label(
                    Label::new(text.clone()).style(
                        Style::default()
                            .width(Length::Px(width))
                            .text_overflow(overflow),
                    ),
                )
            });
        },
    );
}

/// The text the label draws.
fn drawn(context: &Context) -> String {
    context
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            Primitive::Text { text, .. } | Primitive::StyledText { text, .. } => Some(text.clone()),
            _ => None,
        })
        .expect("the label is drawn")
}

fn width(text: &str) -> f32 {
    MonospaceMeasurer
        .measure_styled(text, &TextStyle::default(), None)
        .width
}

#[test]
fn without_it_the_text_is_all_there_and_cut_at_the_edge() {
    let mut context = Context::new();
    show(&mut context, LONG, 100.0, TextOverflow::Clip);
    assert_eq!(drawn(&context), LONG);
}

#[test]
fn an_overlong_line_ends_with_an_ellipsis_where_the_next_character_would_not_fit() {
    let mut context = Context::new();
    show(&mut context, LONG, 100.0, TextOverflow::Ellipsis);
    let shown = drawn(&context);
    assert!(shown.ends_with('\u{2026}'), "{shown}");
    assert!(shown.len() < LONG.len());
    assert!(LONG.starts_with(shown.trim_end_matches('\u{2026}')));
    assert!(width(&shown) <= 100.0 + 0.01, "{shown}: {}", width(&shown));
    // The next character on would not have fitted.
    let kept = shown.trim_end_matches('\u{2026}').chars().count();
    let longer: String = LONG.chars().take(kept + 1).chain(['\u{2026}']).collect();
    assert!(width(&longer) > 100.0, "{longer}: {}", width(&longer));
}

#[test]
fn a_line_that_fits_is_left_alone() {
    let mut context = Context::new();
    show(&mut context, "Short", 200.0, TextOverflow::Ellipsis);
    assert_eq!(drawn(&context), "Short");
}

#[test]
fn it_follows_the_room_the_box_has() {
    let mut context = Context::new();
    show(&mut context, LONG, 100.0, TextOverflow::Ellipsis);
    let narrow = drawn(&context);
    show(&mut context, LONG, 220.0, TextOverflow::Ellipsis);
    let wide = drawn(&context);
    assert!(wide.len() > narrow.len(), "{narrow} then {wide}");
    assert!(wide.ends_with('\u{2026}'));
    // Wide enough for all of it: no ellipsis.
    show(&mut context, LONG, 900.0, TextOverflow::Ellipsis);
    assert_eq!(drawn(&context), LONG);
    // And narrow again.
    show(&mut context, LONG, 100.0, TextOverflow::Ellipsis);
    assert_eq!(drawn(&context), narrow);
}

#[test]
fn a_new_text_is_cut_afresh() {
    let mut context = Context::new();
    show(&mut context, LONG, 100.0, TextOverflow::Ellipsis);
    show(
        &mut context,
        "Pack my box with five dozen liquor jugs",
        100.0,
        TextOverflow::Ellipsis,
    );
    let shown = drawn(&context);
    assert!(shown.starts_with("Pack"), "{shown}");
    assert!(shown.ends_with('\u{2026}'));
}

#[test]
fn the_name_a_screen_reader_gets_is_the_whole_text() {
    let mut context = Context::new();
    show(&mut context, LONG, 100.0, TextOverflow::Ellipsis);
    let node = context.node(context.find(&["label"]).unwrap()).unwrap();
    assert_eq!(node.semantics.label, LONG);
    assert_eq!(node.semantics.role, Role::Label);
}

#[test]
fn it_cuts_between_characters_that_belong_together() {
    // A family emoji is seven code points and one character; accents combine.
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    let text = format!("e\u{301}e\u{301}e\u{301}{family}{family}{family}{family}");
    let mut context = Context::new();
    for room in [10.0, 30.0, 60.0, 90.0] {
        show(&mut context, &text, room, TextOverflow::Ellipsis);
        let shown = drawn(&context);
        // Never a lone combining mark, a lone joiner or half a family.
        let kept = shown.trim_end_matches('\u{2026}');
        assert!(text.starts_with(kept), "{kept:?}");
        assert!(
            !kept.ends_with('\u{200D}') && !kept.ends_with('\u{1F468}') || kept == family,
            "{kept:?}"
        );
    }
}

#[test]
fn a_stylesheet_has_text_overflow() {
    let sheet = StyleSheet::parse(
        "class cell {\ntext-overflow: ellipsis\n}\nclass plain {\ntext-overflow: clip\n}",
    )
    .unwrap();
    assert_eq!(
        sheet.class("cell").unwrap().text_overflow,
        TextOverflow::Ellipsis
    );
    assert_eq!(
        sheet.class("plain").unwrap().text_overflow,
        TextOverflow::Clip
    );
    assert!(StyleSheet::parse("class bad {\ntext-overflow: fade\n}").is_err());
}
