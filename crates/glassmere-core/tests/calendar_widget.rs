mod support;

use glassmere_core::app::Action as Msg;
use glassmere_core::{
    Action, Calendar, Context, Date, Direction, Event, Key, MessageArg, Messages, NodeId, Role, Ui,
    Weekday,
};
use glassmere_scene::Size;
use support::{Driven, center, click, on};

const ROOT: Size = Size::new(500.0, 500.0);

fn date(year: i32, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("a date that exists")
}

#[derive(Default)]
struct Dates {
    selected: Option<Date>,
    picks: Vec<Date>,
    min: Option<Date>,
    max: Option<Date>,
    first: Option<Weekday>,
}

fn view(state: &Dates, ui: &mut Ui<'_, Msg<Dates>>) {
    let mut calendar = Calendar::new("Departure");
    if let Some(min) = state.min {
        calendar = calendar.min(min);
    }
    if let Some(max) = state.max {
        calendar = calendar.max(max);
    }
    if let Some(first) = state.first {
        calendar = calendar.first_weekday(first);
    }
    ui.keyed("calendar", |ui| {
        ui.calendar(
            calendar,
            state.selected,
            on(|s: &mut Dates, date: Date| {
                s.selected = Some(date);
                s.picks.push(date);
            }),
        )
    });
}

fn app(state: Dates) -> Driven<Dates> {
    Driven::new(ROOT, state, view)
}

fn selected(date: Date) -> Dates {
    Dates {
        selected: Some(date),
        ..Dates::default()
    }
}

fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
}

fn grid(app: &Driven<Dates>) -> NodeId {
    app.context.find(&["calendar", "$grid"]).unwrap()
}

/// The day in this row and column of the grid.
fn day(app: &Driven<Dates>, row: usize, column: usize) -> NodeId {
    let (week, day) = (format!("w{row}"), format!("d{column}"));
    app.context
        .find(&["calendar", "$grid", &week, &day])
        .unwrap()
}

fn name(app: &Driven<Dates>, id: NodeId) -> String {
    app.context.node(id).unwrap().semantics.label
}

fn title(app: &Driven<Dates>) -> String {
    let id = app
        .context
        .find(&["calendar", "$header", "$title"])
        .unwrap();
    name(app, id)
}

/// The name of the day the keys are on.
fn cursor(app: &Driven<Dates>) -> String {
    let id = app.context.node(grid(app)).unwrap().active_descendant;
    name(app, id.expect("the grid reports the day under the cursor"))
}

/// The name of the day that is marked selected.
fn marked(app: &Driven<Dates>) -> String {
    let id = (0..6)
        .flat_map(|row| (0..7).map(move |column| (row, column)))
        .map(|(row, column)| day(app, row, column))
        .find(|id| app.context.node(*id).unwrap().selected == Some(true))
        .expect("a day is selected");
    name(app, id)
}

fn disabled(app: &Driven<Dates>, id: NodeId) -> bool {
    app.context.node(id).unwrap().semantics.disabled
}

fn focused(state: Dates) -> Driven<Dates> {
    let mut app = app(state);
    let grid = grid(&app);
    assert!(app.context.action(grid, Action::Focus));
    app.frame();
    assert_eq!(app.context.focused(), Some(grid));
    app
}

fn press(app: &mut Driven<Dates>, keys: &[Key]) {
    for pressed in keys {
        key(&mut app.context, *pressed);
        app.frame();
    }
}

#[test]
fn it_starts_at_the_selection_then_the_minimum_then_a_fixed_month() {
    assert_eq!(title(&app(selected(date(2026, 9, 21)))), "September 2026");
    let from_min = Dates {
        min: Some(date(2031, 3, 10)),
        ..Dates::default()
    };
    assert_eq!(title(&app(from_min)), "March 2031");
    assert_eq!(title(&app(Dates::default())), "January 2000");
    // Whatever it starts from is kept to the days that can be picked.
    let capped = Dates {
        max: Some(date(1999, 6, 15)),
        ..Dates::default()
    };
    assert_eq!(title(&app(capped)), "June 1999");
}

#[test]
fn it_lays_out_six_weeks_from_the_first_weekday_with_the_neighbours_dimmed() {
    let mut app = app(selected(date(2026, 9, 21)));
    // 1 September 2026 is a Tuesday.
    assert_eq!(name(&app, day(&app, 0, 0)), "2026-08-31");
    assert_eq!(name(&app, day(&app, 0, 1)), "2026-09-01");
    assert_eq!(name(&app, day(&app, 5, 6)), "2026-10-11");
    let muted = app.context.theme().text_muted;
    let colour = |app: &Driven<Dates>, row, column| {
        app.context
            .details(day(app, row, column))
            .unwrap()
            .style
            .foreground
    };
    assert_eq!(colour(&app, 0, 0), Some(muted));
    assert_eq!(colour(&app, 0, 1), None);
    assert_eq!(colour(&app, 5, 6), Some(muted));
    // Weeks that start on Sunday move every day one column over.
    app.state_mut().first = Some(Weekday::Sunday);
    app.frame();
    assert_eq!(name(&app, day(&app, 0, 0)), "2026-08-30");
    assert_eq!(name(&app, day(&app, 0, 2)), "2026-09-01");
    let names: Vec<_> = (0..7)
        .map(|column| {
            let column = format!("c{column}");
            let id = app
                .context
                .find(&["calendar", "$grid", "$weekdays", &column])
                .unwrap();
            let snapshot = app.context.node(id).unwrap();
            assert_eq!(snapshot.semantics.role, Role::ColumnHeader);
            snapshot.semantics.label
        })
        .collect();
    assert_eq!(
        names,
        [
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday"
        ]
    );
}

#[test]
fn the_buttons_show_other_months_across_the_new_year() {
    let mut app = app(selected(date(2026, 12, 15)));
    let next = center(&app.context, &["calendar", "$header", "$next"]);
    click(&mut app.context, next);
    app.frame();
    assert_eq!(title(&app), "January 2027");
    assert!(
        app.state().picks.is_empty(),
        "showing a month picks nothing"
    );
    let previous = center(&app.context, &["calendar", "$header", "$previous"]);
    click(&mut app.context, previous);
    app.frame();
    click(&mut app.context, previous);
    app.frame();
    assert_eq!(title(&app), "November 2026");
    // The calendar has the keyboard after the presses, and its cursor is on
    // the same day of the month on show.
    assert_eq!(app.context.focused(), Some(grid(&app)));
    assert_eq!(cursor(&app), "2026-11-15");
}

#[test]
fn the_month_on_show_stays_until_the_application_selects_another_day() {
    let mut app = app(selected(date(2026, 9, 21)));
    let next = center(&app.context, &["calendar", "$header", "$next"]);
    click(&mut app.context, next);
    app.frame();
    // The same selection again is not news.
    app.state_mut().selected = Some(date(2026, 9, 21));
    app.frame();
    assert_eq!(title(&app), "October 2026");
    app.state_mut().selected = Some(date(2027, 2, 3));
    app.frame();
    assert_eq!(title(&app), "February 2027");
    assert_eq!(marked(&app), "2027-02-03");
}

#[test]
fn the_arrows_move_by_day_and_week_and_cross_months() {
    let mut app = focused(selected(date(2026, 9, 29)));
    assert_eq!(cursor(&app), "2026-09-29");
    press(&mut app, &[Key::ArrowRight]);
    assert_eq!(cursor(&app), "2026-09-30");
    press(&mut app, &[Key::ArrowRight]);
    assert_eq!(cursor(&app), "2026-10-01");
    assert_eq!(title(&app), "October 2026");
    press(&mut app, &[Key::ArrowLeft, Key::ArrowLeft]);
    assert_eq!(cursor(&app), "2026-09-29");
    assert_eq!(title(&app), "September 2026");
    press(&mut app, &[Key::ArrowDown]);
    assert_eq!(cursor(&app), "2026-10-06");
    press(&mut app, &[Key::ArrowUp, Key::ArrowUp]);
    assert_eq!(cursor(&app), "2026-09-22");
    assert!(app.state().picks.is_empty(), "moving picks nothing");
}

#[test]
fn the_page_keys_move_by_month_keeping_the_day_where_it_exists() {
    let mut app = focused(selected(date(2027, 1, 31)));
    press(&mut app, &[Key::PageDown]);
    assert_eq!(cursor(&app), "2027-02-28");
    assert_eq!(title(&app), "February 2027");
    press(&mut app, &[Key::PageDown]);
    assert_eq!(cursor(&app), "2027-03-28");
    press(&mut app, &[Key::PageUp, Key::PageUp, Key::PageUp]);
    assert_eq!(cursor(&app), "2026-12-28");
    assert_eq!(title(&app), "December 2026");
}

#[test]
fn home_and_end_go_to_the_ends_of_the_week() {
    // 23 September 2026 is a Wednesday.
    let mut app = focused(selected(date(2026, 9, 23)));
    press(&mut app, &[Key::Home]);
    assert_eq!(cursor(&app), "2026-09-21");
    press(&mut app, &[Key::End]);
    assert_eq!(cursor(&app), "2026-09-27");
    // A week that starts on Sunday ends on Saturday.
    app.state_mut().first = Some(Weekday::Sunday);
    app.frame();
    press(&mut app, &[Key::Home]);
    assert_eq!(cursor(&app), "2026-09-27");
    press(&mut app, &[Key::End]);
    assert_eq!(cursor(&app), "2026-10-03");
    assert_eq!(title(&app), "October 2026");
}

#[test]
fn enter_and_space_pick_the_day_under_the_cursor() {
    let mut app = focused(selected(date(2026, 9, 21)));
    press(&mut app, &[Key::ArrowRight, Key::Enter]);
    assert_eq!(app.state().picks, [date(2026, 9, 22)]);
    assert_eq!(marked(&app), "2026-09-22");
    press(&mut app, &[Key::ArrowDown, Key::Space]);
    assert_eq!(app.state().picks, [date(2026, 9, 22), date(2026, 9, 29)]);
    // Without a selection the cursor still has a day to pick.
    let mut app = focused(Dates {
        min: Some(date(2026, 4, 9)),
        ..Dates::default()
    });
    press(&mut app, &[Key::Enter]);
    assert_eq!(app.state().picks, [date(2026, 4, 9)]);
}

#[test]
fn a_day_is_picked_by_pointer_and_the_month_follows_a_dimmed_one() {
    let mut app = app(selected(date(2026, 9, 21)));
    let point = center(&app.context, &["calendar", "$grid", "w2", "d2"]);
    click(&mut app.context, point);
    app.frame();
    assert_eq!(app.state().picks, [date(2026, 9, 16)]);
    assert_eq!(app.context.focused(), Some(grid(&app)));
    assert_eq!(marked(&app), "2026-09-16");
    assert_eq!(cursor(&app), "2026-09-16");
    // A day of the month before, which is dimmed, is a day like the others.
    let point = center(&app.context, &["calendar", "$grid", "w0", "d0"]);
    click(&mut app.context, point);
    app.frame();
    assert_eq!(app.state().picks.last(), Some(&date(2026, 8, 31)));
    assert_eq!(title(&app), "August 2026");
    // A screen reader activates a day the way a click does.
    let id = day(&app, 1, 3);
    assert!(app.context.action(id, Action::Activate));
    app.frame();
    assert_eq!(app.state().picks.last(), Some(&date(2026, 8, 6)));
}

#[test]
fn days_outside_the_range_are_disabled_and_the_cursor_stops_at_its_ends() {
    let mut app = focused(Dates {
        selected: Some(date(2026, 9, 21)),
        min: Some(date(2026, 9, 10)),
        max: Some(date(2026, 9, 25)),
        ..Dates::default()
    });
    // Sunday 6 September is before the range, Thursday 10 in it, 26 after.
    assert!(disabled(&app, day(&app, 0, 6)));
    assert!(!disabled(&app, day(&app, 1, 3)));
    assert!(!disabled(&app, day(&app, 3, 4)));
    assert!(disabled(&app, day(&app, 3, 5)));
    let before = day(&app, 0, 6);
    assert!(!app.context.action(before, Action::Activate));
    // No other month has a day to pick.
    for button in ["$previous", "$next"] {
        let id = app.context.find(&["calendar", "$header", button]).unwrap();
        assert!(disabled(&app, id), "{button}");
    }
    press(&mut app, &[Key::ArrowUp, Key::ArrowUp, Key::ArrowUp]);
    assert_eq!(cursor(&app), "2026-09-10");
    press(&mut app, &[Key::PageUp, Key::Home]);
    assert_eq!(cursor(&app), "2026-09-10");
    press(&mut app, &[Key::ArrowDown, Key::ArrowDown, Key::ArrowDown]);
    assert_eq!(cursor(&app), "2026-09-25");
    press(&mut app, &[Key::PageDown, Key::End]);
    assert_eq!(cursor(&app), "2026-09-25");
    assert_eq!(title(&app), "September 2026");
    assert!(app.state().picks.is_empty());
    press(&mut app, &[Key::Enter]);
    assert_eq!(app.state().picks, [date(2026, 9, 25)]);
}

#[test]
fn a_month_with_days_in_range_can_be_stepped_to() {
    let mut app = app(Dates {
        selected: Some(date(2026, 9, 21)),
        min: Some(date(2026, 8, 31)),
        max: Some(date(2026, 10, 1)),
        ..Dates::default()
    });
    for button in ["$previous", "$next"] {
        let id = app.context.find(&["calendar", "$header", button]).unwrap();
        assert!(!disabled(&app, id), "{button}");
    }
    let previous = center(&app.context, &["calendar", "$header", "$previous"]);
    click(&mut app.context, previous);
    app.frame();
    assert_eq!(title(&app), "August 2026");
    // Only 31 August of it can be picked, so that is where the cursor lands.
    assert_eq!(cursor(&app), "2026-08-31");
    let id = app
        .context
        .find(&["calendar", "$header", "$previous"])
        .unwrap();
    assert!(disabled(&app, id));
}

#[test]
fn left_and_right_follow_the_reading_direction() {
    let mut app = focused(selected(date(2026, 9, 21)));
    app.context.set_direction(Direction::Rtl);
    app.frame();
    // The first day of the week is at the right.
    let first = app.context.node(day(&app, 1, 0)).unwrap().bounds;
    let last = app.context.node(day(&app, 1, 6)).unwrap().bounds;
    assert!(first.x > last.x);
    // The previous month is toward the start, and points that way.
    let previous = app
        .context
        .find(&["calendar", "$header", "$previous"])
        .unwrap();
    let next = app.context.find(&["calendar", "$header", "$next"]).unwrap();
    assert_eq!(app.context.details(previous).unwrap().text, "›");
    assert_eq!(app.context.details(next).unwrap().text, "‹");
    let (previous, next) = (
        app.context.node(previous).unwrap().bounds,
        app.context.node(next).unwrap().bounds,
    );
    assert!(previous.x > next.x);
    assert_eq!(previous.x, first.x);
    press(&mut app, &[Key::ArrowLeft]);
    assert_eq!(cursor(&app), "2026-09-22");
    press(&mut app, &[Key::ArrowRight, Key::ArrowRight]);
    assert_eq!(cursor(&app), "2026-09-20");
    app.context.set_direction(Direction::Ltr);
    app.frame();
    press(&mut app, &[Key::ArrowLeft]);
    assert_eq!(cursor(&app), "2026-09-19");
}

#[test]
fn assistive_technology_sees_a_grid_of_named_cells_with_the_selection_marked() {
    let app = app(selected(date(2026, 9, 21)));
    let snapshot = app.context.node(grid(&app)).unwrap();
    assert_eq!(snapshot.semantics.role, Role::Grid);
    assert_eq!(snapshot.semantics.label, "Departure");
    assert!(snapshot.semantics.focusable);
    assert_eq!(snapshot.collection.unwrap().columns, Some(7));
    // The weekday names and six weeks.
    assert_eq!(snapshot.children.len(), 7);
    let mut selected = Vec::new();
    for row in 0..6 {
        let week = app.context.node(snapshot.children[row + 1]).unwrap();
        assert_eq!(week.semantics.role, Role::Row);
        for column in 0..7 {
            let cell = app.context.node(day(&app, row, column)).unwrap();
            assert_eq!(cell.semantics.role, Role::Cell);
            assert!(!cell.semantics.focusable, "the grid has the keyboard");
            assert_eq!(cell.collection.unwrap().column, Some(column));
            if cell.selected == Some(true) {
                selected.push(cell.semantics.label.clone());
            } else {
                assert_eq!(cell.selected, Some(false));
            }
        }
    }
    assert_eq!(selected, ["2026-09-21"]);
    // Nothing is selected, but the cursor has a day.
    let app = focused(Dates::default());
    assert_eq!(cursor(&app), "2000-01-01");
}

struct Czech;

impl Messages for Czech {
    fn locale(&self) -> &str {
        "cs"
    }

    fn text(&self, key: &str, args: &[(&str, MessageArg)]) -> Option<String> {
        let arg = |name: &str| match args.iter().find(|(known, _)| *known == name) {
            Some((_, MessageArg::Text(text))) => text.clone(),
            _ => String::new(),
        };
        Some(match key {
            "glassmere-calendar-title" => format!("{} {}", arg("month"), arg("year")),
            "glassmere-month-9" => "září".into(),
            "glassmere-weekday-1" => "pondělí".into(),
            "glassmere-weekday-short-1" => "po".into(),
            "glassmere-calendar-next" => "Další měsíc".into(),
            _ => return None,
        })
    }
}

#[test]
fn month_and_weekday_names_come_from_the_message_catalog() {
    let mut app = app(selected(date(2026, 9, 21)));
    app.context.set_messages(Some(Box::new(Czech)));
    app.frame();
    assert_eq!(title(&app), "září 2026");
    let monday = app
        .context
        .find(&["calendar", "$grid", "$weekdays", "c0"])
        .unwrap();
    assert_eq!(name(&app, monday), "pondělí");
    assert_eq!(app.context.details(monday).unwrap().text, "po");
    // A name the catalog does not have is the built-in English.
    let tuesday = app
        .context
        .find(&["calendar", "$grid", "$weekdays", "c1"])
        .unwrap();
    assert_eq!(name(&app, tuesday), "Tuesday");
    // The arrow is a symbol, so the tooltip is what names the button.
    let next = app.context.find(&["calendar", "$header", "$next"]).unwrap();
    assert_eq!(app.context.details(next).unwrap().text, "›");
    assert_eq!(name(&app, next), "Další měsíc");
}

#[test]
fn a_date_that_does_not_exist_is_read_as_the_nearest_that_does() {
    let mut app = app(Dates {
        selected: Some(Date {
            year: 2026,
            month: 2,
            day: 31,
        }),
        min: Some(Date {
            year: 2026,
            month: 0,
            day: 0,
        }),
        max: Some(Date {
            year: 2026,
            month: 13,
            day: 99,
        }),
        ..Dates::default()
    });
    assert_eq!(title(&app), "February 2026");
    assert_eq!(marked(&app), "2026-02-28");
    // The range is the whole of the year.
    for _ in 0..3 {
        let next = center(&app.context, &["calendar", "$header", "$next"]);
        click(&mut app.context, next);
        app.frame();
    }
    assert_eq!(title(&app), "May 2026");
    assert!(!disabled(&app, day(&app, 2, 2)));
}
