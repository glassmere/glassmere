use glassmere_core::{
    Button, Context, Direction, Event, Length, MonospaceMeasurer, Role, Style, TOOLTIP_DELAY,
    Tooltip, TooltipPlacement, Ui,
};
use glassmere_scene::{Color, Point, Primitive, Rect, Size};
use std::borrow::Cow;
use std::time::Duration;

const WINDOW: Size = Size::new(400.0, 300.0);
const TEXT: &str = "Saves the document";
/// Short enough to fit beside a widget in the middle of the window.
const SHORT: &str = "Save";
const GAP: f32 = 6.0;

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
    context.run(WINDOW, &mut MonospaceMeasurer, view);
}

/// A button of a fixed size at a fixed place, so that a tooltip has somewhere
/// exact to be.
fn spot(ui: &mut Ui<'_>, key: &'static str, x: f32, y: f32, tooltip: Option<Tooltip>) {
    ui.keyed(key, |ui| {
        ui.button(
            Button::new(key).style(
                Style::button()
                    .width(Length::Px(100.0))
                    .height(Length::Px(40.0))
                    .offset(x, y),
            ),
        )
    });
    if let Some(tooltip) = tooltip {
        ui.tooltip_with(tooltip);
    }
}

/// Where the widget `key` is.
fn widget(context: &Context, key: &str) -> Rect {
    context.node(context.find(&[key]).unwrap()).unwrap().bounds
}

/// The bubble around the tooltip text on show, if there is one.
fn bubble(context: &Context, text: &str) -> Option<Rect> {
    let primitives = &context.scene().primitives;
    let at = primitives.iter().position(
        |primitive| matches!(primitive, Primitive::StyledText { text: shown, .. } if shown == text),
    )?;
    match primitives.get(at.checked_sub(1)?)? {
        Primitive::Quad { rect, .. } => Some(*rect),
        _ => None,
    }
}

fn shown(context: &Context, text: &str) -> bool {
    bubble(context, text).is_some()
}

fn hover(context: &mut Context, key: &str) {
    let rect = widget(context, key);
    context.event(Event::PointerMoved(Point {
        x: rect.x + rect.width / 2.0,
        y: rect.y + rect.height / 2.0,
    }));
}

fn within_window(rect: Rect) -> bool {
    rect.x >= 0.0
        && rect.y >= 0.0
        && rect.x + rect.width <= WINDOW.width
        && rect.y + rect.height <= WINDOW.height
}

/// Hovers `key`, lets the delay pass and returns the bubble that shows.
fn bubble_of(
    text: &str,
    placement: TooltipPlacement,
    x: f32,
    y: f32,
    direction: Direction,
) -> (Rect, Rect) {
    let mut context = Context::new();
    context.set_direction(direction);
    let view = |ui: &mut Ui<'_>| {
        spot(
            ui,
            "target",
            x,
            y,
            Some(Tooltip::new(text).placement(placement)),
        )
    };
    context.set_now(Duration::ZERO);
    frame(&mut context, view);
    hover(&mut context, "target");
    context.set_now(TOOLTIP_DELAY);
    frame(&mut context, view);
    let bubble = bubble(&context, text).expect("the tooltip shows");
    assert!(within_window(bubble), "{bubble:?} leaves the window");
    (widget(&context, "target"), bubble)
}

fn right(rect: Rect) -> f32 {
    rect.x + rect.width
}

fn bottom(rect: Rect) -> f32 {
    rect.y + rect.height
}

fn middle(rect: Rect) -> f32 {
    rect.y + rect.height / 2.0
}

#[test]
fn each_placement_puts_the_tooltip_on_its_side_of_the_widget() {
    let (x, y) = (150.0, 130.0);
    for placement in [TooltipPlacement::Auto, TooltipPlacement::Below] {
        let (target, bubble) = bubble_of(SHORT, placement, x, y, Direction::Ltr);
        assert_eq!(bubble.y, bottom(target) + GAP, "{placement:?}");
        assert_eq!(bubble.x, target.x, "{placement:?} lines up with the start");
    }
    let (target, bubble) = bubble_of(SHORT, TooltipPlacement::Above, x, y, Direction::Ltr);
    assert_eq!(bottom(bubble), target.y - GAP);
    assert_eq!(bubble.x, target.x);
    let (target, bubble) = bubble_of(SHORT, TooltipPlacement::Before, x, y, Direction::Ltr);
    assert_eq!(right(bubble), target.x - GAP);
    assert!((middle(bubble) - middle(target)).abs() < 0.5, "centred");
    let (target, bubble) = bubble_of(SHORT, TooltipPlacement::After, x, y, Direction::Ltr);
    assert_eq!(bubble.x, right(target) + GAP);
    assert!((middle(bubble) - middle(target)).abs() < 0.5, "centred");
}

#[test]
fn a_tooltip_flips_to_the_opposite_side_at_every_edge_and_stays_in_the_window() {
    // Below has no room at the bottom, and Auto goes above as it always did.
    for placement in [TooltipPlacement::Auto, TooltipPlacement::Below] {
        let (target, bubble) = bubble_of(SHORT, placement, 150.0, 255.0, Direction::Ltr);
        assert_eq!(
            bottom(bubble),
            target.y - GAP,
            "{placement:?} at the bottom"
        );
    }
    // Above has none at the top.
    let (target, bubble) = bubble_of(SHORT, TooltipPlacement::Above, 150.0, 4.0, Direction::Ltr);
    assert_eq!(bubble.y, bottom(target) + GAP, "above at the top");
    // Before has none at the left edge, After none at the right one.
    let (target, bubble) = bubble_of(SHORT, TooltipPlacement::Before, 4.0, 130.0, Direction::Ltr);
    assert_eq!(bubble.x, right(target) + GAP, "before at the left");
    let (target, bubble) = bubble_of(SHORT, TooltipPlacement::After, 296.0, 130.0, Direction::Ltr);
    assert_eq!(right(bubble), target.x - GAP, "after at the right");
    // Beside a widget that is partly outside, the tooltip is pulled in along the edge.
    let (_, bubble) = bubble_of(SHORT, TooltipPlacement::After, 4.0, -10.0, Direction::Ltr);
    assert_eq!(bubble.y, 0.0);
    let (_, bubble) = bubble_of(
        SHORT,
        TooltipPlacement::Before,
        296.0,
        270.0,
        Direction::Ltr,
    );
    assert_eq!(bottom(bubble), WINDOW.height);
    // Above and below slide along the edge instead of leaving the window.
    let (target, bubble) = bubble_of(TEXT, TooltipPlacement::Below, 296.0, 130.0, Direction::Ltr);
    assert_eq!(bubble.y, bottom(target) + GAP);
    assert_eq!(right(bubble), WINDOW.width);
}

#[test]
fn before_and_after_mirror_in_a_right_to_left_window() {
    let (x, y) = (150.0, 130.0);
    let (target, bubble) = bubble_of(SHORT, TooltipPlacement::Before, x, y, Direction::Rtl);
    assert_eq!(bubble.x, right(target) + GAP, "before is on the right");
    let (target, bubble) = bubble_of(SHORT, TooltipPlacement::After, x, y, Direction::Rtl);
    assert_eq!(right(bubble), target.x - GAP, "after is on the left");
    // Flipping mirrors too: before has no room on the right edge.
    let (target, bubble) = bubble_of(SHORT, TooltipPlacement::Before, 296.0, y, Direction::Rtl);
    assert_eq!(right(bubble), target.x - GAP);
    let (target, bubble) = bubble_of(SHORT, TooltipPlacement::After, 4.0, y, Direction::Rtl);
    assert_eq!(bubble.x, right(target) + GAP);
    // Below lines up with the start edge, which is the right one.
    let (target, bubble) = bubble_of(SHORT, TooltipPlacement::Below, x, y, Direction::Rtl);
    assert_eq!(right(bubble), right(target));
}

#[test]
fn a_tooltip_waits_for_its_own_delay_and_asks_for_no_other_frame() {
    let view = |ui: &mut Ui<'_>| {
        spot(
            ui,
            "slow",
            20.0,
            20.0,
            Some(Tooltip::new("Slow").delay(ms(1500))),
        );
        spot(
            ui,
            "quick",
            20.0,
            80.0,
            Some(Tooltip::new("Quick").delay(ms(80))),
        );
        spot(ui, "usual", 20.0, 140.0, Some("Usual".into()));
        spot(
            ui,
            "instant",
            20.0,
            200.0,
            Some(Tooltip::new("Instant").delay(Duration::ZERO)),
        );
    };
    let mut context = Context::new();
    context.set_now(Duration::ZERO);
    frame(&mut context, view);
    assert_eq!(context.next_wake(), None, "nothing waits before a hover");
    for (key, text, delay) in [
        ("slow", "Slow", ms(1500)),
        ("quick", "Quick", ms(80)),
        ("usual", "Usual", TOOLTIP_DELAY),
        ("instant", "Instant", Duration::ZERO),
    ] {
        // Each starts from a pointer that has left, so no tooltip is following another.
        context.event(Event::PointerLeft);
        context.set_now(context.now() + ms(5000));
        let start = context.now();
        hover(&mut context, key);
        assert_eq!(
            context.next_wake(),
            Some(start + delay),
            "{key} wakes at its delay"
        );
        if delay > Duration::ZERO {
            context.set_now(start + delay - ms(1));
            frame(&mut context, view);
            assert!(!shown(&context, text), "{key} is not due");
        }
        context.set_now(start + delay);
        frame(&mut context, view);
        assert!(shown(&context, text), "{key} shows at its delay");
        assert_eq!(
            context.next_wake(),
            None,
            "{key}: nothing pending while shown"
        );
        context.set_now(start + delay + ms(3000));
        let idle = context.run(WINDOW, &mut MonospaceMeasurer, view);
        assert!(!idle.scene_rebuilt, "{key}: an idle frame repaints nothing");
    }
}

#[test]
fn tooltip_is_tooltip_with_text_and_text_converts_to_a_tooltip() {
    let owned = String::from(TEXT);
    let by_text = |ui: &mut Ui<'_>| {
        spot(ui, "target", 150.0, 130.0, None);
        ui.tooltip(TEXT);
    };
    let by_spec = |ui: &mut Ui<'_>| spot(ui, "target", 150.0, 130.0, Some(Tooltip::new(TEXT)));
    let mut rects = Vec::new();
    for view in [&by_text as &dyn Fn(&mut Ui<'_>), &by_spec] {
        let mut context = Context::new();
        context.set_now(Duration::ZERO);
        frame(&mut context, |ui| view(ui));
        hover(&mut context, "target");
        context.set_now(TOOLTIP_DELAY);
        frame(&mut context, |ui| view(ui));
        rects.push(bubble(&context, TEXT).expect("shown"));
    }
    assert_eq!(rects[0], rects[1]);
    assert_eq!(Tooltip::from(TEXT), Tooltip::new(TEXT));
    assert_eq!(Tooltip::from(owned.clone()), Tooltip::new(TEXT));
    assert_eq!(Tooltip::from(&owned), Tooltip::new(TEXT));
    assert_eq!(Tooltip::from(Cow::Borrowed(TEXT)), Tooltip::new(TEXT));
    assert_eq!(Tooltip::from(Cow::<str>::Owned(owned)), Tooltip::new(TEXT));
    assert_eq!(Tooltip::default(), Tooltip::new(""));
    assert_ne!(
        Tooltip::new(TEXT).placement(TooltipPlacement::Above),
        Tooltip::new(TEXT)
    );
}

#[test]
fn a_tooltip_style_changes_the_bubble() {
    let style = Style::default()
        .background(Color(10, 20, 30, 255))
        .foreground(Color(200, 100, 50, 255))
        .padding(14.0)
        .radius(2.0)
        .font_size(20.0)
        .font_weight(700)
        .italic(true);
    let mut context = Context::new();
    let plain = |ui: &mut Ui<'_>| spot(ui, "target", 150.0, 130.0, Some(Tooltip::new(TEXT)));
    let styled = |ui: &mut Ui<'_>| {
        spot(
            ui,
            "target",
            150.0,
            130.0,
            Some(Tooltip::new(TEXT).style(style.clone())),
        )
    };
    context.set_now(Duration::ZERO);
    frame(&mut context, plain);
    hover(&mut context, "target");
    context.set_now(TOOLTIP_DELAY);
    frame(&mut context, plain);
    let normal = bubble(&context, TEXT).unwrap();
    frame(&mut context, styled);
    let custom = bubble(&context, TEXT).unwrap();
    assert!(custom.width > normal.width && custom.height > normal.height);
    let primitives = &context.scene().primitives;
    let at = primitives
        .iter()
        .position(
            |primitive| matches!(primitive, Primitive::StyledText { text, .. } if text == TEXT),
        )
        .unwrap();
    let Primitive::Quad { color, radius, .. } = &primitives[at - 1] else {
        panic!("a quad under the text");
    };
    assert_eq!((*color, *radius), (Color(10, 20, 30, 255), 2.0));
    let Primitive::StyledText {
        rect, style, color, ..
    } = &primitives[at]
    else {
        panic!("the text");
    };
    assert_eq!(*color, Color(200, 100, 50, 255));
    assert_eq!(
        (style.font_size, style.weight, style.italic),
        (20.0, 700, true)
    );
    assert_eq!(
        (rect.x - custom.x, rect.y - custom.y),
        (14.0, 14.0),
        "padding"
    );
}

#[test]
fn a_tooltip_style_can_limit_the_width() {
    let long = "A long explanation that wraps onto several lines when it is given room";
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        spot(
            ui,
            "target",
            20.0,
            20.0,
            Some(Tooltip::new(long).style(Style::default().max_width(Length::Px(120.0)))),
        )
    };
    context.set_now(Duration::ZERO);
    frame(&mut context, view);
    hover(&mut context, "target");
    context.set_now(TOOLTIP_DELAY);
    frame(&mut context, view);
    let bubble = bubble(&context, long).unwrap();
    assert!(bubble.width <= 120.0, "{bubble:?}");
    assert!(bubble.height > 40.0, "it wrapped: {bubble:?}");
}

#[test]
fn the_text_is_the_description_and_names_an_icon_button() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("plain", |ui| ui.button("Save"));
        ui.tooltip("Saves the document");
        ui.keyed("spec", |ui| ui.button("Open"));
        ui.tooltip_with(Tooltip::new("Opens one").placement(TooltipPlacement::After));
        ui.keyed("icon", |ui| ui.button("\u{2699}"));
        ui.tooltip_with("Settings");
    });
    let node = |key: &str| context.node(context.find(&[key]).unwrap()).unwrap();
    assert_eq!(
        node("plain").description.as_deref(),
        Some("Saves the document")
    );
    assert_eq!(node("spec").description.as_deref(), Some("Opens one"));
    assert_eq!(node("spec").semantics.label, "Open");
    assert_eq!(node("icon").semantics.label, "Settings");
    assert_eq!(node("icon").description.as_deref(), Some("Settings"));
    assert_eq!(node("icon").semantics.role, Role::Button);
}
