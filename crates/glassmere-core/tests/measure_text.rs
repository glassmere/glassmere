//! A view can ask how big some text is, as the layout will measure it.
use glassmere_core::{
    Context, CustomWidget, MonospaceMeasurer, Semantics, Style, TextWrap, Ui, WidgetBuilder,
    WidgetInput,
};
use glassmere_scene::{Size, TextStyle};
use std::cell::Cell;
use std::rc::Rc;

const WINDOW: Size = Size::new(500.0, 300.0);

fn bounds(context: &Context, name: &str) -> glassmere_scene::Rect {
    context.node(context.find(&[name]).unwrap()).unwrap().bounds
}

#[test]
fn the_size_asked_for_is_the_size_a_label_of_that_text_gets() {
    let asked = Rc::new(Cell::new(Size::new(0.0, 0.0)));
    let out = asked.clone();
    let mut context = Context::new();
    context.run(WINDOW, &mut MonospaceMeasurer, move |ui: &mut Ui<'_>| {
        out.set(ui.measure_text("A label of some length", &ui.text_style()));
        ui.keyed("label", |ui| ui.label("A label of some length"));
    });
    let label = bounds(&context, "label");
    let asked = asked.get();
    assert!(asked.width > 0.0 && asked.height > 0.0, "{asked:?}");
    assert_eq!((asked.width, asked.height), (label.width, label.height));
}

#[test]
fn a_bigger_font_and_more_text_measure_bigger() {
    let sizes = Rc::new(Cell::new([Size::new(0.0, 0.0); 3]));
    let out = sizes.clone();
    let mut context = Context::new();
    context.run(WINDOW, &mut MonospaceMeasurer, move |ui: &mut Ui<'_>| {
        let plain = ui.text_style();
        let big = TextStyle {
            font_size: plain.font_size * 2.0,
            ..plain.clone()
        };
        out.set([
            ui.measure_text("text", &plain),
            ui.measure_text("text and more of it", &plain),
            ui.measure_text("text", &big),
        ]);
    });
    let [short, long, big] = sizes.get();
    assert!(long.width > short.width, "{short:?} {long:?}");
    assert!(
        big.width > short.width && big.height > short.height,
        "{short:?} {big:?}"
    );
}

#[test]
fn wrapping_at_a_width_makes_it_taller() {
    let sizes = Rc::new(Cell::new([Size::new(0.0, 0.0); 2]));
    let out = sizes.clone();
    let mut context = Context::new();
    context.run(WINDOW, &mut MonospaceMeasurer, move |ui: &mut Ui<'_>| {
        let style = TextStyle {
            wrap: TextWrap::Word,
            ..ui.text_style()
        };
        let text = "several words that will not fit on one narrow line";
        out.set([
            ui.measure_text_wrapped(text, &style, 1000.0),
            ui.measure_text_wrapped(text, &style, 80.0),
        ]);
    });
    let [wide, narrow] = sizes.get();
    assert!(narrow.height > wide.height, "{wide:?} {narrow:?}");
    assert!(narrow.width <= 80.0 + 0.01, "{narrow:?}");
}

#[test]
fn a_painted_control_can_size_itself_to_its_label() {
    let mut context = Context::new();
    context.run(WINDOW, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        let label = ui.measure_text("Save changes", &ui.text_style());
        let size = Size::new(label.width + 24.0, label.height + 12.0);
        ui.keyed("pill", |ui| {
            ui.widget(
                Style::default(),
                WidgetBuilder::new(move |_: &WidgetInput| CustomWidget {
                    size,
                    semantics: Semantics::default(),
                    ..CustomWidget::default()
                }),
            );
        });
        ui.keyed("text", |ui| ui.label("Save changes"));
    });
    let (pill, text) = (bounds(&context, "pill"), bounds(&context, "text"));
    assert_eq!(pill.width, text.width + 24.0);
    assert_eq!(pill.height, text.height + 12.0);
}

#[test]
fn it_works_inside_a_mapped_scope_too() {
    use glassmere_core::{App, AppDriver, Effects};
    #[derive(Clone)]
    enum Inner {}
    struct Wrapper(Rc<Cell<f32>>);
    impl App for Wrapper {
        type Message = ();
        fn update(&mut self, _: (), _: &mut Effects<'_, ()>) {}
        fn view(&self, ui: &mut Ui<'_, ()>) {
            let seen = self.0.clone();
            ui.map(
                |_: Inner| (),
                move |ui: &mut Ui<'_, Inner>| {
                    seen.set(ui.measure_text("inside", &ui.text_style()).width);
                },
            );
        }
    }
    let seen = Rc::new(Cell::new(0.0));
    let mut app = Wrapper(seen.clone());
    let mut driver = AppDriver::new(|| true);
    let mut context = Context::new();
    driver.frame(&mut app, &mut context, WINDOW, &mut MonospaceMeasurer);
    assert!(seen.get() > 0.0);
}
