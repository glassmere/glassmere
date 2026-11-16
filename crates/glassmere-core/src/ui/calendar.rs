//! A month grid to pick a day from.

use super::*;
use crate::calendar::{Month, WEEKS};
use crate::{
    Align, Calendar, Date, DatePicker, DateStyle, Justify, Length, MessageArg, Paint, StateStyles,
    Weekday, app::Handler,
};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;
use glassmere_scene::Color;

const CELL_WIDTH: f32 = 40.0;
const CELL_HEIGHT: f32 = 36.0;
const WEEKDAY_HEIGHT: f32 = 28.0;
const PADDING: f32 = 8.0;
/// The width of the weekday names, the days and the header over them.
const GRID_WIDTH: f32 = 7.0 * CELL_WIDTH;

impl<'a, M: 'static> Ui<'a, M> {
    /// A month to pick a day from, with the days of the months beside it
    /// dimmed. `selected` is marked, and a pick sends `on_pick(day)`.
    /// [`Calendar::new`] carries the label, a style and the days that can be
    /// picked.
    ///
    /// The month on show is the calendar's own to keep: it starts at the
    /// selection, or at today when there is none, or else at the earliest day
    /// that can be picked, and moves with the buttons beside its name and
    /// with the keys. The keys are the arrows by day and week, Page Up and
    /// Page Down by month, Home and End to the ends of the week, and Enter or
    /// Space to pick. Today is marked only when [`Calendar::today`] tells
    /// it, because the framework has no clock.
    #[track_caller]
    pub fn calendar(
        &mut self,
        calendar: impl Into<Calendar>,
        selected: Option<Date>,
        on_pick: impl Fn(Date) -> M + 'static,
    ) -> Response<'a> {
        let calendar = calendar.into();
        let key = self.next_key();
        let owner = self.context.child(self.parent, &key, Kind::Column);
        let month = self.calendar_month(owner, &calendar, selected);
        let style = calendar.style.unwrap_or_else(|| Style {
            padding: PADDING,
            gap: 4.0,
            width: Length::Px(GRID_WIDTH + 2.0 * PADDING),
            ..Style::default()
        });
        let mut child = self.scope(owner);
        child.calendar_body(&calendar.label, &month, Some(Box::new(on_pick)));
        let children = child.elements;
        self.push(Element {
            style,
            children,
            ..Element::new(key, Kind::Column)
        })
    }

    /// A field showing `value`, which opens a calendar in a popup on a click,
    /// Enter, Space or Alt+Down. Picking a day sends `on_change(day)` and
    /// closes the popup, and Escape closes it without. [`DatePicker::new`]
    /// carries the label, a style, the days that can be picked, how the date
    /// is written and what shows while there is none.
    ///
    /// The date is not typed. The calendar in the popup starts at `value`, or
    /// at [`DatePicker::today`] when there is none, and its keys are those of
    /// [`Ui::calendar`], which the field forwards while the popup is open, so
    /// the keyboard stays with the field.
    #[track_caller]
    pub fn date_picker(
        &mut self,
        picker: impl Into<DatePicker>,
        value: Option<Date>,
        on_change: impl Fn(Date) -> M + 'static,
    ) -> Response<'a> {
        let picker = picker.into();
        let key = self.next_key();
        // A date that does not exist is the nearest that does, so that a
        // locale's formatting never sees one.
        let value = value.map(Date::normalized);
        let written = value.map(|date| self.context.format_date(date, picker.date_style));
        let children = self.date_popup(&key, &picker, value);
        let mut style = picker.style.unwrap_or_else(Self::field_style);
        let text = match &written {
            Some(text) => text.as_str(),
            None => {
                style.foreground = style.foreground.or(Some(self.context.theme().text_muted));
                if picker.placeholder.is_empty() {
                    "—"
                } else {
                    picker.placeholder.as_str()
                }
            }
        };
        let response = self.push(Element {
            text: format!("{text}  ▾"),
            style,
            enabled: self.enabled,
            children,
            control: Control::DatePicker {
                label: picker.label,
                value: written,
            },
            ..Element::new(key, Kind::ComboBox)
        });
        self.set_handler(Handler::Map(Box::new(move |update| match update {
            ControlUpdate::Date(date) => Some(on_change(date)),
            _ => None,
        })));
        response
    }

    /// The popup of the date picker `key` while it is open: a panel holding a
    /// calendar, which leaves a pick to the field.
    #[track_caller]
    fn date_popup(
        &mut self,
        key: &WidgetKey,
        picker: &DatePicker,
        value: Option<Date>,
    ) -> Vec<Element> {
        let owner = self.context.child(self.parent, key, Kind::ComboBox);
        if !self.enabled
            || owner
                .and_then(|owner| self.context.popup_of(owner))
                .is_none()
        {
            return Vec::new();
        }
        let panel = owner.and_then(|owner| {
            self.context
                .child(Some(owner), &WidgetKey::named("$popup"), Kind::Popup)
        });
        let calendar = Calendar {
            label: picker.label.clone(),
            style: None,
            min: picker.min,
            max: picker.max,
            first_weekday: picker.first_weekday,
            today: picker.today,
        };
        let month = self.calendar_month(panel, &calendar, value);
        let mut child = self.scope(owner);
        child.group(
            WidgetKey::named("$popup"),
            Kind::Popup,
            Style {
                width: Length::Px(GRID_WIDTH + 2.0 * PADDING),
                padding: PADDING,
                gap: 4.0,
                ..Style::default()
            },
            |ui| ui.calendar_body(&picker.label, &month, None),
        );
        child.elements[0].control = Control::CalendarPopup {
            label: picker.label.clone(),
        };
        child.elements
    }

    /// The month a calendar shows, from what it kept in `container`.
    fn calendar_month(
        &self,
        container: Option<NodeId>,
        calendar: &Calendar,
        selected: Option<Date>,
    ) -> Month {
        let grid = container
            .and_then(|container| {
                self.context
                    .child(Some(container), &WidgetKey::named("$grid"), Kind::Calendar)
            })
            .and_then(|grid| self.context.calendar_view(grid));
        Month::new(
            grid,
            selected.map(Date::normalized),
            calendar.today.map(Date::normalized),
            calendar.min.map(Date::normalized),
            calendar.max.map(Date::normalized),
            calendar.first_weekday.unwrap_or(Weekday::Monday),
        )
    }

    /// The header and the days of `month`, as children of the container this
    /// is the scope of. `on_pick` is what a pick sends; a calendar without
    /// one is in the popup of a date picker, which is told of a pick instead.
    #[track_caller]
    pub(super) fn calendar_body(
        &mut self,
        label: &str,
        month: &Month,
        on_pick: Option<Box<dyn Fn(Date) -> M>>,
    ) {
        let context = self.context;
        let theme = context.theme();
        let rtl = context.direction() == crate::Direction::Rtl;
        let shown = month.shown();
        let title = context.tr(
            "glassmere-calendar-title",
            &[
                (
                    "month",
                    MessageArg::Text(context.tr(&format!("glassmere-month-{}", shown.month), &[])),
                ),
                // Text, so that the year is not grouped as a number.
                ("year", MessageArg::Text(shown.year.to_string())),
            ],
        );
        let cell = |height| Style {
            width: Length::Px(CELL_WIDTH),
            height: Length::Px(height),
            align_items: Align::Center,
            justify_content: Justify::Center,
            ..Style::default()
        };
        self.group(
            WidgetKey::named("$header"),
            Kind::Row,
            Style {
                width: Length::Px(GRID_WIDTH),
                gap: 0.0,
                align_items: Align::Center,
                ..Style::default()
            },
            |ui| {
                // The previous month is toward the start, which is on the
                // right in a right-to-left calendar.
                let (previous, next) = if rtl { ("›", "‹") } else { ("‹", "›") };
                for (key, arrow, months, name) in [
                    ("$previous", previous, -1, "glassmere-calendar-previous"),
                    ("$next", next, 1, "glassmere-calendar-next"),
                ] {
                    if months == 1 {
                        ui.push(Element {
                            text: title.clone(),
                            style: Style {
                                grow: 1.0,
                                height: Length::Px(CELL_HEIGHT),
                                align_items: Align::Center,
                                justify_content: Justify::Center,
                                font_weight: Some(600),
                                ..Style::default()
                            },
                            ..Element::new(WidgetKey::named("$title"), Kind::Label)
                        });
                    }
                    ui.push(Element {
                        text: arrow.into(),
                        style: Style {
                            radius: 8.0,
                            ..cell(CELL_HEIGHT)
                        },
                        enabled: month.can_step(months),
                        tooltip: Some(Box::new(crate::Tooltip::new(context.tr(name, &[])))),
                        control: Control::CalendarStep { months },
                        ..Element::new(WidgetKey::named(key), Kind::Button)
                    });
                }
            },
        );
        let selection = theme.selection;
        let grid_id = context.child(self.parent, &WidgetKey::named("$grid"), Kind::Calendar);
        let mut grid = self.scope(grid_id);
        grid.group(
            WidgetKey::named("$weekdays"),
            Kind::Row,
            Style {
                gap: 0.0,
                ..Style::default()
            },
            |ui| {
                for column in 0..7 {
                    let weekday = (month.first.index() + column) % 7 + 1;
                    ui.push(Element {
                        text: context.tr(&format!("glassmere-weekday-short-{weekday}"), &[]),
                        style: Style {
                            foreground: Some(theme.text_muted),
                            font_size: Some(13.0),
                            ..cell(WEEKDAY_HEIGHT)
                        },
                        control: Control::CalendarWeekday {
                            name: context.tr(&format!("glassmere-weekday-{weekday}"), &[]),
                            column,
                        },
                        ..Element::new(WidgetKey::named(format!("c{column}")), Kind::Label)
                    });
                }
            },
        );
        grid.set_control(Control::CalendarRow { index: 0 });
        for row in 0..WEEKS {
            grid.group(
                WidgetKey::named(format!("w{row}")),
                Kind::Row,
                Style {
                    gap: 0.0,
                    ..Style::default()
                },
                |ui| {
                    for column in 0..7 {
                        let date = month.date(row, column);
                        let selected = month.selected == Some(date);
                        let held = selected.then_some(Paint::background(selection));
                        ui.push(Element {
                            text: date.day.to_string(),
                            style: Style {
                                radius: 8.0,
                                background: Some(if selected {
                                    selection
                                } else {
                                    Color::TRANSPARENT
                                }),
                                // The days of the months beside it are dimmed.
                                foreground: (date.month != shown.month).then_some(theme.text_muted),
                                font_weight: selected.then_some(700),
                                // A pointer over the selection leaves it as it is,
                                // and a day that cannot be picked is dimmed, not boxed.
                                states: Some(Box::new(match held {
                                    Some(held) => StateStyles {
                                        hover: held,
                                        pressed: held,
                                        disabled: held,
                                        ..StateStyles::default()
                                    },
                                    None => StateStyles {
                                        disabled: Paint::background(Color::TRANSPARENT),
                                        ..StateStyles::default()
                                    },
                                })),
                                ..cell(CELL_HEIGHT)
                            },
                            enabled: month.enabled(date),
                            control: Control::CalendarDay {
                                date,
                                name: context.format_date(date, DateStyle::Long),
                                row,
                                column,
                                selected,
                            },
                            ..Element::new(WidgetKey::named(format!("d{column}")), Kind::Button)
                        });
                    }
                },
            );
            grid.set_control(Control::CalendarRow { index: row + 1 });
        }
        let children = grid.elements;
        self.push(Element {
            style: Style {
                width: Length::Px(GRID_WIDTH),
                gap: 0.0,
                ..Style::default()
            },
            children,
            control: Control::CalendarGrid {
                label: label.into(),
                month: month.clone(),
                popup: on_pick.is_none(),
            },
            ..Element::new(WidgetKey::named("$grid"), Kind::Calendar)
        });
        if let Some(on_pick) = on_pick {
            self.set_handler(Handler::Map(Box::new(move |update| match update {
                ControlUpdate::Date(date) => Some(on_pick(date)),
                _ => None,
            })));
        }
    }
}
