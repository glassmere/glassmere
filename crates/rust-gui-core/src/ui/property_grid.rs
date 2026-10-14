use super::*;
use crate::{
    Align, Axis, Length, NumberOptions, PropertyGridOptions, PropertyGridResponse,
    PropertyGridState, SplitOptions,
};
use rust_gui_scene::Color;
use std::any::Any;

/// Gap between the name and the value, and the width of the header's divider,
/// so the columns line up with the header.
const COLUMN_GAP: f32 = 6.0;

/// A value that knows its property grid rows. Usually derived with
/// `#[derive(Properties)]` from the `rust-gui` crate; written by hand it is a
/// function that calls the builder.
pub trait Properties {
    fn properties(&mut self, grid: &mut PropertyGrid<'_, '_>);
}

/// Declares the rows of a property grid; see `Ui::property_grid`.
pub struct PropertyGrid<'a, 'u> {
    ui: &'a mut Ui<'u>,
    state: &'a mut PropertyGridState,
    changed: &'a mut Vec<String>,
    /// Names of the enclosing groups.
    path: Vec<String>,
    /// An enclosing group matched the filter, so all its rows show.
    shown_by_group: bool,
    default: Option<(Box<dyn Any>, &'static str)>,
    rows: usize,
}

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
        ..Ui::field_style()
    }
}

impl Ui<'_> {
    /// A two-column editor for named values: a filter box, a header whose
    /// divider sizes the name column, collapsible groups and one row per
    /// property, declared through the builder. Values are bound directly:
    ///
    /// ```ignore
    /// let response = ui.property_grid(&mut state, options, |grid| {
    ///     grid.group("Layout", |grid| {
    ///         grid.default(120.0).number("Width", &mut width, NumberOptions::default());
    ///         grid.bool("Visible", &mut visible);
    ///     });
    /// });
    /// ```
    #[track_caller]
    pub fn property_grid(
        &mut self,
        state: &mut PropertyGridState,
        options: PropertyGridOptions,
        content: impl FnOnce(&mut PropertyGrid<'_, '_>),
    ) -> PropertyGridResponse {
        let mut changed = Vec::new();
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
                ui.text_input_at(
                    "$filter",
                    ui.context.tr(
                        "rust-gui-grid-filter",
                        &[("name", options.label.as_str().into())],
                    ),
                    &mut state.filter,
                    &ui.context.tr("rust-gui-filter-placeholder", &[]),
                    fill().width(content_width),
                );
            }
            let header = |key: &'static str| {
                move |ui: &mut Ui<'_>| {
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
            ui.at("$columns").split(
                &mut state.name_share,
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
            let rows = |ui: &mut Ui<'_>| {
                let mut grid = PropertyGrid {
                    ui,
                    state,
                    changed: &mut changed,
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
        PropertyGridResponse { changed }
    }
}

impl PropertyGrid<'_, '_> {
    fn filter(&self) -> String {
        self.state.filter.trim().to_lowercase()
    }

    fn finish(&mut self) {
        if self.default.take().is_some() {
            self.ui
                .misuse("PropertyGrid::default was not followed by a row".to_owned());
        }
    }

    /// Gives the next row a reset button that restores `value`; it is enabled
    /// while the property differs. The type must be the row's value type.
    pub fn default<T: Any>(&mut self, value: T) -> &mut Self {
        self.default = Some((Box::new(value), std::any::type_name::<T>()));
        self
    }

    fn take_default<T: Any>(&mut self, name: &str) -> Option<T> {
        let (value, given) = self.default.take()?;
        match value.downcast::<T>() {
            Ok(value) => Some(*value),
            Err(_) => {
                self.ui.misuse(format!(
                    "property `{name}` holds a {} but its default is a {given}",
                    std::any::type_name::<T>()
                ));
                None
            }
        }
    }

    /// A collapsible group of rows; groups nest. While a filter is typed,
    /// collapsed groups open, groups without a matching row hide, and a group
    /// whose own name matches shows all its rows.
    pub fn group(&mut self, name: &str, content: impl FnOnce(&mut PropertyGrid<'_, '_>)) {
        self.finish();
        let mut path = self.path.clone();
        path.push(name.to_owned());
        let joined = path.join("/");
        let filter = self.filter();
        let shown_by_group = self.shown_by_group || (!filter.is_empty() && contains(name, &filter));
        let collapsed = filter.is_empty() && self.state.collapsed.contains(&joined);
        let depth = self.path.len();
        let (state, changed) = (&mut *self.state, &mut *self.changed);
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
            let toggled = ui
                .button_at("$header", format!("{indent}{arrow}  {name}"), true, header)
                .clicked();
            ui.tooltip(if collapsed {
                ui.context
                    .tr("rust-gui-property-expand", &[("name", name.into())])
            } else {
                ui.context
                    .tr("rust-gui-property-collapse", &[("name", name.into())])
            });
            if toggled && !state.collapsed.remove(&joined) {
                state.collapsed.insert(joined.clone());
            }
            if collapsed {
                return;
            }
            let mut grid = PropertyGrid {
                ui,
                state,
                changed,
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

    /// One row: the name, the editor and, with a default, a reset button.
    /// Returns whether the editor reported a change and whether reset was clicked.
    fn row(
        &mut self,
        name: &str,
        differs: Option<bool>,
        editor: impl FnOnce(&mut Ui<'_>, &str) -> bool,
    ) -> (bool, bool) {
        let filter = self.filter();
        if !(filter.is_empty() || self.shown_by_group || contains(name, &filter)) {
            return (false, false);
        }
        self.rows += 1;
        let share = self.state.name_share;
        // Nested groups indent their names; the value column stays aligned.
        let shown = format!("{}{name}", "    ".repeat(self.path.len()));
        let (mut changed, mut reset) = (false, false);
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
                |ui| changed = editor(ui, name),
            );
            if let Some(differs) = differs {
                let compact = Style {
                    padding: 8.0,
                    ..Style::button()
                };
                reset = ui
                    .button_at("$reset", "\u{21ba}", differs, compact)
                    .clicked();
                ui.tooltip(
                    ui.context
                        .tr("rust-gui-property-reset", &[("name", name.into())]),
                );
            }
        });
        if changed || reset {
            let mut path = self.path.clone();
            path.push(name.to_owned());
            self.changed.push(path.join("/"));
        }
        (changed, reset)
    }

    /// A row for any value that can be compared and restored.
    fn typed<T: Any + Clone + PartialEq>(
        &mut self,
        name: &str,
        value: &mut T,
        editor: impl FnOnce(&mut Ui<'_>, &str, &mut T) -> bool,
    ) -> bool {
        let default = self.take_default::<T>(name);
        let differs = default.as_ref().map(|default| default != value);
        let (changed, reset) = self.row(name, differs, |ui, label| editor(ui, label, value));
        if let (true, Some(default)) = (reset, default) {
            *value = default;
        }
        changed || reset
    }

    pub fn number(&mut self, name: &str, value: &mut f64, options: NumberOptions) -> bool {
        let options = options.style(Style::default().width(Length::Percent(1.0)));
        self.typed(name, value, |ui, label, value| {
            ui.at("$value").number_input(label, value, options).changed
        })
    }

    pub fn bool(&mut self, name: &str, value: &mut bool) -> bool {
        self.typed(name, value, |ui, label, value| {
            // The name is already in the first column; the box alone shows.
            ui.at("$value").checkbox_shown(label, "", value).changed
        })
    }

    pub fn text(&mut self, name: &str, value: &mut String) -> bool {
        self.typed(name, value, |ui, label, value| {
            ui.text_input_at("$value", label, value, "", fill()).changed
        })
    }

    /// One of `choices`, as an index.
    pub fn choice(&mut self, name: &str, value: &mut usize, choices: &[&str]) -> bool {
        self.typed(name, value, |ui, label, value| {
            ui.combo_box_at("$value", label, value, choices, fill())
                .changed
        })
    }

    /// A swatch and a `#rrggbb` or `#rrggbbaa` field. The colour follows the
    /// text whenever it is a whole colour; anything else is a draft that is
    /// dropped when the field is left.
    pub fn color(&mut self, name: &str, value: &mut Color) -> bool {
        let mut path = self.path.clone();
        path.push(name.to_owned());
        let path = path.join("/");
        let mut drafts = std::mem::take(&mut self.state.drafts);
        let changed = self.typed(name, value, |ui, label, value| {
            let mut changed = false;
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
                        background: Some(*value),
                        ..Style::default()
                    },
                    |_| {},
                );
                let mut text = drafts.get(&path).cloned().unwrap_or_else(|| hex(*value));
                let field = Style {
                    grow: 1.0,
                    shrink: 1.0,
                    width: Length::Auto,
                    ..Ui::field_style()
                };
                let response = ui.text_input_at("$hex", label, &mut text, "#rrggbb", field);
                if response.changed {
                    if let Some(color) = parse_hex(&text) {
                        changed = color != *value;
                        *value = color;
                    }
                    drafts.insert(path.clone(), text);
                } else if !response.focused {
                    drafts.remove(&path);
                }
            });
            changed
        });
        self.state.drafts = drafts;
        changed
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
            false
        });
    }

    /// Any editor. Give its controls `name` as their accessible label and
    /// return whether the value changed.
    pub fn custom(&mut self, name: &str, editor: impl FnOnce(&mut Ui<'_>, &str) -> bool) -> bool {
        self.finish();
        self.row(name, None, editor).0
    }
}
