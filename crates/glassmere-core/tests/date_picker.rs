mod support;

use glassmere_core::app::Action as Msg;
use glassmere_core::{
    Action, Date, DatePicker, DateStyle, Direction, Event, Key, LocaleFormat, NodeId, Role,
    Shortcut, ShortcutKey, Time, Ui, Weekday,
};
use glassmere_scene::{Point, Size};
use support::{Driven, center, click, on};

const ROOT: Size = Size::new(500.0, 500.0);

fn date(year: i32, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("a date that exists")
}

struct Form {
    value: Option<Date>,
    changes: Vec<Date>,
    min: Option<Date>,
    max: Option<Date>,
    placeholder: Option<&'static str>,
    style: DateStyle,
    enabled: bool,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            value: None,
            changes: Vec::new(),
            min: None,
            max: None,
            placeholder: None,
            style: DateStyle::default(),
            enabled: true,
        }
    }
}

fn valued(date: Date) -> Form {
    Form {
        value: Some(date),
        ..Form::default()
    }
}

fn view(state: &Form, ui: &mut Ui<'_, Msg<Form>>) {
    let mut picker = DatePicker::new("Departure").date_style(state.style);
    if let Some(min) = state.min {
        picker = picker.min(min);
    }
    if let Some(max) = state.max {
        picker = picker.max(max);
    }
    if let Some(placeholder) = state.placeholder {
        picker = picker.placeholder(placeholder);
    }
    ui.enabled(state.enabled, |ui| {
        ui.keyed("picker", |ui| {
            ui.date_picker(
                picker,
                state.value,
                on(|s: &mut Form, date: Date| {
                    s.value = Some(date);
                    s.changes.push(date);
                }),
            )
        });
    });
    ui.keyed("behind", |ui| ui.button("Behind popup"));
}

fn app(state: Form) -> Driven<Form> {
    Driven::new(ROOT, state, view)
}

fn key(app: &mut Driven<Form>, key: Key) {
    app.context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
    app.frame();
}

fn field(app: &Driven<Form>) -> NodeId {
    app.context.find(&["picker"]).unwrap()
}

fn grid(app: &Driven<Form>) -> NodeId {
    app.context.find(&["picker", "$popup", "$grid"]).unwrap()
}

fn is_open(app: &Driven<Form>) -> bool {
    app.context.find(&["picker", "$popup"]).is_some()
}

fn day(app: &Driven<Form>, row: usize, column: usize) -> NodeId {
    let (week, day) = (format!("w{row}"), format!("d{column}"));
    app.context
        .find(&["picker", "$popup", "$grid", &week, &day])
        .unwrap()
}

fn name(app: &Driven<Form>, id: NodeId) -> String {
    app.context.node(id).unwrap().semantics.label
}

fn title(app: &Driven<Form>) -> String {
    let id = app
        .context
        .find(&["picker", "$popup", "$header", "$title"])
        .unwrap();
    name(app, id)
}

/// The name of the day the keys are on.
fn cursor(app: &Driven<Form>) -> String {
    let id = app.context.node(field(app)).unwrap().active_descendant;
    name(app, id.expect("the field reports the day under the cursor"))
}

fn text(app: &Driven<Form>) -> String {
    app.context.details(field(app)).unwrap().text
}

/// Focuses the field and opens its popup the way a keyboard does.
fn opened(state: Form) -> Driven<Form> {
    let mut app = app(state);
    let field = field(&app);
    assert!(app.context.action(field, Action::Focus));
    app.frame();
    key(&mut app, Key::Enter);
    app.context.event(Event::KeyUp(Key::Enter));
    app.frame();
    assert!(is_open(&app));
    assert_eq!(app.context.focused(), Some(field));
    app
}

fn alt_down() -> Event {
    Event::Shortcut(Shortcut {
        key: ShortcutKey::Named(Key::ArrowDown),
        control: false,
        alt: true,
        shift: false,
        super_key: false,
    })
}

#[test]
fn the_field_shows_the_date_or_the_placeholder() {
    let mut app = app(valued(date(2026, 9, 21)));
    assert_eq!(text(&app), "2026-09-21  ▾");
    let snapshot = app.context.node(field(&app)).unwrap();
    assert_eq!(snapshot.semantics.role, Role::ComboBox);
    assert_eq!(snapshot.semantics.label, "Departure");
    assert_eq!(snapshot.value.as_deref(), Some("2026-09-21"));
    assert_eq!(snapshot.expanded, Some(false));
    assert!(snapshot.semantics.focusable);
    app.state_mut().value = None;
    app.frame();
    assert_eq!(text(&app), "—  ▾");
    assert_eq!(app.context.node(field(&app)).unwrap().value, None);
    let muted = app.context.theme().text_muted;
    assert_eq!(
        app.context.details(field(&app)).unwrap().style.foreground,
        Some(muted)
    );
    app.state_mut().placeholder = Some("Pick a day");
    app.frame();
    assert_eq!(text(&app), "Pick a day  ▾");
}

struct Dotted;

impl LocaleFormat for Dotted {
    fn number(&self, value: f64, decimals: usize) -> String {
        format!("{value:.decimals$}")
    }

    fn parse_number(&self, text: &str) -> Option<f64> {
        text.parse().ok()
    }

    fn date(&self, date: Date, style: DateStyle) -> String {
        match style {
            DateStyle::Short => format!("{}.{}.{}", date.day, date.month, date.year),
            DateStyle::Medium => format!("{}. {}. {}", date.day, date.month, date.year),
            DateStyle::Long => format!("{}. {}. {} (long)", date.day, date.month, date.year),
        }
    }

    fn time(&self, time: Time, _seconds: bool) -> String {
        format!("{}:{}", time.hour, time.minute)
    }
}

#[test]
fn the_field_writes_the_date_in_the_style_and_locale_it_was_given() {
    let mut app = app(valued(date(2026, 9, 21)));
    app.context.set_format(Some(Box::new(Dotted)));
    app.frame();
    assert_eq!(text(&app), "21. 9. 2026  ▾");
    app.state_mut().style = DateStyle::Short;
    app.frame();
    assert_eq!(text(&app), "21.9.2026  ▾");
    // The calendar names its days in full whatever the field shows.
    app.state_mut().style = DateStyle::Medium;
    app.frame();
    key(&mut app, Key::Tab);
    app.context.action(field(&app), Action::Expand);
    app.frame();
    assert_eq!(cursor(&app), "21. 9. 2026 (long)");
}

#[test]
fn a_click_enter_space_and_alt_down_open_the_calendar() {
    let mut app = app(valued(date(2026, 9, 21)));
    assert!(!is_open(&app));
    let point = center(&app.context, &["picker"]);
    click(&mut app.context, point);
    app.frame();
    assert!(is_open(&app));
    assert_eq!(app.context.focused(), Some(field(&app)));
    assert_eq!(
        app.context.node(field(&app)).unwrap().expanded,
        Some(true),
        "the field says it is expanded"
    );
    // A second click closes it.
    click(&mut app.context, point);
    app.frame();
    assert!(!is_open(&app));
    for pressed in [Key::Enter, Key::Space] {
        key(&mut app, pressed);
        assert!(!is_open(&app), "{pressed:?} acts when it is released");
        app.context.event(Event::KeyUp(pressed));
        app.frame();
        assert!(is_open(&app), "{pressed:?}");
        key(&mut app, Key::Escape);
        assert!(!is_open(&app));
    }
    assert!(app.context.event(alt_down()));
    app.frame();
    assert!(is_open(&app));
    assert_eq!(cursor(&app), "2026-09-21");
}

#[test]
fn the_arrows_alone_do_not_open_it_and_alt_down_needs_the_field() {
    let mut app = app(valued(date(2026, 9, 21)));
    assert!(!app.context.event(alt_down()), "nothing has the keyboard");
    let field = field(&app);
    app.context.action(field, Action::Focus);
    app.frame();
    key(&mut app, Key::ArrowDown);
    key(&mut app, Key::ArrowUp);
    assert!(!is_open(&app));
    // Other chords with the arrow are not the one.
    for (control, alt, shift) in [
        (true, true, false),
        (false, false, false),
        (false, true, true),
    ] {
        assert!(!app.context.event(Event::Shortcut(Shortcut {
            key: ShortcutKey::Named(Key::ArrowDown),
            control,
            alt,
            shift,
            super_key: false,
        })));
    }
    assert!(!is_open(&app));
    let behind = app.context.find(&["behind"]).unwrap();
    app.context.action(behind, Action::Focus);
    app.frame();
    assert!(!app.context.event(alt_down()));
    assert!(!is_open(&app));
}

#[test]
fn a_disabled_field_does_not_open() {
    let mut app = app(Form {
        enabled: false,
        ..valued(date(2026, 9, 21))
    });
    let point = center(&app.context, &["picker"]);
    click(&mut app.context, point);
    app.frame();
    assert!(!is_open(&app));
    assert!(!app.context.action(field(&app), Action::Expand));
    assert!(app.context.node(field(&app)).unwrap().semantics.disabled);
}

#[test]
fn the_popup_starts_at_the_value_then_the_minimum() {
    let app = opened(valued(date(2026, 9, 21)));
    assert_eq!(title(&app), "September 2026");
    assert_eq!(cursor(&app), "2026-09-21");
    let app = opened(Form {
        min: Some(date(2031, 3, 10)),
        ..Form::default()
    });
    assert_eq!(title(&app), "March 2031");
    assert_eq!(cursor(&app), "2031-03-10");
    let app = opened(Form::default());
    assert_eq!(title(&app), "January 2000");
}

#[test]
fn a_day_picked_by_pointer_is_sent_and_closes_the_popup() {
    let mut app = app(valued(date(2026, 9, 21)));
    let point = center(&app.context, &["picker"]);
    click(&mut app.context, point);
    app.frame();
    // Tuesday 15 September.
    let day_point = center(&app.context, &["picker", "$popup", "$grid", "w2", "d1"]);
    click(&mut app.context, day_point);
    app.frame();
    assert_eq!(app.state().changes, [date(2026, 9, 15)]);
    assert!(!is_open(&app));
    assert_eq!(app.context.focused(), Some(field(&app)));
    assert_eq!(text(&app), "2026-09-15  ▾");
    // The same day again is still a pick.
    click(&mut app.context, point);
    app.frame();
    let day_point = center(&app.context, &["picker", "$popup", "$grid", "w2", "d1"]);
    click(&mut app.context, day_point);
    app.frame();
    assert_eq!(app.state().changes, [date(2026, 9, 15), date(2026, 9, 15)]);
}

#[test]
fn the_keys_move_in_the_calendar_while_the_field_keeps_the_keyboard() {
    let mut app = opened(valued(date(2026, 9, 29)));
    let field = field(&app);
    key(&mut app, Key::ArrowRight);
    assert_eq!(cursor(&app), "2026-09-30");
    key(&mut app, Key::ArrowDown);
    assert_eq!(cursor(&app), "2026-10-07");
    assert_eq!(title(&app), "October 2026");
    key(&mut app, Key::PageUp);
    assert_eq!(cursor(&app), "2026-09-07");
    key(&mut app, Key::End);
    assert_eq!(cursor(&app), "2026-09-13");
    key(&mut app, Key::Home);
    assert_eq!(cursor(&app), "2026-09-07");
    key(&mut app, Key::ArrowLeft);
    key(&mut app, Key::ArrowUp);
    assert_eq!(cursor(&app), "2026-08-30");
    assert_eq!(app.context.focused(), Some(field));
    assert!(is_open(&app));
    assert!(app.state().changes.is_empty(), "moving picks nothing");
}

#[test]
fn enter_and_space_pick_the_day_and_return_focus_to_the_field() {
    for pick in [Key::Enter, Key::Space] {
        let mut app = opened(valued(date(2026, 9, 21)));
        key(&mut app, Key::ArrowRight);
        key(&mut app, pick);
        assert_eq!(app.state().changes, [date(2026, 9, 22)], "{pick:?}");
        assert!(!is_open(&app));
        assert_eq!(app.context.focused(), Some(field(&app)));
        assert_eq!(text(&app), "2026-09-22  ▾");
        // The key's release does not open it again.
        app.context.event(Event::KeyUp(pick));
        app.frame();
        assert!(!is_open(&app), "{pick:?}");
    }
    // Without a value, the first day the calendar offers is picked.
    let mut app = opened(Form {
        min: Some(date(2026, 4, 9)),
        ..Form::default()
    });
    key(&mut app, Key::Enter);
    assert_eq!(app.state().changes, [date(2026, 4, 9)]);
}

#[test]
fn escape_closes_it_without_a_pick_and_the_field_keeps_the_keyboard() {
    let mut app = opened(valued(date(2026, 9, 21)));
    key(&mut app, Key::ArrowRight);
    key(&mut app, Key::Escape);
    assert!(!is_open(&app));
    assert_eq!(app.context.focused(), Some(field(&app)));
    assert!(app.state().changes.is_empty());
    assert_eq!(text(&app), "2026-09-21  ▾");
    // The month on show is not kept: it starts at the value again.
    key(&mut app, Key::Enter);
    app.context.event(Event::KeyUp(Key::Enter));
    app.frame();
    assert_eq!(cursor(&app), "2026-09-21");
}

#[test]
fn tab_and_a_click_elsewhere_close_it() {
    let mut app = opened(valued(date(2026, 9, 21)));
    key(&mut app, Key::Tab);
    assert!(!is_open(&app));
    assert_eq!(app.context.focused(), app.context.find(&["behind"]));
    assert!(app.state().changes.is_empty());
    let mut app = opened(valued(date(2026, 9, 21)));
    // Far from the popup, on nothing.
    click(&mut app.context, Point { x: 480.0, y: 480.0 });
    app.frame();
    assert!(!is_open(&app));
    assert!(app.state().changes.is_empty());
}

#[test]
fn the_buttons_and_the_popup_itself_leave_it_open_with_the_field_focused() {
    let mut app = opened(valued(date(2026, 12, 15)));
    let next = center(&app.context, &["picker", "$popup", "$header", "$next"]);
    click(&mut app.context, next);
    app.frame();
    assert!(is_open(&app));
    assert_eq!(title(&app), "January 2027");
    assert_eq!(app.context.focused(), Some(field(&app)));
    assert!(app.state().changes.is_empty());
    // The cursor moved with the month.
    assert_eq!(cursor(&app), "2027-01-15");
    // A press on the title is on no control of the calendar.
    let heading = center(&app.context, &["picker", "$popup", "$header", "$title"]);
    click(&mut app.context, heading);
    app.frame();
    assert!(is_open(&app));
    assert_eq!(app.context.focused(), Some(field(&app)));
    key(&mut app, Key::ArrowRight);
    assert_eq!(cursor(&app), "2027-01-16");
}

#[test]
fn days_outside_min_and_max_are_disabled_and_cannot_be_picked() {
    let mut app = opened(Form {
        value: Some(date(2026, 9, 21)),
        min: Some(date(2026, 9, 10)),
        max: Some(date(2026, 9, 25)),
        ..Form::default()
    });
    let disabled = |app: &Driven<Form>, row, column| {
        app.context
            .node(day(app, row, column))
            .unwrap()
            .semantics
            .disabled
    };
    // Sunday 6 September is before the range, Thursday 10 in it, 26 after.
    assert!(disabled(&app, 0, 6));
    assert!(!disabled(&app, 1, 3));
    assert!(disabled(&app, 3, 5));
    let before = day(&app, 0, 6);
    assert!(!app.context.action(before, Action::Activate));
    let point = center(&app.context, &["picker", "$popup", "$grid", "w3", "d5"]);
    click(&mut app.context, point);
    app.frame();
    assert!(is_open(&app), "a disabled day closes nothing");
    assert!(app.state().changes.is_empty());
    for _ in 0..5 {
        key(&mut app, Key::ArrowDown);
    }
    assert_eq!(cursor(&app), "2026-09-25");
    key(&mut app, Key::Enter);
    assert_eq!(app.state().changes, [date(2026, 9, 25)]);
}

#[test]
fn left_and_right_follow_the_reading_direction_in_the_popup() {
    let mut app = opened(valued(date(2026, 9, 21)));
    app.context.set_direction(Direction::Rtl);
    app.frame();
    key(&mut app, Key::ArrowLeft);
    assert_eq!(cursor(&app), "2026-09-22");
    key(&mut app, Key::ArrowRight);
    key(&mut app, Key::ArrowRight);
    assert_eq!(cursor(&app), "2026-09-20");
    // The first day of the week is at the right, and the popup hangs from
    // the right edge of the field, which is where its text starts.
    let first = app.context.node(day(&app, 1, 0)).unwrap().bounds;
    let last = app.context.node(day(&app, 1, 6)).unwrap().bounds;
    assert!(first.x > last.x);
    let popup = app.context.find(&["picker", "$popup"]).unwrap();
    let panel = app.context.node(popup).unwrap().bounds;
    let field = app.context.node(field(&app)).unwrap().bounds;
    assert!((panel.x + panel.width - (field.x + field.width)).abs() < 1.0);
}

#[test]
fn assistive_technology_sees_a_dialog_with_the_calendar_and_the_cursor_as_the_focus() {
    let app = opened(valued(date(2026, 9, 21)));
    let snapshot = app.context.node(field(&app)).unwrap();
    assert_eq!(snapshot.semantics.role, Role::ComboBox);
    assert_eq!(snapshot.expanded, Some(true));
    assert_eq!(snapshot.value.as_deref(), Some("2026-09-21"));
    let panel = app
        .context
        .node(app.context.find(&["picker", "$popup"]).unwrap());
    let panel = panel.unwrap();
    assert!(panel.is_popup);
    assert_eq!(panel.semantics.role, Role::Dialog);
    assert_eq!(panel.semantics.label, "Departure");
    let grid = app.context.node(grid(&app)).unwrap();
    assert_eq!(grid.semantics.role, Role::Grid);
    assert_eq!(grid.semantics.label, "Departure");
    assert!(
        !grid.semantics.focusable,
        "the field is what has the keyboard"
    );
    let active = snapshot.active_descendant.unwrap();
    let cell = app.context.node(active).unwrap();
    assert_eq!(cell.semantics.role, Role::Cell);
    assert_eq!(cell.semantics.label, "2026-09-21");
    assert_eq!(cell.selected, Some(true));
    // Moving the cursor moves it, and the selection stays where it was.
    let mut app = app;
    key(&mut app, Key::ArrowRight);
    let active = app.context.node(field(&app)).unwrap().active_descendant;
    let cell = app.context.node(active.unwrap()).unwrap();
    assert_eq!(cell.semantics.label, "2026-09-22");
    assert_eq!(cell.selected, Some(false));
    let selected = day(&app, 3, 0);
    assert_eq!(app.context.node(selected).unwrap().selected, Some(true));
    key(&mut app, Key::Escape);
    let snapshot = app.context.node(field(&app)).unwrap();
    assert_eq!(snapshot.expanded, Some(false));
    assert_eq!(snapshot.active_descendant, None);
}

#[test]
fn the_popup_stays_inside_the_window() {
    let size = Size::new(500.0, 320.0);
    let mut app = Driven::new(size, valued(date(2026, 9, 21)), view);
    let point = center(&app.context, &["picker"]);
    click(&mut app.context, point);
    app.frame();
    let popup = app.context.find(&["picker", "$popup"]).unwrap();
    let bounds = app.context.node(popup).unwrap().bounds;
    assert!(bounds.y >= 0.0 && bounds.y + bounds.height <= size.height + 0.5);
    assert!(bounds.x >= 0.0 && bounds.x + bounds.width <= size.width + 0.5);
    // The days stay reachable by pointer where the popup is.
    let point = center(&app.context, &["picker", "$popup", "$grid", "w1", "d1"]);
    assert_eq!(
        app.context.hit_test(point),
        Some(day(&app, 1, 1)),
        "the popup is over what lies behind it"
    );
}

#[test]
fn a_value_that_does_not_exist_is_read_as_the_nearest_that_does() {
    let mut app = app(valued(Date {
        year: 2026,
        month: 2,
        day: 31,
    }));
    assert_eq!(text(&app), "2026-02-28  ▾");
    app.state_mut().value = Some(Date {
        year: 2026,
        month: 13,
        day: 0,
    });
    app.frame();
    assert_eq!(text(&app), "2026-12-01  ▾");
    let field = field(&app);
    assert!(app.context.action(field, Action::Expand));
    app.frame();
    assert_eq!(title(&app), "December 2026");
    assert_eq!(
        app.context.node(field).unwrap().value.as_deref(),
        Some("2026-12-01")
    );
}

#[test]
fn the_first_weekday_of_the_picker_orders_its_days() {
    fn sunday(_: &Form, ui: &mut Ui<'_, Msg<Form>>) {
        ui.keyed("picker", |ui| {
            ui.date_picker(
                DatePicker::new("Departure").first_weekday(Weekday::Sunday),
                Some(Date::new(2026, 9, 21).unwrap()),
                |_| act_nothing(),
            )
        });
    }
    fn act_nothing() -> Msg<Form> {
        support::act(|_: &mut Form| ())
    }
    let mut app = Driven::new(ROOT, Form::default(), sunday);
    let field = field(&app);
    assert!(app.context.action(field, Action::Expand));
    app.frame();
    assert_eq!(name(&app, day(&app, 0, 0)), "2026-08-30");
    assert_eq!(name(&app, day(&app, 0, 2)), "2026-09-01");
}
