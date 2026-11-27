//! `text_align`: where a line sits in a box wider than it, and the alignment
//! the lines of wrapped text are shaped with.
use glassmere_core::{
    Context, Label, Length, MonospaceMeasurer, Style, StyleSheet, TextAlign, TextWrap, Ui,
};
use glassmere_scene::{Primitive, Size};

const ROOT: Size = Size::new(400.0, 300.0);

fn label_in(style: Style, parent: Style) -> Context {
    let mut context = Context::new();
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        ui.keyed("box", |ui| {
            ui.column(parent.clone(), |ui| {
                ui.keyed("label", |ui| {
                    ui.label(Label::new("Total").style(style.clone()));
                })
            })
        });
    });
    context
}

/// Where the label's text is drawn, relative to the left of its box.
fn text_offset(context: &Context) -> f32 {
    let bounds = context
        .node(context.find(&["box", "label"]).unwrap())
        .unwrap()
        .bounds;
    context
        .scene()
        .primitives
        .iter()
        .find_map(|p| match p {
            Primitive::Text { rect, text, .. } | Primitive::StyledText { rect, text, .. }
                if text == "Total" =>
            {
                Some(rect.x - bounds.x)
            }
            _ => None,
        })
        .expect("the label's text is drawn")
}

fn wide() -> Style {
    Style::default().width(Length::Px(300.0))
}

fn column() -> Style {
    Style::default().width(Length::Px(300.0))
}

#[test]
fn a_line_sits_at_the_start_the_centre_or_the_end_of_a_wider_box() {
    let start = text_offset(&label_in(wide(), column()));
    let center = text_offset(&label_in(wide().text_align(TextAlign::Center), column()));
    let end = text_offset(&label_in(wide().text_align(TextAlign::End), column()));
    let justify = text_offset(&label_in(wide().text_align(TextAlign::Justify), column()));
    assert_eq!(start, 0.0);
    assert!(center > 50.0, "{center}");
    assert!(end > center, "{end} against {center}");
    // One line is never stretched: it stays at the start.
    assert_eq!(justify, 0.0);
    // The centred text is as far from the right edge as from the left.
    let width = 300.0;
    let text = width - end;
    assert!((center - (width - text) / 2.0).abs() < 0.5, "{center}");
    assert!((end + text - width).abs() < 0.5);
}

#[test]
fn what_a_text_align_on_a_parent_says_the_label_inside_takes() {
    let inherited = text_offset(&label_in(wide(), column().text_align(TextAlign::End)));
    let own = text_offset(&label_in(wide().text_align(TextAlign::End), column()));
    assert!(inherited > 100.0);
    assert_eq!(inherited, own);
    // The label's own value wins over the parent's.
    let overridden = text_offset(&label_in(
        wide().text_align(TextAlign::Start),
        column().text_align(TextAlign::End),
    ));
    assert_eq!(overridden, 0.0);
}

#[test]
fn wrapped_text_is_shaped_with_the_alignment() {
    let mut context = Context::new();
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        ui.keyed("box", |ui| {
            ui.column(Style::default().width(Length::Px(120.0)), |ui| {
                ui.label(
                    Label::new("Total").style(
                        Style::default()
                            .wrap(TextWrap::Word)
                            .text_align(TextAlign::Center),
                    ),
                );
            })
        });
    });
    let aligned = context.scene().primitives.iter().find_map(|p| match p {
        Primitive::StyledText { style, text, .. } if text == "Total" => Some(style.align),
        _ => None,
    });
    assert_eq!(aligned, Some(TextAlign::Center));
}

#[test]
fn a_stylesheet_has_text_align() {
    let sheet = StyleSheet::parse(
        "class price {\ntext-align: end\n}\nclass note {\ntext-align: justify\n}",
    )
    .unwrap();
    assert_eq!(
        sheet.class("price").unwrap().text_align,
        Some(TextAlign::End)
    );
    assert_eq!(
        sheet.class("note").unwrap().text_align,
        Some(TextAlign::Justify)
    );
    assert!(StyleSheet::parse("class bad {\ntext-align: middle\n}").is_err());
}
