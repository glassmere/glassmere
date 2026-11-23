//! A wrapped text leaf may be asked for its size several times in one layout
//! pass, and the measurer answers the same each time. What is laid out is what
//! a context that never remembered an answer lays out, and an answer is never
//! carried into another pass.

use glassmere_core::{
    Context, FrameStats, Label, Length, MonospaceMeasurer, RichText, Style, TextWrap, Ui,
};
use glassmere_scene::{Color, Rect, Size, TextGeometry, TextMeasurer, TextSpan, TextStyle};

const PARAGRAPH: &str = "a long paragraph that has to wrap inside the view and then some more ";

/// A size that changes with the width in no regular way, so that what is
/// measured at one width is wrong at any other, however close.
fn uneven(size: Size, width: Option<f32>) -> Size {
    match width {
        Some(width) => Size::new(
            size.width + (width * 8.0) % 5.0,
            size.height + (width * 8.0) % 7.0,
        ),
        None => size,
    }
}

/// The monospace measurer, noting what it is asked to measure for layout and
/// making every answer's height `scale` times as tall. With `uneven` set the
/// answers also vary with the width as `uneven` says.
struct Probe {
    scale: f32,
    uneven: bool,
    style: Option<TextStyle>,
    plain: Vec<(String, Option<u32>)>,
    rich: Vec<Option<u32>>,
}

impl Probe {
    fn new() -> Self {
        Self {
            scale: 1.0,
            uneven: false,
            style: None,
            plain: Vec::new(),
            rich: Vec::new(),
        }
    }
}

impl TextMeasurer for Probe {
    fn measure_styled(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> Size {
        self.plain.push((text.to_owned(), width.map(f32::to_bits)));
        self.style = Some(style.clone());
        let size = MonospaceMeasurer.measure_styled(text, style, width);
        let size = if self.uneven {
            uneven(size, width)
        } else {
            size
        };
        Size::new(size.width, size.height * self.scale)
    }

    fn geometry(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> TextGeometry {
        MonospaceMeasurer.geometry(text, style, width)
    }

    fn geometry_rich(
        &mut self,
        spans: &[TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextGeometry {
        self.rich.push(width.map(f32::to_bits));
        MonospaceMeasurer.geometry_rich(spans, style, width)
    }
}

fn wrapped(width: Length) -> Style {
    Style::default().width(width).wrap(TextWrap::Word)
}

fn paragraphs(ui: &mut Ui<'_>, count: usize, style: &Style) {
    for n in 0..count {
        ui.keyed(n.to_string(), |ui| {
            ui.label(Label::new(format!("{n} {PARAGRAPH}").repeat(3)).style(style.clone()))
        });
    }
}

/// A scroll view of wrapped paragraphs.
fn scrolled(ui: &mut Ui<'_>, count: usize) {
    ui.keyed("view", |ui| {
        ui.scroll(
            Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Percent(1.0)),
            |ui| paragraphs(ui, count, &wrapped(Length::Percent(1.0))),
        )
    });
}

fn bounds(context: &Context, paths: &[Vec<String>]) -> Vec<Rect> {
    paths
        .iter()
        .map(|path| {
            let path: Vec<&str> = path.iter().map(String::as_str).collect();
            context.node(context.find(&path).unwrap()).unwrap().bounds
        })
        .collect()
}

fn labels(count: usize) -> Vec<Vec<String>> {
    (0..count)
        .map(|n| vec!["view".to_owned(), n.to_string()])
        .collect()
}

/// Everything laid out and drawn, compared with a context that was only ever
/// given this one frame.
fn assert_as_fresh(
    context: &Context,
    window: Size,
    paths: &[Vec<String>],
    view: impl FnOnce(&mut Ui<'_>),
) {
    let mut fresh = Context::new();
    fresh.run(window, &mut Probe::new(), view);
    assert_eq!(bounds(context, paths), bounds(&fresh, paths));
    assert_eq!(context.scene(), fresh.scene());
}

fn run(
    context: &mut Context,
    window: Size,
    measurer: &mut Probe,
    view: impl FnOnce(&mut Ui<'_>),
) -> FrameStats {
    measurer.plain.clear();
    measurer.rich.clear();
    context.run(window, measurer, view)
}

#[test]
fn a_leaf_is_measured_once_for_each_width_in_a_pass() {
    const COUNT: usize = 8;
    let mut context = Context::new();
    let mut measurer = Probe::new();
    run(&mut context, Size::new(500.0, 300.0), &mut measurer, |ui| {
        scrolled(ui, COUNT)
    });
    let narrow = Size::new(333.0, 300.0);
    let stats = run(&mut context, narrow, &mut measurer, |ui| {
        scrolled(ui, COUNT)
    });
    assert_eq!(stats.layout_passes, 1);
    // Every request is counted, whether or not the measurer is asked.
    assert!(stats.text_measures >= 3 * COUNT, "{stats:?}");
    let mut asked = measurer.plain.clone();
    let total = asked.len();
    asked.sort();
    asked.dedup();
    assert_eq!(total, asked.len(), "asked again for a size it gave");
    assert!(total < stats.text_measures);
    assert_as_fresh(&context, narrow, &labels(COUNT), |ui| scrolled(ui, COUNT));
}

#[test]
fn an_answer_is_not_carried_into_the_next_pass() {
    // Only the style changes, so the label keeps what it measured and layout
    // still has to ask the measurer again, which now answers differently.
    let view = |ui: &mut Ui<'_>, floor: f32| {
        ui.keyed("page", |ui| {
            ui.column(Style::default(), |ui| {
                ui.keyed("note", |ui| {
                    ui.label(
                        Label::new(PARAGRAPH.repeat(4))
                            .style(wrapped(Length::Px(240.0)).min_height(Length::Px(floor))),
                    )
                });
            })
        });
    };
    let height = |context: &Context| {
        context
            .node(context.find(&["page", "note"]).unwrap())
            .unwrap()
            .bounds
            .height
    };
    let window = Size::new(500.0, 400.0);
    let mut context = Context::new();
    let mut measurer = Probe::new();
    run(&mut context, window, &mut measurer, |ui| view(ui, 1.0));
    let single = height(&context);
    measurer.scale = 2.0;
    let stats = run(&mut context, window, &mut measurer, |ui| view(ui, 2.0));
    assert_eq!(stats.layout_passes, 1);
    assert!(!measurer.plain.is_empty());
    assert_eq!(height(&context), single * 2.0);
}

#[test]
fn a_changed_link_target_is_not_a_changed_measurement() {
    let view = |ui: &mut Ui<'_>, target: &str| {
        let text = RichText::new()
            .text("See ")
            .link("docs", target)
            .text(" for how this wraps inside a narrow block of text");
        ui.keyed("text", |ui| {
            ui.rich_text(&text, wrapped(Length::Px(200.0)), |_: String| ());
        });
    };
    let mut context = Context::new();
    let mut measurer = Probe::new();
    let window = Size::new(500.0, 400.0);
    let first = run(&mut context, window, &mut measurer, |ui| {
        view(ui, "https://example.test/a")
    });
    assert_eq!(first.layout_passes, 1);
    let second = run(&mut context, window, &mut measurer, |ui| {
        view(ui, "https://example.test/b")
    });
    assert_eq!(second.layout_passes, 0);
}

#[test]
fn a_leaf_measured_at_several_widths_in_a_pass_gets_each_answer() {
    // In a row without a width a label is asked at its widest, at its
    // narrowest and at the width the row gives it.
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("view", |ui| {
            ui.row(Style::default().gap(8.0), |ui| {
                paragraphs(ui, 3, &wrapped(Length::Auto));
            })
        });
    };
    let mut context = Context::new();
    let mut measurer = Probe::new();
    run(&mut context, Size::new(700.0, 400.0), &mut measurer, view);
    let window = Size::new(450.0, 400.0);
    run(&mut context, window, &mut measurer, view);
    let mut widths: Vec<_> = measurer.plain.iter().map(|(_, width)| *width).collect();
    widths.sort();
    widths.dedup();
    assert!(widths.len() > 1, "{widths:?}");
    assert_as_fresh(&context, window, &labels(3), view);
}

#[test]
fn a_leaf_is_as_tall_as_its_width_makes_it() {
    // Items that shrink are measured at their widest, at their narrowest and
    // at the width they end up with. In a narrow window that is hardly wider
    // than the narrowest, and the measurer answers each width differently.
    let style = wrapped(Length::Auto).shrink(1.0);
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("view", |ui| {
            ui.row(
                Style::default().width(Length::Percent(1.0)).gap(3.0),
                |ui| paragraphs(ui, 3, &style),
            )
        });
    };
    let mut context = Context::new();
    let mut measurer = Probe::new();
    measurer.uneven = true;
    for width in [500.5, 333.37, 10.0, 333.37, 87.31] {
        run(&mut context, Size::new(width, 300.0), &mut measurer, view);
        let text_style = measurer.style.clone().unwrap();
        for (n, path) in labels(3).iter().enumerate() {
            let text = format!("{n} {PARAGRAPH}").repeat(3);
            let path: Vec<&str> = path.iter().map(String::as_str).collect();
            let bounds = context.node(context.find(&path).unwrap()).unwrap().bounds;
            let asked = Some(bounds.width);
            let size = MonospaceMeasurer.measure_styled(&text, &text_style, asked);
            let height = uneven(size, asked).height;
            assert!(
                (bounds.height - height).abs() < 0.01,
                "window {width}: {bounds:?} should be {height} tall"
            );
        }
    }
}

#[test]
fn a_rich_block_is_shaped_once_for_each_width_in_a_pass() {
    let text = RichText::new()
        .text("Wrapped ")
        .bold("rich")
        .text(" text that has to wrap inside the view, again and again. ".repeat(6));
    let view = |ui: &mut Ui<'_>, ink: Color| {
        ui.keyed("view", |ui| {
            ui.scroll(
                Style::default()
                    .width(Length::Percent(1.0))
                    .height(Length::Percent(1.0)),
                |ui| {
                    for n in 0..4 {
                        ui.keyed(n.to_string(), |ui| {
                            let style = wrapped(Length::Percent(1.0)).foreground(ink);
                            ui.rich_text(&text, style, |_: String| ());
                        });
                    }
                },
            )
        });
    };
    let mut context = Context::new();
    let mut measurer = Probe::new();
    run(&mut context, Size::new(500.0, 300.0), &mut measurer, |ui| {
        view(ui, Color(0, 0, 0, 255))
    });
    let window = Size::new(280.0, 300.0);
    let stats = run(&mut context, window, &mut measurer, |ui| {
        view(ui, Color(0, 0, 0, 255))
    });
    assert!(stats.text_measures >= 3 * 4, "{stats:?}");
    let shaped = measurer.rich.len();
    // Painting shapes each block too; a frame that only repaints says how many
    // calls that is, and what remains is layout, which shapes each block once.
    let repainted = run(&mut context, window, &mut measurer, |ui| {
        view(ui, Color(200, 0, 0, 255))
    });
    assert_eq!(repainted.layout_passes, 0);
    assert_eq!(shaped - measurer.rich.len(), 4, "{:?}", measurer.rich);
    assert_as_fresh(&context, window, &labels(4), |ui| {
        view(ui, Color(200, 0, 0, 255))
    });
}
