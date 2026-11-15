mod support;

use glassmere_core::app::Action as Msg;
use glassmere_core::{
    Action, Button, Context, DialogOptions, Direction, Event, Key, Length, MonospaceMeasurer, Role,
    Style, TOOLTIP_DELAY, Tooltip, TooltipPlacement, Ui,
};
use glassmere_scene::{Color, Point, Primitive, Rect, Size};
use std::borrow::Cow;
use std::time::Duration;
use support::{Driven, on};

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

// -- Keyboard focus ----------------------------------------------------------

fn press(context: &mut Context, key: Key, shift: bool) -> bool {
    let handled = context.event(Event::KeyDown {
        key,
        shift,
        repeat: false,
    });
    context.event(Event::KeyUp(key));
    handled
}

fn tab(context: &mut Context) {
    press(context, Key::Tab, false);
}

fn focused_key(context: &Context) -> String {
    context
        .focused()
        .and_then(|id| context.details(id))
        .map_or_else(|| "none".into(), |details| details.key)
}

fn toolbar(ui: &mut Ui<'_>) {
    spot(ui, "first", 20.0, 20.0, Some("First help".into()));
    spot(
        ui,
        "second",
        130.0,
        20.0,
        Some(Tooltip::new("Second help").delay(ms(200))),
    );
    spot(ui, "bare", 240.0, 20.0, None);
}

fn toolbar_at(now: Duration) -> Context {
    let mut context = Context::new();
    context.set_now(now);
    frame(&mut context, toolbar);
    context
}

#[test]
fn keyboard_focus_shows_the_tooltip_after_its_delay_against_the_widget() {
    let mut context = toolbar_at(Duration::ZERO);
    // The pointer is somewhere else altogether.
    context.event(Event::PointerMoved(Point { x: 380.0, y: 280.0 }));
    context.set_now(ms(10));
    tab(&mut context);
    assert_eq!(focused_key(&context), "first");
    assert_eq!(context.next_wake(), Some(ms(10) + TOOLTIP_DELAY));
    context.set_now(ms(10) + TOOLTIP_DELAY - ms(1));
    frame(&mut context, toolbar);
    assert!(!shown(&context, "First help"), "not yet due");
    context.set_now(ms(10) + TOOLTIP_DELAY);
    let stats = context.run(WINDOW, &mut MonospaceMeasurer, toolbar);
    assert!(stats.scene_rebuilt);
    let first = widget(&context, "first");
    let bubble = bubble(&context, "First help").expect("shown on focus");
    assert_eq!((bubble.x, bubble.y), (first.x, bottom(first) + GAP));
    assert_eq!(context.next_wake(), None, "nothing pending while shown");
    // The same for a tooltip made from a spec, with its delay.
    let mut context = toolbar_at(Duration::ZERO);
    tab(&mut context);
    tab(&mut context);
    assert_eq!(focused_key(&context), "second");
    assert_eq!(context.next_wake(), Some(ms(200)));
    context.set_now(ms(200));
    frame(&mut context, toolbar);
    assert!(shown(&context, "Second help"));
}

#[test]
fn a_focus_tooltip_follows_its_placement() {
    let view = |ui: &mut Ui<'_>| {
        spot(
            ui,
            "target",
            150.0,
            130.0,
            Some(Tooltip::new(SHORT).placement(TooltipPlacement::After)),
        )
    };
    let mut context = Context::new();
    context.set_now(Duration::ZERO);
    frame(&mut context, view);
    tab(&mut context);
    context.set_now(TOOLTIP_DELAY);
    frame(&mut context, view);
    let target = widget(&context, "target");
    assert_eq!(bubble(&context, SHORT).unwrap().x, right(target) + GAP);
}

#[test]
fn a_focus_tooltip_hides_on_blur_and_when_focus_leaves_the_window() {
    let mut context = toolbar_at(Duration::ZERO);
    tab(&mut context);
    context.set_now(TOOLTIP_DELAY);
    frame(&mut context, toolbar);
    assert!(shown(&context, "First help"));
    // Moving on to a widget without a tooltip takes it away and brings no other.
    tab(&mut context);
    tab(&mut context);
    assert_eq!(focused_key(&context), "bare");
    frame(&mut context, toolbar);
    assert!(!shown(&context, "First help"));
    assert!(!shown(&context, "Second help"));
    assert_eq!(context.next_wake(), None);
    // Shift+Tab back to the first one, and the window losing focus.
    press(&mut context, Key::Tab, true);
    press(&mut context, Key::Tab, true);
    assert_eq!(focused_key(&context), "first");
    context.set_now(TOOLTIP_DELAY * 4);
    frame(&mut context, toolbar);
    assert!(shown(&context, "First help"));
    context.event(Event::FocusLost);
    frame(&mut context, toolbar);
    assert!(!shown(&context, "First help"));
    assert_eq!(context.next_wake(), None);
}

#[test]
fn pointer_focus_shows_no_focus_tooltip() {
    let mut context = toolbar_at(Duration::ZERO);
    // A click focuses the button; the pointer then leaves, so only focus is left.
    hover(&mut context, "first");
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
    assert_eq!(focused_key(&context), "first");
    context.event(Event::PointerLeft);
    assert_eq!(context.next_wake(), None, "nothing to wait for");
    context.set_now(TOOLTIP_DELAY * 4);
    frame(&mut context, toolbar);
    assert!(!shown(&context, "First help"));
    // Nor does focus an assistive technology moved there.
    let second = context.find(&["second"]).unwrap();
    context.action(second, Action::Focus);
    assert_eq!(focused_key(&context), "second");
    assert_eq!(context.next_wake(), None);
    context.set_now(TOOLTIP_DELAY * 8);
    frame(&mut context, toolbar);
    assert!(!shown(&context, "Second help"));
}

#[test]
fn escape_hides_a_focus_tooltip_and_does_nothing_else() {
    #[derive(Default)]
    struct Page {
        open: bool,
        closed: u32,
    }
    let view = |state: &Page, ui: &mut Ui<'_, Msg<Page>>| {
        ui.keyed("opener", |ui| ui.button("Opener"));
        ui.dialog(
            "ask",
            state.open,
            DialogOptions::new("Ask"),
            on(|s: &mut Page, open: bool| {
                s.open = open;
                s.closed += u32::from(!open);
            }),
            |ui| {
                ui.keyed("no", |ui| ui.button("No"));
                ui.keyed("yes", |ui| ui.button("Yes"));
                ui.tooltip("Confirms the question");
            },
        );
    };
    let mut driven = Driven::new(WINDOW, Page::default(), view);
    driven.state_mut().open = true;
    driven.frame();
    // The dialog put focus on its first button; Tab is the keyboard arriving at the next.
    assert_eq!(focused_key(&driven.context), "no");
    tab(&mut driven.context);
    assert_eq!(focused_key(&driven.context), "yes");
    driven.context.set_now(TOOLTIP_DELAY);
    driven.frame();
    assert!(shown(&driven.context, "Confirms the question"));
    // The first Escape takes the tooltip away and leaves the dialog alone.
    assert!(press(&mut driven.context, Key::Escape, false));
    driven.frame();
    assert!(!shown(&driven.context, "Confirms the question"));
    assert!(driven.state().open);
    assert_eq!(driven.state().closed, 0);
    assert_eq!(driven.context.next_wake(), None);
    // The next one closes the dialog, as it always did.
    press(&mut driven.context, Key::Escape, false);
    driven.frame();
    assert!(!driven.state().open);
    assert_eq!(driven.state().closed, 1);
}

#[test]
fn escape_before_the_tooltip_shows_cancels_it_and_a_pointer_tooltip_does_not_take_escape() {
    let mut context = toolbar_at(Duration::ZERO);
    tab(&mut context);
    assert!(context.next_wake().is_some());
    press(&mut context, Key::Escape, false);
    assert_eq!(context.next_wake(), None, "no tooltip is going to appear");
    context.set_now(TOOLTIP_DELAY * 2);
    frame(&mut context, toolbar);
    assert!(!shown(&context, "First help"));
    // A tooltip from hovering is not what Escape is for: the key goes on to what it does.
    hover(&mut context, "second");
    context.set_now(TOOLTIP_DELAY * 4);
    frame(&mut context, toolbar);
    assert!(shown(&context, "Second help"));
    assert!(
        !press(&mut context, Key::Escape, false),
        "nothing took Escape"
    );
}

#[test]
fn a_key_that_changes_the_value_ends_the_focus_tooltip() {
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("other", |ui| ui.button("Other"));
        ui.keyed("name", |ui| ui.text_input("Name", "abc", |_| ()));
        ui.tooltip("Your name");
    };
    let mut context = Context::new();
    context.set_now(Duration::ZERO);
    frame(&mut context, view);
    tab(&mut context);
    tab(&mut context);
    assert_eq!(focused_key(&context), "name");
    // Typing before it shows keeps it from showing at all.
    context.event(Event::Text(glassmere_core::TextEdit::Insert("d".into())));
    assert_eq!(context.next_wake(), None);
    context.set_now(TOOLTIP_DELAY * 2);
    frame(&mut context, view);
    assert!(!shown(&context, "Your name"));
    // Moving the caret is not a change: the tooltip stays once it shows.
    press(&mut context, Key::Tab, true);
    tab(&mut context);
    context.set_now(TOOLTIP_DELAY * 4);
    frame(&mut context, view);
    assert!(shown(&context, "Your name"));
    press(&mut context, Key::ArrowLeft, false);
    frame(&mut context, view);
    assert!(
        shown(&context, "Your name"),
        "the caret moved, the value did not change"
    );
    // Typing hides it.
    context.event(Event::Text(glassmere_core::TextEdit::Insert("e".into())));
    frame(&mut context, view);
    assert!(!shown(&context, "Your name"));
    assert_eq!(context.next_wake(), None);
}

#[test]
fn a_key_that_presses_the_widget_ends_the_focus_tooltip() {
    let mut context = toolbar_at(Duration::ZERO);
    tab(&mut context);
    context.set_now(TOOLTIP_DELAY);
    frame(&mut context, toolbar);
    assert!(shown(&context, "First help"));
    press(&mut context, Key::Enter, false);
    frame(&mut context, toolbar);
    assert!(!shown(&context, "First help"));
}

#[test]
fn only_one_tooltip_is_on_show_and_a_pointer_tooltip_takes_over() {
    let on_show = |context: &Context| {
        ["First help", "Second help"]
            .into_iter()
            .filter(|text| shown(context, text))
            .collect::<Vec<_>>()
    };
    let mut context = toolbar_at(Duration::ZERO);
    tab(&mut context);
    context.set_now(TOOLTIP_DELAY);
    frame(&mut context, toolbar);
    assert_eq!(on_show(&context), ["First help"]);
    // Moving the pointer about does not take it away; only another tooltip does.
    hover(&mut context, "bare");
    frame(&mut context, toolbar);
    assert_eq!(on_show(&context), ["First help"]);
    hover(&mut context, "second");
    let start = context.now();
    assert_eq!(
        context.next_wake(),
        Some(start + ms(200)),
        "the pointer's tooltip is due"
    );
    frame(&mut context, toolbar);
    assert_eq!(on_show(&context), ["First help"]);
    context.set_now(start + ms(200));
    frame(&mut context, toolbar);
    assert_eq!(on_show(&context), ["Second help"]);
    assert_eq!(context.next_wake(), None);
    // The pointer leaving takes its tooltip, and the keyboard's does not come back.
    context.event(Event::PointerLeft);
    context.set_now(start + TOOLTIP_DELAY * 4);
    frame(&mut context, toolbar);
    assert!(on_show(&context).is_empty());
    assert_eq!(context.next_wake(), None);
    // A key takes the pointer's away and moves on to one of the keyboard's.
    hover(&mut context, "second");
    context.set_now(start + TOOLTIP_DELAY * 8);
    frame(&mut context, toolbar);
    assert_eq!(on_show(&context), ["Second help"]);
    tab(&mut context);
    assert_eq!(focused_key(&context), "second");
    frame(&mut context, toolbar);
    assert!(on_show(&context).is_empty());
    context.set_now(start + TOOLTIP_DELAY * 8 + ms(200));
    frame(&mut context, toolbar);
    assert_eq!(on_show(&context), ["Second help"]);
}
