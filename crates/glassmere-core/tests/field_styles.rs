//! The number input's field and steppers, and the autocomplete's field, take
//! a style of their own, so they fit a bar that is not 44 tall.

use glassmere_core::{
    Autocomplete, Context, Length, MonospaceMeasurer, NodeId, NumberOptions, Style, Ui, suggest,
};
use glassmere_scene::Size;

const ROOT: Size = Size::new(600.0, 400.0);

fn number(options: NumberOptions) -> Context {
    let mut context = Context::new();
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_, ()>| {
        ui.keyed("amount", |ui| {
            ui.number_input("Amount", 3.0, options.clone(), |_| ());
        });
    });
    context
}

fn height(context: &Context, path: &[&str]) -> f32 {
    context.node(find(context, path)).unwrap().bounds.height
}

fn find(context: &Context, path: &[&str]) -> NodeId {
    context.find(path).unwrap()
}

fn compact() -> Style {
    Style {
        height: Length::Px(28.0),
        padding: 4.0,
        ..Style::button()
    }
}

#[test]
fn the_number_field_and_steppers_are_44_tall_unless_told_otherwise() {
    let context = number(NumberOptions::default());
    assert_eq!(height(&context, &["amount", "$field"]), 44.0);
    assert_eq!(height(&context, &["amount", "$less"]), 44.0);
    assert_eq!(
        context
            .node(find(&context, &["amount", "$more"]))
            .unwrap()
            .bounds
            .width,
        32.0
    );
}

#[test]
fn a_number_field_can_be_compact() {
    let context = number(
        NumberOptions::default()
            .field_style(compact())
            .stepper_style(Style {
                width: Length::Px(20.0),
                ..compact()
            }),
    );
    assert_eq!(height(&context, &["amount", "$field"]), 28.0);
    assert_eq!(height(&context, &["amount", "$less"]), 28.0);
    let more = context
        .node(find(&context, &["amount", "$more"]))
        .unwrap()
        .bounds;
    assert_eq!(more.width, 20.0);
}

#[test]
fn a_number_field_can_do_without_its_steppers() {
    let context = number(NumberOptions::default().without_steppers());
    assert!(context.find(&["amount", "$field"]).is_some());
    assert!(context.find(&["amount", "$less"]).is_none());
    assert!(context.find(&["amount", "$more"]).is_none());
}

#[test]
fn an_autocomplete_takes_a_style() {
    let cities = ["Zagreb", "Zurich"];
    let size = |style: Option<Style>| {
        let mut context = Context::new();
        context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_, ()>| {
            ui.keyed("city", |ui| {
                let matches = suggest("Z", &cities, 8);
                let mut extra = Autocomplete::new();
                if let Some(style) = style.clone() {
                    extra = extra.style(style);
                }
                ui.autocomplete_with("City", "Z", &matches, |_| (), extra);
            });
        });
        let bounds = context
            .node(context.find(&["city"]).unwrap())
            .unwrap()
            .bounds;
        (bounds.width, bounds.height)
    };
    assert_eq!(size(None), (280.0, 44.0));
    let (width, height) = size(Some(Style {
        width: Length::Px(160.0),
        ..compact()
    }));
    assert_eq!((width, height), (160.0, 28.0));
}
