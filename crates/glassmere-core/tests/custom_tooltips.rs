//! A tooltip on a widget of the application's own: it shows after the hover
//! delay and after the keyboard delay, hides when the pointer leaves, and
//! describes the widget to assistive technology, as it does for a stock one.
use glassmere_core::{
    Context, CustomWidget, Event, Key, MonospaceMeasurer, Role, Semantics, Style, TOOLTIP_DELAY,
    Ui, WidgetBuilder, WidgetInput,
};
use glassmere_scene::{Point, Primitive, Size};
use std::time::Duration;

const WINDOW: Size = Size::new(400.0, 300.0);
const HELP: &str = "Marks it done";

fn view(ui: &mut Ui<'_>) {
    ui.keyed("pill", |ui| {
        ui.widget(
            Style::default().offset(40.0, 40.0),
            WidgetBuilder::new(|_: &WidgetInput| CustomWidget {
                size: Size::new(90.0, 30.0),
                semantics: Semantics {
                    role: Role::Button,
                    label: "Done".into(),
                    focusable: true,
                    ..Semantics::default()
                },
                ..CustomWidget::default()
            }),
        );
    });
    ui.tooltip(HELP);
}

fn shown(context: &Context) -> bool {
    context
        .scene()
        .primitives
        .iter()
        .any(|p| matches!(p, Primitive::StyledText { text, .. } if text == HELP))
}

fn frame(context: &mut Context) {
    context.run(WINDOW, &mut MonospaceMeasurer, view);
}

fn pointer(context: &mut Context, at: Point) {
    context.event(Event::PointerMoved(at));
}

const OVER: Point = Point { x: 80.0, y: 55.0 };
const AWAY: Point = Point { x: 380.0, y: 280.0 };

#[test]
fn hovering_a_custom_widget_shows_its_tooltip_after_the_delay_and_leaving_hides_it() {
    let mut context = Context::new();
    context.set_now(Duration::ZERO);
    frame(&mut context);
    assert_eq!(context.next_wake(), None);
    pointer(&mut context, OVER);
    assert_eq!(context.next_wake(), Some(TOOLTIP_DELAY));
    context.set_now(TOOLTIP_DELAY - Duration::from_millis(1));
    frame(&mut context);
    assert!(!shown(&context), "not yet due");
    context.set_now(TOOLTIP_DELAY);
    frame(&mut context);
    assert!(shown(&context), "shown once the delay has passed");
    pointer(&mut context, AWAY);
    frame(&mut context);
    assert!(!shown(&context), "gone when the pointer leaves");
}

#[test]
fn keyboard_focus_on_a_custom_widget_shows_its_tooltip_too() {
    let mut context = Context::new();
    context.set_now(Duration::ZERO);
    frame(&mut context);
    pointer(&mut context, AWAY);
    context.event(Event::KeyDown {
        key: Key::Tab,
        shift: false,
        repeat: false,
    });
    context.event(Event::KeyUp(Key::Tab));
    let pill = context.find(&["pill"]).unwrap();
    assert!(context.node(pill).unwrap().semantics.focusable);
    context.set_now(TOOLTIP_DELAY);
    frame(&mut context);
    assert!(shown(&context), "the focused widget's tooltip is shown");
}

#[test]
fn the_tooltip_is_the_description_of_the_custom_widget() {
    let mut context = Context::new();
    frame(&mut context);
    let pill = context.node(context.find(&["pill"]).unwrap()).unwrap();
    assert_eq!(pill.description.as_deref(), Some(HELP));
    assert_eq!(pill.semantics.label, "Done");
    assert_eq!(pill.semantics.role, Role::Button);
}
