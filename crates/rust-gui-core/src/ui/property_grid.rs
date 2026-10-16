use super::controls::FieldExtra;
use super::*;
use crate::app::Handler;
use crate::controls::ControlUpdate;
use crate::{
    Align, Axis, Button, ComboBox, Length, NumberOptions, PropertyGridOptions, PropertyGridState,
    PropertyMsg, SplitOptions, TextInput,
};
use rust_gui_scene::Color;
use std::any::Any;
use std::rc::Rc;

/// Gap between the name and the value, and the width of the header's divider,
/// so the columns line up with the header.
const COLUMN_GAP: f32 = 6.0;

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
// ponytail: a percentage because styles have no one-sided padding; an inset
// when they do.
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
        let (state, send) = (self.state, self.send.clone());
        let mut rows = 0;
        let style = Style {
            width: Length::Percent(1.0),
            gap: 4.0,
            ..Style::default()
        };
        self.ui.column_at(format!("group:{name}"), style, |ui| {
            let header = Style {
                width: Length::Percent(1.0),
                padding: 8.0,
                radius: 6.0,
                font_weight: Some(700),
                background: Some(ui.context.theme().track),
                ..Style::default()
            };
            let arrow = if collapsed { "\u{25b8}" } else { "\u{25be}" };
            let indent = "    ".repeat(depth);
            let toggle = send(PropertyMsg::Group {
                path: joined.clone(),
                collapsed: !state.collapsed.contains(&joined),
            });
            ui.at("$header").button(
                Button::new(format!("{indent}{arrow}  {name}"))
                    .style(header)
                    .on_click(toggle),
            );
            ui.tooltip(if collapsed {
                ui.context
                    .tr("rust-gui-property-expand", &[("name", name.into())])
            } else {
                ui.context
                    .tr("rust-gui-property-collapse", &[("name", name.into())])
            });
            if collapsed {
                return;
            }
            let mut grid = PropertyGrid {
                ui,
                state,
                send,
                path,
                shown_by_group,
                default: None,
                rows: 0,
            };
            content(&mut grid);
            grid.finish();
            rows = grid.rows;
        });
        // A filter hides groups that have nothing to show.
        if !filter.is_empty() && rows == 0 {
            self.ui.take_declared();
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
        let share = self.state.name_share;
        let shown = format!("{}{name}", "    ".repeat(self.path.len()));
        let style = Style {
            width: Length::Percent(1.0),
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
                        .tr("rust-gui-property-reset", &[("name", name.into())]),
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
                let placeholder = ui.context.tr("rust-gui-filter-placeholder", &[]);
                let label = ui.context.tr(
                    "rust-gui-grid-filter",
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
                move |share_now| share(PropertyMsg::NameShare(share_now)),
                Axis::Horizontal,
                SplitOptions {
                    min_first: 60.0,
                    min_second: 100.0,
                    divider: COLUMN_GAP,
                    style: Style::default()
                        .width(content_width)
                        .height(Length::Px(22.0)),
                },
                header("rust-gui-property-column"),
                header("rust-gui-value-column"),
            );
            let rows = |ui: &mut Ui<'_, M>| {
                let mut grid = PropertyGrid {
                    ui,
                    state,
                    send,
                    path: Vec::new(),
                    shown_by_group: false,
                    default: None,
                    rows: 0,
                };
                content(&mut grid);
                grid.finish();
            };
            let body = Style {
                width: Length::Percent(1.0),
                gap: 4.0,
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
