use super::*;
use crate::{Length, MenuItem, app::Handler, controls::slider_value, editor::single_line};
use std::ops::RangeInclusive;

/// What makes a text field a number or autocomplete field.
#[derive(Default)]
pub(super) struct FieldExtra {
    pub number: Option<crate::NumericValue>,
    pub suggestions: Option<Vec<MenuItem>>,
    /// The suggestion popup while it is open.
    pub children: Vec<Element>,
    /// See `Control::TextInput::draft`.
    pub draft: bool,
}

impl<'a, M: 'static> Ui<'a, M> {
    #[track_caller]
    pub(super) fn take_update(&mut self, key: &WidgetKey, kind: Kind) -> Option<ControlUpdate> {
        let id = self.context.child(self.parent, key, kind)?;
        let update = self.updates.remove(&id);
        self.enabled.then_some(update).flatten()
    }

    #[track_caller]
    fn control(
        &mut self,
        key: WidgetKey,
        kind: Kind,
        text: String,
        style: Style,
        control: Control,
    ) -> Response<'a> {
        self.push(Element {
            handler: None,
            seq: 0,
            key,
            kind,
            text,
            style,
            enabled: self.enabled,
            children: Vec::new(),
            custom: None,
            tooltip: None,
            control,
        })
    }

    /// A single-line text field. `ui.text_input("Name", &mut name)` is the
    /// whole of it, and [`TextInput::new`] carries a placeholder or a style.
    /// The label is what a screen reader announces, not what is drawn.
    #[track_caller]
    pub fn text_input(
        &mut self,
        input: impl Into<TextInput<M>>,
        value: &mut String,
    ) -> Response<'a> {
        let input = input.into();
        let key = self.next_key();
        let style = input.style.unwrap_or_else(Self::field_style);
        let response = self.text_field(
            key,
            input.label,
            value,
            &input.placeholder,
            style,
            FieldExtra::default(),
        );
        if let Some(submit) = input.on_submit {
            self.set_handler(Handler::Field {
                change: None,
                submit: Some(submit),
            });
        }
        response
    }

    /// A single-line text field showing `value`: an edit sends
    /// `on_change(text)`, and Enter what [`TextInput::on_submit`] names.
    #[track_caller]
    pub fn text_input_msg(
        &mut self,
        input: impl Into<TextInput<M>>,
        value: &str,
        on_change: impl Fn(String) -> M + 'static,
    ) -> Response<'a> {
        let input = input.into();
        let key = self.next_key();
        let style = input.style.unwrap_or_else(Self::field_style);
        let mut shown = value.to_owned();
        let response = self.text_field(
            key,
            input.label,
            &mut shown,
            &input.placeholder,
            style,
            FieldExtra::default(),
        );
        self.set_handler(Handler::Field {
            change: Some(Box::new(move |update| match update {
                ControlUpdate::Text(text) => Some(on_change(single_line(&text))),
                _ => None,
            })),
            submit: input.on_submit,
        });
        response
    }

    /// The look of a default text field.
    #[track_caller]
    pub(super) fn field_style() -> Style {
        Style {
            width: Length::Px(280.0),
            height: Length::Px(44.0),
            padding: 10.0,
            ..Style::button()
        }
    }

    /// Every single-line field: plain, number and autocomplete.
    #[track_caller]
    pub(super) fn text_field(
        &mut self,
        key: WidgetKey,
        label: String,
        value: &mut String,
        placeholder: &str,
        mut style: Style,
        extra: FieldExtra,
    ) -> Response<'a> {
        let mut changed = false;
        if let Some(ControlUpdate::Text(text)) = self.take_update(&key, Kind::TextInput) {
            changed = *value != text;
            *value = text;
        }
        let normalized = single_line(value);
        if normalized != *value {
            *value = normalized;
            changed = true;
        }
        style.wrap = rust_gui_scene::TextWrap::None;
        let mut response = self.push(Element {
            handler: None,
            seq: 0,
            key,
            kind: Kind::TextInput,
            text: String::new(),
            style,
            enabled: self.enabled,
            children: extra.children,
            custom: None,
            tooltip: None,
            control: Control::TextInput {
                label,
                value: value.clone(),
                placeholder: placeholder.into(),
                number: extra.number,
                suggestions: extra.suggestions,
                draft: extra.draft,
            },
        });
        response.changed = changed;
        response
    }

    /// A checkbox. The label is both what is drawn and what a screen reader
    /// announces.
    #[track_caller]
    pub fn checkbox(&mut self, label: impl Into<String>, checked: &mut bool) -> Response<'a> {
        let label = label.into();
        let shown = label.clone();
        self.checkbox_shown(label, &shown, checked)
    }

    /// A checkbox showing `checked`; a click sends `on_toggle(!checked)`.
    #[track_caller]
    pub fn checkbox_msg(
        &mut self,
        label: impl Into<String>,
        checked: bool,
        on_toggle: impl Fn(bool) -> M,
    ) -> Response<'a> {
        let label = label.into();
        let shown = label.clone();
        let mut shown_checked = checked;
        let response = self.checkbox_shown(label, &shown, &mut shown_checked);
        self.set_handler(Handler::Message(on_toggle(!checked)));
        response
    }

    /// A checkbox whose visible text differs from its accessible label, for
    /// layouts that show the name elsewhere.
    #[track_caller]
    pub(super) fn checkbox_shown(
        &mut self,
        label: impl Into<String>,
        shown: &str,
        checked: &mut bool,
    ) -> Response<'a> {
        let key = self.next_key();
        let label = label.into();
        let id = self.context.child(self.parent, &key, Kind::Checkbox);
        let changed = self.enabled && id.is_some_and(|id| self.clicks.remove(&id));
        if changed {
            *checked = !*checked;
        }
        let mut response = self.control(
            key,
            Kind::Checkbox,
            format!("{}  {shown}", if *checked { "☑" } else { "☐" })
                .trim_end()
                .to_owned(),
            Style::button(),
            Control::Checkbox {
                label,
                checked: *checked,
            },
        );
        response.changed = changed;
        response.clicked = changed;
        response
    }

    /// One of a set of choices, selected while `*selected == option`.
    #[track_caller]
    pub fn radio(
        &mut self,
        label: impl Into<String>,
        selected: &mut usize,
        option: usize,
    ) -> Response<'a> {
        let key = self.next_key();
        let label = label.into();
        let id = self.context.child(self.parent, &key, Kind::Radio);
        let clicked = self.enabled && id.is_some_and(|id| self.clicks.remove(&id));
        let changed = clicked && *selected != option;
        if clicked {
            *selected = option;
        }
        let mut response = self.control(
            key,
            Kind::Radio,
            format!("{}  {label}", if *selected == option { "●" } else { "○" }),
            Style::button(),
            Control::Radio {
                label,
                selected: *selected == option,
            },
        );
        response.changed = changed;
        response.clicked = clicked;
        response
    }

    /// A slider over `range`, snapping to `step`.
    #[track_caller]
    pub fn slider(
        &mut self,
        label: impl Into<String>,
        value: &mut f64,
        range: RangeInclusive<f64>,
        step: f64,
    ) -> Response<'a> {
        let (min, max) = (*range.start(), *range.end());
        if !(min.is_finite() && max.is_finite() && min < max && step.is_finite() && step > 0.0) {
            self.misuse("slider requires finite ordered bounds and a positive step".to_owned());
            return Response::default();
        }
        let key = self.next_key();
        let label = label.into();
        let old = *value;
        if let Some(ControlUpdate::Number(number)) = self.take_update(&key, Kind::Slider) {
            *value = number;
        }
        *value = slider_value(*value, min, max, step);
        let mut response = self.control(
            key,
            Kind::Slider,
            format!("{label}: {value:.2}"),
            Style {
                width: Length::Px(280.0),
                height: Length::Px(64.0),
                padding: 10.0,
                ..Style::button()
            },
            Control::Slider {
                label,
                value: *value,
                min,
                max,
                step,
            },
        );
        response.changed = old != *value;
        response
    }

    /// A slider showing `value` over `range`: a drag sends `on_change` of
    /// the value it reaches, snapped to `step`.
    #[track_caller]
    pub fn slider_msg(
        &mut self,
        label: impl Into<String>,
        value: f64,
        range: RangeInclusive<f64>,
        step: f64,
        on_change: impl Fn(f64) -> M + 'static,
    ) -> Response<'a> {
        let (min, max) = (*range.start(), *range.end());
        let mut shown = value;
        let response = self.slider(label, &mut shown, range, step);
        // A misused slider declared nothing to send from.
        if min.is_finite() && max.is_finite() && min < max && step.is_finite() && step > 0.0 {
            self.set_handler(Handler::Map(Box::new(move |update| match update {
                ControlUpdate::Number(number) => {
                    Some(on_change(slider_value(number, min, max, step)))
                }
                _ => None,
            })));
        }
        response
    }

    /// A drop-down over `items`, `*selected` being the one shown.
    /// `ui.combo_box("Theme", &mut choice, &items)` is the whole of it, and
    /// [`ComboBox::new`] carries a style.
    #[track_caller]
    pub fn combo_box(
        &mut self,
        combo: impl Into<ComboBox>,
        selected: &mut usize,
        items: &[&str],
    ) -> Response<'a> {
        let combo = combo.into();
        let style = combo.style.unwrap_or_else(Self::field_style);
        let key = self.next_key();
        let label = combo.label;
        let old = *selected;
        if let Some(ControlUpdate::Choice(index)) = self.take_update(&key, Kind::ComboBox) {
            *selected = index;
        }
        *selected = (*selected).min(items.len().saturating_sub(1));
        let entries = items
            .iter()
            .map(|label| MenuItem::new(*label))
            .collect::<Vec<_>>();
        let children = self.popup_children(
            &key,
            Kind::ComboBox,
            &entries,
            *selected,
            false,
            Length::Percent(1.0),
        );
        let mut response = self.push(Element {
            handler: None,
            seq: 0,
            key,
            kind: Kind::ComboBox,
            text: format!("{}  ▾", items.get(*selected).copied().unwrap_or("—")),
            style,
            enabled: self.enabled && !items.is_empty(),
            children,
            custom: None,
            tooltip: None,
            control: Control::Combo {
                label,
                selected: *selected,
                items: entries,
            },
        });
        response.changed = old != *selected;
        response
    }

    /// A popup menu. The returned item index is emitted once on activation.
    #[track_caller]
    pub fn menu(&mut self, label: impl Into<String>, items: &[MenuItem]) -> Option<usize> {
        let key = self.next_key();
        self.menu_styled(
            key,
            format!("{}  ▾", label.into()),
            items,
            Style {
                width: Length::Px(220.0),
                height: Length::Px(44.0),
                padding: 10.0,
                ..Style::button()
            },
            Length::Percent(1.0),
        )
    }

    /// `menu` with a caller-chosen button look and popup width.
    #[track_caller]
    pub(super) fn menu_styled(
        &mut self,
        key: WidgetKey,
        text: impl Into<String>,
        items: &[MenuItem],
        style: Style,
        popup_width: Length,
    ) -> Option<usize> {
        let choice = match self.take_update(&key, Kind::MenuButton) {
            Some(ControlUpdate::Choice(index))
                if items.get(index).is_some_and(|item| item.enabled) =>
            {
                Some(index)
            }
            _ => None,
        };
        let children =
            self.popup_children(&key, Kind::MenuButton, items, usize::MAX, true, popup_width);
        self.push(Element {
            handler: None,
            seq: 0,
            key,
            kind: Kind::MenuButton,
            text: text.into(),
            style,
            enabled: self.enabled && items.iter().any(|item| item.enabled),
            children,
            custom: None,
            tooltip: None,
            control: Control::Menu {
                items: items.to_vec(),
            },
        });
        choice
    }

    #[track_caller]
    pub(super) fn popup_children(
        &mut self,
        key: &WidgetKey,
        kind: Kind,
        items: &[MenuItem],
        selected: usize,
        menu: bool,
        width: Length,
    ) -> Vec<Element> {
        let owner = self.context.child(self.parent, key, kind);
        let Some(popup) = self
            .context
            .popup
            .filter(|popup| Some(popup.owner) == owner && self.enabled)
        else {
            return Vec::new();
        };
        let selection = self.context.theme().selection;
        let mut child = self.scope(owner);
        child.group(
            WidgetKey::named("$popup"),
            Kind::Popup,
            Style {
                // Percent widths match the owner; compact owners pass pixels.
                width,
                height: Length::Px((items.len() as f32 * 34.0 + 8.0).min(280.0)),
                gap: 2.0,
                padding: 4.0,
                ..Style::default()
            },
            |ui| {
                for (index, item) in items.iter().enumerate() {
                    ui.push(Element {
                        handler: None,
                        seq: 0,
                        key: WidgetKey::Named(index.to_string()),
                        kind: Kind::Choice,
                        text: item.label.clone(),
                        style: Style {
                            width: Length::Percent(1.0),
                            height: Length::Px(32.0),
                            padding: 5.0,
                            background: (index == popup.highlighted).then_some(selection),
                            ..Style::default()
                        },
                        enabled: item.enabled,
                        children: Vec::new(),
                        custom: None,
                        tooltip: None,
                        control: Control::Choice {
                            hint: item.hint.clone(),
                            index,
                            selected: selected == index,
                            menu,
                        },
                    });
                }
            },
        );
        child.elements[0].control = Control::Popup { menu };
        child.elements
    }
}

impl<'a, M: 'static> Ui<'a, M> {
    /// One command declaration per window. Its shortcut and button share the
    /// same enabled check and one-shot clicked response.
    #[track_caller]
    pub fn command_button(&mut self, command: &crate::Command) -> Response<'a> {
        let command = self.command(command);
        let response = self.button(
            Button::new(&command.label)
                .enabled(command.enabled)
                .style(Style::button()),
        );
        self.set_control(Control::Command {
            id: command.id,
            shortcut: command.shortcut,
            while_editing: command.while_editing,
            scoped: command.scoped,
        });
        response
    }

    /// The command as the user's bindings leave it. `command_button` applies
    /// this itself; menus and palettes go through it to show the shortcut the
    /// user will actually press.
    #[track_caller]
    pub fn command(&self, command: &crate::Command) -> crate::Command {
        match self.context.binding(&command.id) {
            Some(shortcut) => crate::Command {
                shortcut: shortcut.clone(),
                ..command.clone()
            },
            None => command.clone(),
        }
    }
}
