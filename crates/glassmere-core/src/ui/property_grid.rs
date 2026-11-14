use super::collections::realized_rows;
use super::controls::FieldExtra;
use super::*;
use crate::app::Handler;
use crate::controls::ControlUpdate;
use crate::{
    Align, Axis, Button, ComboBox, Length, NumberOptions, PropertyGridOptions, PropertyGridState,
    PropertyMsg, SplitOptions, TextInput, VirtualListOptions,
};
use glassmere_scene::Color;
use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;

/// Gap between the name and the value, and the width of the header's divider,
/// so the columns line up with the header.
const COLUMN_GAP: f32 = 6.0;

/// Height of every row and group header, and the gap between them. They are
/// alike so that a scrolled grid can tell which rows are in view from the
/// scroll offset alone.
const ROW_HEIGHT: f32 = 44.0;
const ROW_GAP: f32 = 4.0;

fn contains(text: &str, filter: &str) -> bool {
    text.to_lowercase().contains(filter)
}

fn hex(color: Color) -> String {
    let Color(r, g, b, a) = color;
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

fn parse_hex(text: &str) -> Option<Color> {
    let digits = text.trim().trim_start_matches('#');
    if !(digits.len() == 6 || digits.len() == 8) || !digits.is_ascii() {
        return None;
    }
    let channel = |index: usize| u8::from_str_radix(&digits[index..index + 2], 16).ok();
    Some(Color(
        channel(0)?,
        channel(2)?,
        channel(4)?,
        if digits.len() == 8 { channel(6)? } else { 255 },
    ))
}

/// Share of the width the content takes in a scrolling grid, which leaves the
/// scrollbar a margin of its own instead of covering the editors' ends.
/// A percentage because styles have no one-sided padding.
const BESIDE_SCROLLBAR: f32 = 0.97;

fn fill() -> Style {
    Style {
        width: Length::Percent(1.0),
        ..Ui::<()>::field_style()
    }
}

/// The default a row takes, if one was given for it; a default of another
/// type is a misuse, and the row shows without a reset.
fn take_default_from<T: Any, M: 'static>(
    default: &mut Option<(Box<dyn Any>, &'static str)>,
    ui: &mut Ui<'_, M>,
    name: &str,
) -> Option<T> {
    let (value, given) = default.take()?;
    match value.downcast::<T>() {
        Ok(value) => Some(*value),
        Err(_) => {
            ui.misuse(format!(
                "property `{name}` holds a {} but its default is a {given}",
                std::any::type_name::<T>()
            ));
            None
        }
    }
}

/// The rows of a scrolling grid that exist. Rows and headers are counted as
/// the view declares them, in order, and only those in `range` become nodes;
/// the rest are stood in for by a spacer of their height, as in a virtual list.
struct Window {
    range: std::ops::Range<usize>,
    /// Rows and headers counted so far, in view or not.
    counted: Cell<usize>,
    offset: f32,
    height: f32,
}

impl Window {
    /// Whether the scroll offset the window was made from lies past the end
    /// of what was counted, which happens when a filter or a collapsed group
    /// shortens the grid: nothing is in view, and the next declaration is
    /// made from the offset the layout clamped it to.
    fn past_end(&self) -> bool {
        let content = (self.counted.get() as f32 * (ROW_HEIGHT + ROW_GAP) - ROW_GAP).max(0.0);
        self.offset > (content - self.height).max(0.0)
    }
}

/// Declares the rows of a property grid whose values are messages; see
/// [`Ui::property_grid`]. Each row shows the value it is given and sends
/// the message of its editor, so the view keeps no state of its own.
pub struct PropertyGrid<'a, 'u, M: 'static = ()> {
    ui: &'a mut Ui<'u, M>,
    state: &'a PropertyGridState,
    /// Sends the grid's own changes and the value changes made by path.
    send: Rc<dyn Fn(PropertyMsg) -> M>,
    /// Names of the enclosing groups.
    path: Vec<String>,
    /// An enclosing group matched the filter, so all its rows show.
    shown_by_group: bool,
    default: Option<(Box<dyn Any>, &'static str)>,
    /// Which rows exist; `None` declares them all.
    window: Option<&'a Window>,
    /// Rows and headers passed over since the last one declared here.
    skipped: usize,
    /// Whether anything was declared here.
    shown: bool,
    rows: usize,
}

impl<M: 'static> PropertyGrid<'_, '_, M> {
    fn filter(&self) -> String {
        self.state.filter.trim().to_lowercase()
    }

    fn finish(&mut self) {
        if self.default.take().is_some() {
            self.ui
                .misuse("PropertyGrid::default was not followed by a row".to_owned());
        }
    }

    /// Counts the next row or header, and says whether it is declared. The
    /// rows passed over before it are declared as one spacer first.
    fn take_slot(&mut self) -> bool {
        if let Some(window) = self.window {
            let index = window.counted.get();
            window.counted.set(index + 1);
            if !window.range.contains(&index) {
                self.skipped += 1;
                return false;
            }
            self.flush("$before");
        }
        self.shown = true;
        true
    }

    /// Declares the rows passed over as a spacer, which the gaps around it
    /// complete to the height they would have taken.
    fn flush(&mut self, key: &str) {
        if self.skipped > 0 {
            let height = self.skipped as f32 * (ROW_HEIGHT + ROW_GAP) - ROW_GAP;
            self.ui.label_at(
                key,
                "",
                Style {
                    height: Length::Px(height),
                    ..Style::default()
                },
            );
            self.skipped = 0;
        }
    }

    fn path_of(&self, name: &str) -> String {
        let mut path = self.path.clone();
        path.push(name.to_owned());
        path.join("/")
    }

    /// Gives the next row a reset button that sends `value` again; it is
    /// enabled while the property differs. The type must be the row's type.
    pub fn default<T: Any>(&mut self, value: T) -> &mut Self {
        self.default = Some((Box::new(value), std::any::type_name::<T>()));
        self
    }

    /// Whether the pending default differs from `value`, and the message that
    /// restores it.
    fn reset<T: Any + Clone + PartialEq>(
        &mut self,
        name: &str,
        value: &T,
        on: &Rc<dyn Fn(T) -> M>,
    ) -> Option<(bool, M)> {
        let default = take_default_from::<T, M>(&mut self.default, self.ui, name)?;
        Some((default != *value, on(default.clone())))
    }

    /// A collapsible group of rows. The header sends the toggled state, which
    /// [`PropertyGridState::apply`] stores.
    pub fn group(&mut self, name: &str, content: impl FnOnce(&mut PropertyGrid<'_, '_, M>)) {
        self.finish();
        let mut path = self.path.clone();
        path.push(name.to_owned());
        let joined = path.join("/");
        let filter = self.filter();
        let shown_by_group = self.shown_by_group || (!filter.is_empty() && contains(name, &filter));
        let collapsed = filter.is_empty() && self.state.collapsed.contains(&joined);
        let depth = self.path.len();
        let (state, send, window) = (self.state, self.send.clone(), self.window);
        let first = window.map(|window| window.counted.get());
        // What was passed over before the group is a spacer inside it, if the
        // group has anything in view.
        let carried = std::mem::take(&mut self.skipped);
        let (mut rows, mut skipped, mut shown) = (0, 0, false);
        let style = Style {
            width: Length::Percent(1.0),
            gap: ROW_GAP,
            ..Style::default()
        };
        self.ui.column_at(format!("group:{name}"), style, |ui| {
            let mut grid = PropertyGrid {
                ui,
                state,
                send,
                path,
                shown_by_group,
                default: None,
                window,
                skipped: carried,
                shown: false,
                rows: 0,
            };
            if grid.take_slot() {
                let header = Style {
                    width: Length::Percent(1.0),
                    height: Length::Px(ROW_HEIGHT),
                    padding: 8.0,
                    radius: 6.0,
                    font_weight: Some(700),
                    background: Some(grid.ui.context.theme().track),
                    ..Style::default()
                };
                let arrow = if collapsed { "\u{25b8}" } else { "\u{25be}" };
                let indent = "    ".repeat(depth);
                let toggle = (grid.send)(PropertyMsg::Group {
                    path: joined.clone(),
                    collapsed: !state.collapsed.contains(&joined),
                });
                grid.ui.at("$header").button(
                    Button::new(format!("{indent}{arrow}  {name}"))
                        .style(header)
                        .on_click(toggle),
                );
                let key = if collapsed {
                    "glassmere-property-expand"
                } else {
                    "glassmere-property-collapse"
                };
                let tooltip = grid.ui.context.tr(key, &[("name", name.into())]);
                grid.ui.tooltip(tooltip);
            }
            if !collapsed {
                content(&mut grid);
            }
            grid.finish();
            (rows, skipped, shown) = (grid.rows, grid.skipped, grid.shown);
        });
        if !filter.is_empty() && rows == 0 {
            // A filter hides groups that have nothing to show, and their
            // header is not counted.
            self.ui.take_declared();
            if let (Some(window), Some(first)) = (window, first) {
                window.counted.set(first);
            }
            self.skipped = carried;
        } else {
            // A scrolled grid declares no group that is out of view.
            if !shown {
                self.ui.take_declared();
            }
            self.skipped = skipped;
        }
        self.rows += rows;
    }

    /// One row: the name, the editor and, with a reset, a button that sends
    /// its message while the property differs from the default.
    fn row(
        &mut self,
        name: &str,
        reset: Option<(bool, M)>,
        editor: impl FnOnce(&mut Ui<'_, M>, &str),
    ) {
        let filter = self.filter();
        if !(filter.is_empty() || self.shown_by_group || contains(name, &filter)) {
            return;
        }
        self.rows += 1;
        if !self.take_slot() {
            return;
        }
        let share = self.state.name_share;
        let shown = format!("{}{name}", "    ".repeat(self.path.len()));
        let style = Style {
            width: Length::Percent(1.0),
            height: Length::Px(ROW_HEIGHT),
            gap: COLUMN_GAP,
            align_items: Align::Center,
            ..Style::default()
        };
        self.ui.row_at(format!("row:{name}"), style, |ui| {
            ui.label_at(
                "$name",
                shown,
                Style::default().width(Length::Percent(share)),
            );
            ui.container_at(
                "$editor",
                Style {
                    grow: 1.0,
                    shrink: 1.0,
                    ..Style::default()
                },
                |ui| editor(ui, name),
            );
            if let Some((differs, message)) = reset {
                let compact = Style {
                    padding: 8.0,
                    ..Style::button()
                };
                ui.at("$reset").button(
                    Button::new("\u{21ba}")
                        .enabled(differs)
                        .style(compact)
                        .on_click(message),
                );
                ui.tooltip(
                    ui.context
                        .tr("glassmere-property-reset", &[("name", name.into())]),
                );
            }
        });
    }

    /// A number row: edits send `on` of the number.
    pub fn number_with(
        &mut self,
        name: &str,
        value: f64,
        options: NumberOptions,
        on: impl Fn(f64) -> M + 'static,
    ) {
        let options = options.style(Style::default().width(Length::Percent(1.0)));
        let on: Rc<dyn Fn(f64) -> M> = Rc::new(on);
        let reset = self.reset(name, &value, &on);
        self.row(name, reset, |ui, label| {
            ui.at("$value")
                .number_input(label, value, options, move |number| on(number));
        });
    }

    /// A number row whose edits send `PropertyMsg::Number` with its path.
    pub fn number(&mut self, name: &str, value: f64, options: NumberOptions) {
        let (path, send) = (self.path_of(name), self.send.clone());
        self.number_with(name, value, options, move |value| {
            send(PropertyMsg::Number {
                path: path.clone(),
                value,
            })
        });
    }

    /// A checkbox row: a click sends `on` of the opposite of `value`.
    pub fn bool_with(&mut self, name: &str, value: bool, on: impl Fn(bool) -> M + 'static) {
        let on: Rc<dyn Fn(bool) -> M> = Rc::new(on);
        let reset = self.reset(name, &value, &on);
        self.row(name, reset, |ui, label| {
            // The name is already in the first column; the box alone shows.
            let checkbox = ui.at("$value");
            checkbox.checkbox_shown(label, "", value);
            checkbox.set_handler(Handler::Message(on(!value)));
        });
    }

    /// A checkbox row whose clicks send `PropertyMsg::Bool` with its path.
    pub fn bool(&mut self, name: &str, value: bool) {
        let (path, send) = (self.path_of(name), self.send.clone());
        self.bool_with(name, value, move |value| {
            send(PropertyMsg::Bool {
                path: path.clone(),
                value,
            })
        });
    }

    /// A text row: edits send `on` of the text.
    pub fn text_with(&mut self, name: &str, value: &str, on: impl Fn(String) -> M + 'static) {
        let on: Rc<dyn Fn(String) -> M> = Rc::new(on);
        let reset = self.reset(name, &value.to_owned(), &on);
        self.row(name, reset, |ui, label| {
            ui.at("$value")
                .text_input(TextInput::new(label).style(fill()), value, move |text| {
                    on(text)
                });
        });
    }

    /// A text row whose edits send `PropertyMsg::Text` with its path.
    pub fn text(&mut self, name: &str, value: &str) {
        let (path, send) = (self.path_of(name), self.send.clone());
        self.text_with(name, value, move |value| {
            send(PropertyMsg::Text {
                path: path.clone(),
                value,
            })
        });
    }

    /// A drop-down row over `choices`: a choice sends `on` of its index.
    pub fn choice_with(
        &mut self,
        name: &str,
        value: usize,
        choices: &[&str],
        on: impl Fn(usize) -> M + 'static,
    ) {
        let on: Rc<dyn Fn(usize) -> M> = Rc::new(on);
        let reset = self.reset(name, &value, &on);
        self.row(name, reset, |ui, label| {
            ui.at("$value").combo_box(
                ComboBox::new(label).style(fill()),
                value,
                choices,
                move |index| on(index),
            );
        });
    }

    /// A drop-down row whose choices send `PropertyMsg::Choice` with its path.
    pub fn choice(&mut self, name: &str, value: usize, choices: &[&str]) {
        let (path, send) = (self.path_of(name), self.send.clone());
        self.choice_with(name, value, choices, move |value| {
            send(PropertyMsg::Choice {
                path: path.clone(),
                value,
            })
        });
    }

    /// A swatch and a `#rrggbb` or `#rrggbbaa` field. A whole colour sends
    /// `on` at once. Text that is not a colour sends nothing, and the field
    /// keeps what was typed until the colour changes.
    pub fn color_with(&mut self, name: &str, value: Color, on: impl Fn(Color) -> M + 'static) {
        let on: Rc<dyn Fn(Color) -> M> = Rc::new(on);
        let reset = self.reset(name, &value, &on);
        self.row(name, reset, |ui, label| {
            let row = Style {
                width: Length::Percent(1.0),
                gap: COLUMN_GAP,
                align_items: Align::Center,
                ..Style::default()
            };
            ui.row_at("$value", row, |ui| {
                ui.container_at(
                    "$swatch",
                    Style {
                        width: Length::Px(28.0),
                        height: Length::Px(28.0),
                        radius: 6.0,
                        background: Some(value),
                        ..Style::default()
                    },
                    |_| {},
                );
                let mut shown = hex(value);
                let field = Style {
                    grow: 1.0,
                    shrink: 1.0,
                    width: Length::Auto,
                    ..Ui::<()>::field_style()
                };
                let hex_field = ui.at("$hex");
                let key = hex_field.next_key();
                // What was typed stays while the field has focus, whether or
                // not it is a colour yet; leaving the field drops it.
                let id = hex_field
                    .context
                    .child(hex_field.parent, &key, Kind::TextInput);
                let mut drafting = false;
                if let Some(state) = id.and_then(|id| hex_field.context.field_state(id))
                    && state.draft
                    && state.focused
                {
                    shown = state.text;
                    drafting = true;
                }
                hex_field.text_field(
                    key,
                    label.to_owned(),
                    &shown,
                    "#rrggbb",
                    field,
                    FieldExtra {
                        draft: drafting,
                        ..FieldExtra::default()
                    },
                );
                hex_field.set_handler(Handler::Field {
                    change: Some(Box::new(move |update| match update {
                        ControlUpdate::Text(text) => parse_hex(&text).map(|color| on(color)),
                        _ => None,
                    })),
                    submit: None,
                });
            });
        });
    }

    /// A colour row whose edits send `PropertyMsg::Color` with its path.
    pub fn color(&mut self, name: &str, value: Color) {
        let (path, send) = (self.path_of(name), self.send.clone());
        self.color_with(name, value, move |value| {
            send(PropertyMsg::Color {
                path: path.clone(),
                value,
            })
        });
    }

    /// Any editor, declared with the application's own messages. Give its
    /// controls `name` as their accessible label.
    pub fn custom(&mut self, name: &str, editor: impl FnOnce(&mut Ui<'_, M>, &str)) {
        self.finish();
        self.row(name, None, editor);
    }

    /// A value that is shown, not edited.
    pub fn readonly(&mut self, name: &str, text: impl Into<String>) {
        let text = text.into();
        self.finish();
        self.row(name, None, |ui, _| {
            let muted = ui.context.theme().text_muted;
            ui.label_at(
                "$value",
                text,
                Style {
                    foreground: Some(muted),
                    ..Style::default()
                },
            );
        });
    }
}

impl<'a, M: 'static> Ui<'a, M> {
    /// A two-column editor for named values: a filter box, a header whose
    /// divider sizes the name column, collapsible groups and one row per
    /// property, declared through the builder. The grid shows `state` and
    /// sends a [`PropertyMsg`] through `on` for each change: a filter, a
    /// collapsed group, or a value by its path. The application applies the
    /// first two with [`PropertyGridState::apply`] and stores the values.
    ///
    /// The name column's divider sends [`PropertyMsg::NameShare`], which
    /// [`PropertyGridState::apply`] stores.
    ///
    /// With a [`PropertyGridOptions::height`] the grid scrolls and declares
    /// only the rows in view and a few beyond, like a virtual list; the
    /// content still runs for every property, so it should only name them.
    /// Rows and group headers are all one height for that.
    #[track_caller]
    pub fn property_grid(
        &mut self,
        state: &PropertyGridState,
        options: PropertyGridOptions,
        on: impl Fn(PropertyMsg) -> M + 'static,
        content: impl FnOnce(&mut PropertyGrid<'_, '_, M>),
    ) {
        let send: Rc<dyn Fn(PropertyMsg) -> M> = Rc::new(on);
        let mut style = options.style.clone();
        if style.width == Length::Auto {
            style.width = Length::Percent(1.0);
        }
        style.gap = 4.0;
        let content_width = Length::Percent(if options.height.is_some() {
            BESIDE_SCROLLBAR
        } else {
            1.0
        });
        self.column(style, |ui| {
            if options.filter {
                let send = send.clone();
                let placeholder = ui.context.tr("glassmere-filter-placeholder", &[]);
                let label = ui.context.tr(
                    "glassmere-grid-filter",
                    &[("name", options.label.as_str().into())],
                );
                ui.at("$filter").text_input(
                    TextInput::new(label)
                        .placeholder(placeholder)
                        .style(fill().width(content_width)),
                    &state.filter,
                    move |text| send(PropertyMsg::Filter(text)),
                );
            }
            let header = |key: &'static str| {
                move |ui: &mut Ui<'_, M>| {
                    let text = ui.context.tr(key, &[]);
                    ui.label_at(
                        "$title",
                        text,
                        Style {
                            font_size: Some(12.0),
                            foreground: Some(ui.context.theme().text_muted),
                            ..Style::default()
                        },
                    );
                }
            };
            let share = send.clone();
            ui.at("$columns").split(
                state.name_share,
                Axis::Horizontal,
                SplitOptions {
                    min_first: 60.0,
                    min_second: 100.0,
                    divider: COLUMN_GAP,
                    style: Style::default()
                        .width(content_width)
                        .height(Length::Px(22.0)),
                },
                move |share_now| share(PropertyMsg::NameShare(share_now)),
                header("glassmere-property-column"),
                header("glassmere-value-column"),
            );
            // A grid that scrolls declares the rows in view and a few beyond,
            // like a virtual list, where the last declaration left the scroll.
            let window = options.height.map(|height| {
                let offset = ui
                    .context
                    .child(ui.parent, &WidgetKey::named("$scroll"), Kind::Scroll)
                    .and_then(|scroll| ui.context.node(scroll))
                    .and_then(|node| node.scroll)
                    .map_or(0.0, |scroll| scroll.offset);
                let list = VirtualListOptions {
                    height,
                    row_height: ROW_HEIGHT + ROW_GAP,
                    ..VirtualListOptions::default()
                };
                Window {
                    range: realized_rows(usize::MAX, offset, &list),
                    counted: Cell::new(0),
                    offset,
                    height,
                }
            });
            let rows = |ui: &mut Ui<'_, M>| {
                let mut grid = PropertyGrid {
                    ui,
                    state,
                    send,
                    path: Vec::new(),
                    shown_by_group: false,
                    default: None,
                    window: window.as_ref(),
                    skipped: 0,
                    shown: false,
                    rows: 0,
                };
                content(&mut grid);
                grid.finish();
                grid.flush("$after");
                if window.as_ref().is_some_and(Window::past_end) {
                    grid.ui.request_redraw();
                }
            };
            let body = Style {
                width: Length::Percent(1.0),
                gap: ROW_GAP,
                ..Style::default()
            };
            match options.height {
                Some(height) => ui.scroll_at(
                    "$scroll",
                    Style::default()
                        .width(Length::Percent(1.0))
                        .height(Length::Px(height)),
                    |ui| ui.column_at("$rows", body.width(content_width), rows),
                ),
                None => ui.column_at("$rows", body, rows),
            }
        });
    }
}
