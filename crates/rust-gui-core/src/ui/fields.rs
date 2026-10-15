use super::controls::FieldExtra;
use super::*;
use crate::app::Handler;
use crate::editor::single_line;
use crate::{AutocompleteResponse, Length, MenuItem, NumberOptions, NumericValue};

impl<'a, M: 'static> Ui<'a, M> {
    /// A text input that offers `suggestions` in a list below it while the
    /// user types. The app decides what matches: pass the entries for the
    /// current `value` each frame (see `suggest` for the simple case), from
    /// memory, an index or the latest answer of a remote search. Free text
    /// stays allowed. Up and Down move through the list, Enter or a click
    /// takes a suggestion, Escape closes the list, and Enter without a
    /// highlighted suggestion submits what was typed.
    #[track_caller]
    pub fn autocomplete<S: AsRef<str>>(
        &mut self,
        label: impl Into<String>,
        value: &mut String,
        suggestions: &[S],
    ) -> AutocompleteResponse<'a> {
        let key = self.next_key();
        let id = self.context.child(self.parent, &key, Kind::TextInput);
        let mut picked = None;
        let mut changed = false;
        // The field already holds the chosen text; the list may have moved on.
        if let Some(ControlUpdate::Choice(index)) = id.and_then(|id| self.updates.get(&id).cloned())
            && let Some(text) = id
                .and_then(|id| self.context.field_state(id))
                .map(|state| state.text)
        {
            self.updates.remove(&id.unwrap());
            if self.enabled {
                changed = *value != text;
                *value = text;
                picked = Some(index);
            }
        }
        let entries: Vec<MenuItem> = suggestions
            .iter()
            .map(|label| MenuItem::new(label.as_ref()))
            .collect();
        let focused = id.is_some_and(|id| self.context.focused() == Some(id));
        let children = if focused && !entries.is_empty() {
            self.popup_children(
                &key,
                Kind::TextInput,
                &entries,
                usize::MAX,
                false,
                Length::Percent(1.0),
            )
        } else {
            Vec::new()
        };
        let mut response = self.text_field(
            key,
            label.into(),
            value,
            "",
            Self::field_style(),
            FieldExtra {
                number: None,
                suggestions: Some(entries),
                children,
                draft: false,
            },
        );
        response.changed |= changed;
        AutocompleteResponse { response, picked }
    }

    /// `autocomplete` showing `value`: an edit sends `on_change(text)`, and a
    /// pick from the list sends `on_change` of the suggestion it took.
    #[track_caller]
    pub fn autocomplete_msg<S: AsRef<str>>(
        &mut self,
        label: impl Into<String>,
        value: &str,
        suggestions: &[S],
        on_change: impl Fn(String) -> M + 'static,
    ) -> Response<'a> {
        let key = self.next_key();
        let labels: Vec<String> = suggestions
            .iter()
            .map(|label| label.as_ref().to_owned())
            .collect();
        let entries: Vec<MenuItem> = labels
            .iter()
            .map(|label| MenuItem::new(label.as_str()))
            .collect();
        let id = self.context.child(self.parent, &key, Kind::TextInput);
        let focused = id.is_some_and(|id| self.context.focused() == Some(id));
        let children = if focused && !entries.is_empty() {
            self.popup_children(
                &key,
                Kind::TextInput,
                &entries,
                usize::MAX,
                false,
                Length::Percent(1.0),
            )
        } else {
            Vec::new()
        };
        let mut shown = value.to_owned();
        let response = self.text_field(
            key,
            label.into(),
            &mut shown,
            "",
            Self::field_style(),
            FieldExtra {
                number: None,
                suggestions: Some(entries),
                children,
                draft: false,
            },
        );
        // Picking a suggestion edits the field to it, and the kernel reports
        // that pick as the index, which names the text in `labels`.
        self.set_handler(Handler::Field {
            change: Some(Box::new(move |update| match update {
                ControlUpdate::Text(text) => Some(on_change(single_line(&text))),
                ControlUpdate::Choice(index) => {
                    labels.get(index).map(|label| on_change(label.clone()))
                }
                _ => None,
            })),
            submit: None,
        });
        response
    }

    /// A field for one number with stepper buttons. Typing is free text: Enter,
    /// leaving the field or stepping commits it, which parses (a decimal comma
    /// is accepted), clamps to the range, snaps to the step and reformats;
    /// text that is not a number reverts. Up and Down step once, PageUp and
    /// PageDown ten times, and the wheel steps while the field has both the
    /// keyboard and the pointer.
    #[track_caller]
    pub fn number_input(
        &mut self,
        label: impl Into<String>,
        value: &mut f64,
        options: NumberOptions,
    ) -> Response<'a> {
        let label = label.into();
        let old = *value;
        *value = options.constrain(*value);
        let mut row = options.style.clone();
        if row.width == Length::Auto {
            row.width = Length::Px(160.0);
        }
        row.gap = 4.0;
        let mut result = Response::default();
        let key = self.next_key();
        let context = self.context;
        self.group(key, Kind::Row, row, |ui| {
            let child = |ui: &Ui<'_, M>, key: &str, kind| {
                ui.context.child(ui.parent, &WidgetKey::named(key), kind)
            };
            let field = child(ui, "$field", Kind::TextInput);
            let clicked = |ui: &Ui<'_, M>, key: &str| {
                child(ui, key, Kind::Button).is_some_and(|id| ui.clicks.contains(&id))
            };
            let mut steps = match ui.take_update(&WidgetKey::named("$field"), Kind::TextInput) {
                Some(ControlUpdate::Number(steps)) => steps,
                _ => 0.0,
            };
            if ui.enabled {
                steps += f64::from(u8::from(clicked(ui, "$more")));
                steps -= f64::from(u8::from(clicked(ui, "$less")));
            }
            let submitted = field.is_some_and(|id| ui.submitted.contains(&id));
            let formatted = options.format_in(ui.context, *value);
            let mut text = formatted.clone();
            let mut drafting = false;
            if let Some(state) = field.and_then(|id| ui.context.field_state(id))
                // Only what the user typed is a draft. Text the field wrote
                // itself is never parsed back: after a change of locale it
                // would be read by the wrong rules.
                && state.draft
                // The app changing the number wins over a draft.
                && state.number.is_some_and(|last| last.value == *value)
            {
                if !state.focused || submitted || steps != 0.0 {
                    // Commit; text that is not a number reverts.
                    *value = options.parse_in(ui.context, &state.text).unwrap_or(*value);
                } else {
                    text = state.text;
                    drafting = true;
                }
            }
            if steps != 0.0 {
                *value = options.constrain(*value + steps * options.step);
            }
            if !drafting {
                text = options.format_in(ui.context, *value);
            }
            let number = NumericValue {
                value: *value,
                min: options.min,
                max: options.max,
                step: options.step,
            };
            let mut style = Self::field_style();
            style.width = Length::Auto;
            style.grow = 1.0;
            result = ui
                .text_field(
                    WidgetKey::named("$field"),
                    label,
                    &mut text,
                    "",
                    style,
                    FieldExtra {
                        number: Some(number),
                        draft: drafting,
                        ..FieldExtra::default()
                    },
                )
                .rebind(context);
            for (key, symbol, name, enabled) in [
                (
                    "$less",
                    "\u{2212}",
                    ui.context.tr("rust-gui-number-decrease", &[]),
                    *value > options.min,
                ),
                (
                    "$more",
                    "+",
                    ui.context.tr("rust-gui-number-increase", &[]),
                    *value < options.max,
                ),
            ] {
                ui.push(Element {
                    handler: None,
                    seq: 0,
                    key: WidgetKey::Named(key.into()),
                    kind: Kind::Button,
                    text: symbol.into(),
                    style: Style {
                        width: Length::Px(32.0),
                        height: Length::Px(44.0),
                        padding: 10.0,
                        ..Style::button()
                    },
                    enabled: ui.enabled && enabled,
                    children: Vec::new(),
                    custom: None,
                    tooltip: Some(name),
                    control: Control::Stepper,
                });
            }
        });
        result.changed = *value != old;
        result
    }
}

/// The entries of `candidates` that contain `typed`, ignoring case, at most
/// `limit` of them; nothing for empty text. The simple source for
/// `Ui::autocomplete`.
pub fn suggest<'a>(typed: &str, candidates: &[&'a str], limit: usize) -> Vec<&'a str> {
    let typed = typed.trim().to_lowercase();
    if typed.is_empty() {
        return Vec::new();
    }
    candidates
        .iter()
        .filter(|candidate| candidate.to_lowercase().contains(&typed))
        .take(limit)
        .copied()
        .collect()
}

impl<'a, M: 'static> Ui<'a, M> {
    // The framework's own composite widgets name the parts they are made of:
    // a number field's `$field` and its two steppers, a popup's `$popup`. That
    // structure is fixed and internal, so it does not go through `Ui::keyed`.
    // Each of these is the public method with a name given to it.

    /// Names the next element declared, and hands the `Ui` back to declare it.
    #[track_caller]
    pub(crate) fn at(&mut self, key: impl Into<String>) -> &mut Self {
        self.pending_key = Some(key.into());
        self
    }

    #[track_caller]
    pub(crate) fn label_keyed_at(&mut self, key: impl Into<String>, text: impl Into<String>) {
        self.at(key).label(text.into());
    }

    #[track_caller]
    pub(crate) fn label_at(
        &mut self,
        key: impl Into<String>,
        text: impl Into<String>,
        style: Style,
    ) {
        self.at(key).label(Label::new(text).style(style));
    }

    #[track_caller]
    pub(crate) fn button_at(
        &mut self,
        key: impl Into<String>,
        text: impl Into<String>,
        enabled: bool,
        style: Style,
    ) -> Response<'a> {
        self.at(key)
            .button(Button::new(text).enabled(enabled).style(style))
    }

    #[track_caller]
    pub(crate) fn text_input_at(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        value: &mut String,
        placeholder: &str,
        style: Style,
    ) -> Response<'a> {
        self.at(key).text_input(
            TextInput::new(label).placeholder(placeholder).style(style),
            value,
        )
    }

    #[track_caller]
    pub(crate) fn combo_box_at(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        selected: &mut usize,
        items: &[&str],
        style: Style,
    ) -> Response<'a> {
        self.at(key)
            .combo_box(ComboBox::new(label).style(style), selected, items)
    }

    #[track_caller]
    pub(crate) fn checkbox_at(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        checked: &mut bool,
    ) -> Response<'a> {
        self.at(key).checkbox(label, checked)
    }

    #[track_caller]
    pub(crate) fn column_at(
        &mut self,
        key: impl Into<String>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        self.at(key).column(style, content);
    }

    #[track_caller]
    pub(crate) fn row_at(
        &mut self,
        key: impl Into<String>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        self.at(key).row(style, content);
    }

    #[track_caller]
    pub(crate) fn container_at(
        &mut self,
        key: impl Into<String>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        self.at(key).container(style, content);
    }

    #[track_caller]
    pub(crate) fn scroll_at(
        &mut self,
        key: impl Into<String>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        self.at(key).scroll(style, content);
    }

    #[track_caller]
    pub(crate) fn focus_scope_at(
        &mut self,
        key: impl Into<String>,
        scope: crate::FocusScope,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        self.at(key).focus_scope(scope, style, content);
    }
}
