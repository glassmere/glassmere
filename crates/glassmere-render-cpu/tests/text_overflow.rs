//! An overlong line ended with an ellipsis, shaped by the real text system
//! (the bundled font, so the lines break the same everywhere): what is drawn
//! fits, by the measure the layout used, and one more letter would not.
use glassmere_core::{Context, Label, Length, Style, TextOverflow, Ui};
use glassmere_scene::{Primitive, Size, TextMeasurer, TextStyle};
use glassmere_text::TextSystem;

fn fira() -> TextSystem {
    TextSystem::with_fonts([include_bytes!(
        "../../glassmere-render-wgpu/fonts/FiraMono-Regular.ttf"
    )
    .to_vec()])
}

const TEXT: &str = "Quarterly revenue by region and product line, with forecasts";

fn drawn(room: f32) -> (String, TextSystem) {
    let mut system = fira();
    let mut context = Context::new();
    context.run(Size::new(500.0, 100.0), &mut system, |ui: &mut Ui<'_>| {
        ui.keyed("label", |ui| {
            ui.label(
                Label::new(TEXT).style(
                    Style::default()
                        .width(Length::Px(room))
                        .text_overflow(TextOverflow::Ellipsis),
                ),
            )
        });
    });
    let text = context
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            Primitive::Text { text, .. } | Primitive::StyledText { text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    (text, system)
}

#[test]
fn what_is_drawn_fits_the_box_and_one_more_letter_would_not() {
    for room in [60.0, 120.0, 200.0, 280.0] {
        let (shown, mut system) = drawn(room);
        assert!(shown.ends_with('\u{2026}'), "{room}: {shown}");
        let mut width = |text: &str| {
            system
                .measure_styled(text, &TextStyle::default(), None)
                .width
        };
        assert!(
            width(&shown) <= room + 0.5,
            "{room}: {shown} is {}",
            width(&shown)
        );
        let kept = shown.trim_end_matches('\u{2026}').chars().count();
        let longer: String = TEXT.chars().take(kept + 1).chain(['\u{2026}']).collect();
        assert!(
            width(&longer) > room,
            "{room}: {longer} is {}",
            width(&longer)
        );
    }
}
