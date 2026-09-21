use super::*;
use crate::{Length, MenuItem, controls::slider_value, editor::single_line};
use std::ops::RangeInclusive;

impl Ui<'_> {
    pub(super) fn take_update(&mut self, key: &str, kind: Kind) -> Option<ControlUpdate> {
        let id = self
            .context
            .child(self.parent, &WidgetKey::Named(key.into()), kind)?;
        let update = self.updates.remove(&id);
        self.enabled.then_some(update).flatten()
    }

    fn control(
        &mut self,
        key: String,
        kind: Kind,
        text: String,
        style: Style,
        control: Control,
    ) -> Response {
        self.push(Element {
            key: WidgetKey::Named(key),
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

    pub fn text_input(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        value: &mut String,
    ) -> Response {
        self.text_input_styled(
            key,
            label,
            value,
            "",
            Style {
                width: Length::Px(280.0),
                height: Length::Px(44.0),
                padding: 10.0,
                ..Style::button()
            },
        )
    }

    pub fn text_input_styled(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        value: &mut String,
        placeholder: &str,
        mut style: Style,
    ) -> Response {
        let key = key.into();
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
        let mut response = self.control(
            key,
            Kind::TextInput,
            String::new(),
            style,
            Control::TextInput {
                label: label.into(),
                value: value.clone(),
                placeholder: placeholder.into(),
            },
        );
        response.changed = changed;
        response
    }

    pub fn checkbox(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        checked: &mut bool,
    ) -> Response {
        let key = key.into();
        let label = label.into();
        let id = self
            .context
            .child(self.parent, &WidgetKey::Named(key.clone()), Kind::Checkbox);
        let changed = self.enabled && id.is_some_and(|id| self.clicks.remove(&id));
        if changed {
            *checked = !*checked;
        }
        let mut response = self.control(
            key,
            Kind::Checkbox,
            format!("{}  {label}", if *checked { "☑" } else { "☐" }),
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

    pub fn radio(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        selected: &mut usize,
        option: usize,
    ) -> Response {
        let key = key.into();
        let label = label.into();
        let id = self
            .context
            .child(self.parent, &WidgetKey::Named(key.clone()), Kind::Radio);
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

    pub fn slider(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        value: &mut f64,
        range: RangeInclusive<f64>,
        step: f64,
    ) -> Response {
        let (min, max) = (*range.start(), *range.end());
        assert!(
            min.is_finite() && max.is_finite() && min < max && step.is_finite() && step > 0.0,
            "slider requires finite ordered bounds and a positive step"
        );
        let key = key.into();
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

    pub fn combo_box(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        selected: &mut usize,
        items: &[&str],
    ) -> Response {
        let key = key.into();
        let label = label.into();
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
            key: WidgetKey::Named(key),
            kind: Kind::ComboBox,
            text: format!("{}  ▾", items.get(*selected).copied().unwrap_or("—")),
            style: Style {
                width: Length::Px(280.0),
                height: Length::Px(44.0),
                padding: 10.0,
                ..Style::button()
            },
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
    pub fn menu(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        items: &[MenuItem],
    ) -> Option<usize> {
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
    pub(super) fn menu_styled(
        &mut self,
        key: impl Into<String>,
        text: impl Into<String>,
        items: &[MenuItem],
        style: Style,
        popup_width: Length,
    ) -> Option<usize> {
        let key = key.into();
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
            key: WidgetKey::Named(key),
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

    pub(super) fn popup_children(
        &mut self,
        key: &str,
        kind: Kind,
        items: &[MenuItem],
        selected: usize,
        menu: bool,
        width: Length,
    ) -> Vec<Element> {
        let owner = self
            .context
            .child(self.parent, &WidgetKey::Named(key.into()), kind);
        let Some(popup) = self
            .context
            .popup
            .filter(|popup| Some(popup.owner) == owner && self.enabled)
        else {
            return Vec::new();
        };
        let mut child = Ui {
            context: self.context,
            parent: owner,
            clicks: self.clicks,
            elements: Vec::new(),
            keys: HashSet::new(),
            updates: self.updates,
            submitted: self.submitted,
            warnings: self.warnings,
            theme_request: self.theme_request,
            clipboard: self.clipboard,
            redraw: self.redraw,
            focus: self.focus,
            host: self.host,
            layers: self.layers,
            enabled: self.enabled,
        };
        let selection = self.context.theme().selection;
        child.group(
            "$popup".into(),
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
