use super::controls::FieldExtra;
use super::*;
use crate::{AutocompleteResponse, Length, MenuItem, NumberOptions, NumericValue};

impl Ui<'_> {
    /// A text input that offers `suggestions` in a list below it while the
    /// user types. The app decides what matches: pass the entries for the
    /// current `value` each frame (see `suggest` for the simple case), from
    /// memory, an index or the latest answer of a remote search. Free text
    /// stays allowed. Up and Down move through the list, Enter or a click
    /// takes a suggestion, Escape closes the list, and Enter without a
    /// highlighted suggestion submits what was typed.
    pub fn autocomplete<S: AsRef<str>>(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        value: &mut String,
        suggestions: &[S],
    ) -> AutocompleteResponse {
        let key = key.into();
        let id = self
            .context
            .child(self.parent, &WidgetKey::Named(key.clone()), Kind::TextInput);
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

    /// A field for one number with stepper buttons. Typing is free text: Enter,
    /// leaving the field or stepping commits it, which parses (a decimal comma
    /// is accepted), clamps to the range, snaps to the step and reformats;
    /// text that is not a number reverts. Up and Down step once, PageUp and
    /// PageDown ten times, and the wheel steps while the field has both the
    /// keyboard and the pointer.
    pub fn number_input(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        value: &mut f64,
        options: NumberOptions,
    ) -> Response {
        let label = label.into();
        let old = *value;
        *value = options.constrain(*value);
        let mut row = options.style.clone();
        if row.width == Length::Auto {
            row.width = Length::Px(160.0);
        }
        row.gap = 4.0;
        let mut result = Response::default();
        self.group(key.into(), Kind::Row, row, |ui| {
            let child = |ui: &Ui<'_>, key: &str, kind| {
                ui.context
                    .child(ui.parent, &WidgetKey::Named(key.into()), kind)
            };
            let field = child(ui, "$field", Kind::TextInput);
            let clicked = |ui: &Ui<'_>, key: &str| {
                child(ui, key, Kind::Button).is_some_and(|id| ui.clicks.contains(&id))
            };
            let mut steps = match ui.take_update("$field", Kind::TextInput) {
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
            result = ui.text_field(
                "$field".into(),
                label,
                &mut text,
                "",
                style,
                FieldExtra {
                    number: Some(number),
                    draft: drafting,
                    ..FieldExtra::default()
                },
            );
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
