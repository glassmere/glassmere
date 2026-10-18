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
    pub password: bool,
}

impl<'a, M: 'static> Ui<'a, M> {
    /// What reached the widget the framework is about to declare under `key`
    /// since the last declaration, for its own composites, whose state is not
    /// the application's.
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
            text,
            style,
            enabled: self.enabled,
            control,
            ..Element::new(key, kind)
        })
    }

    /// A single-line text field showing `value`: an edit sends
    /// `on_change(text)`, and Enter what [`TextInput::on_submit`] names.
    #[track_caller]
    pub fn text_input(
        &mut self,
        input: impl Into<TextInput<M>>,
        value: &str,
        on_change: impl Fn(String) -> M + 'static,
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
            FieldExtra {
                password: input.password,
                ..FieldExtra::default()
            },
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
        value: &str,
        placeholder: &str,
        mut style: Style,
        extra: FieldExtra,
    ) -> Response<'a> {
        style.wrap = glassmere_scene::TextWrap::None;
        self.push(Element {
            style,
            enabled: self.enabled,
            children: extra.children,
            control: Control::TextInput {
                label,
                value: single_line(value),
                placeholder: placeholder.into(),
                number: extra.number,
                suggestions: extra.suggestions,
                draft: extra.draft,
                password: extra.password,
            },
            ..Element::new(key, Kind::TextInput)
        })
    }

    /// A checkbox showing `checked`; a click sends `on_toggle(!checked)`.
    #[track_caller]
    pub fn checkbox(
        &mut self,
        label: impl Into<String>,
        checked: bool,
        on_toggle: impl Fn(bool) -> M,
    ) -> Response<'a> {
        let label = label.into();
        let shown = label.clone();
        let response = self.checkbox_shown(label, &shown, checked);
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
        checked: bool,
    ) -> Response<'a> {
        let key = self.next_key();
        let label = label.into();
        self.control(
            key,
            Kind::Checkbox,
            format!("{}  {shown}", if checked { "☑" } else { "☐" })
                .trim_end()
                .to_owned(),
            Style::button(),
            Control::Checkbox { label, checked },
        )
    }

    /// One of a set of choices, selected while `selected == option`; a click
    /// sends `on_select(option)`.
    #[track_caller]
    pub fn radio(
        &mut self,
        label: impl Into<String>,
        selected: usize,
        option: usize,
        on_select: impl Fn(usize) -> M,
    ) -> Response<'a> {
        let key = self.next_key();
        let response = self.radio_shown(key, label.into(), selected == option);
        self.set_handler(Handler::Message(on_select(option)));
        response
    }

    #[track_caller]
    fn radio_shown(&mut self, key: WidgetKey, label: String, selected: bool) -> Response<'a> {
        self.control(
            key,
            Kind::Radio,
            format!("{}  {label}", if selected { "●" } else { "○" }),
            Style::button(),
            Control::Radio { label, selected },
        )
    }

    /// A slider showing `value` over `range`: a drag sends `on_change` of
    /// the value it reaches, snapped to `step`.
    #[track_caller]
    pub fn slider(
        &mut self,
        label: impl Into<String>,
        value: f64,
        range: RangeInclusive<f64>,
        step: f64,
        on_change: impl Fn(f64) -> M + 'static,
    ) -> Response<'a> {
        let (min, max) = (*range.start(), *range.end());
        if !(min.is_finite() && max.is_finite() && min < max && step.is_finite() && step > 0.0) {
            self.misuse("slider requires finite ordered bounds and a positive step".to_owned());
            return Response::default();
        }
        let key = self.next_key();
        let label = label.into();
        let value = slider_value(value, min, max, step);
        let response = self.control(
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
                value,
                min,
                max,
                step,
            },
        );
        self.set_handler(Handler::Map(Box::new(move |update| match update {
            ControlUpdate::Number(number) => Some(on_change(slider_value(number, min, max, step))),
            _ => None,
        })));
        response
    }

    /// A drop-down showing `selected` over `items`; a choice sends
    /// `on_choose(index)`. `ui.combo_box("Theme", choice, &items, Msg::Theme)`
    /// is the whole of it, and [`ComboBox::new`] carries a style.
    #[track_caller]
    pub fn combo_box(
        &mut self,
        combo: impl Into<ComboBox>,
        selected: usize,
        items: &[&str],
        on_choose: impl Fn(usize) -> M + 'static,
    ) -> Response<'a> {
        let combo = combo.into();
        let key = self.next_key();
        let selected = selected.min(items.len().saturating_sub(1));
        let response = self.combo_shown(combo, key, selected, items);
        self.set_handler(Handler::Map(Box::new(move |update| match update {
            ControlUpdate::Choice(index) => Some(on_choose(index)),
            _ => None,
        })));
        response
    }

    #[track_caller]
    pub(super) fn combo_shown(
        &mut self,
        combo: ComboBox,
        key: WidgetKey,
        selected: usize,
        items: &[&str],
    ) -> Response<'a> {
        let style = combo.style.unwrap_or_else(Self::field_style);
        let entries = items
            .iter()
            .map(|label| MenuItem::new(*label))
            .collect::<Vec<_>>();
        let children = self.popup_children(
            &key,
            Kind::ComboBox,
            &entries,
            selected,
            false,
            Length::Percent(1.0),
        );
        self.push(Element {
            text: format!("{}  ▾", items.get(selected).copied().unwrap_or("—")),
            style,
            enabled: self.enabled && !items.is_empty(),
            children,
            control: Control::Combo {
                label: combo.label,
                selected,
                items: entries,
            },
            ..Element::new(key, Kind::ComboBox)
        })
    }

    /// A popup menu; a pick sends `on_pick(index)`.
    #[track_caller]
    pub fn menu(
        &mut self,
        label: impl Into<String>,
        items: &[MenuItem],
        on_pick: impl Fn(usize) -> M + 'static,
    ) -> Response<'a> {
        let key = self.next_key();
        let response = self.menu_shown(
            key,
            format!("{}  ▾", label.into()),
            items,
            Self::menu_style(),
            Length::Percent(1.0),
        );
        self.set_handler(Handler::Map(Box::new(move |update| match update {
            ControlUpdate::Choice(index) => Some(on_pick(index)),
            _ => None,
        })));
        response
    }

    /// The look of a menu button.
    #[track_caller]
    fn menu_style() -> Style {
        Style {
            width: Length::Px(220.0),
            height: Length::Px(44.0),
            padding: 10.0,
            ..Style::button()
        }
    }

    /// A menu over picks the framework handles itself: the index of the
    /// item picked since the last declaration, if it is enabled.
    #[track_caller]
    pub(super) fn menu_pick(
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
        self.menu_shown(key, text, items, style, popup_width);
        choice
    }

    /// A menu with a caller-chosen button look and popup width.
    #[track_caller]
    pub(super) fn menu_shown(
        &mut self,
        key: WidgetKey,
        text: impl Into<String>,
        items: &[MenuItem],
        style: Style,
        popup_width: Length,
    ) -> Response<'a> {
        let children =
            self.popup_children(&key, Kind::MenuButton, items, usize::MAX, true, popup_width);
        self.push(Element {
            text: text.into(),
            style,
            enabled: self.enabled && items.iter().any(|item| item.enabled),
            children,
            control: Control::Menu {
                items: items.to_vec(),
            },
            ..Element::new(key, Kind::MenuButton)
        })
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
                        text: item.label.clone(),
                        style: Style {
                            width: Length::Percent(1.0),
                            height: Length::Px(32.0),
                            padding: 5.0,
                            background: (index == popup.highlighted).then_some(selection),
                            ..Style::default()
                        },
                        enabled: item.enabled,
                        control: Control::Choice {
                            hint: item.hint.clone(),
                            index,
                            selected: selected == index,
                            menu,
                        },
                        ..Element::new(WidgetKey::Named(index.to_string()), Kind::Choice)
                    });
                }
            },
        );
        child.elements[0].control = Control::Popup { menu };
        child.elements
    }
}

impl<'a, M: 'static> Ui<'a, M> {
    /// One command declaration per window, whose click sends `message`. Its
    /// shortcut and a menu pick of the command reach the same message, since
    /// they activate this button.
    #[track_caller]
    pub fn command_button(&mut self, command: &crate::Command, message: M) -> Response<'a> {
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
        self.set_handler(Handler::Message(message));
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
