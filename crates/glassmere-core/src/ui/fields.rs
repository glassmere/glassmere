use super::controls::FieldExtra;
use super::*;
use crate::app::Handler;
use crate::editor::single_line;
use crate::{Length, MenuItem, NumberOptions, NumericValue};

/// What an autocomplete field sends besides its edits; see
/// [`Ui::autocomplete_with`].
pub struct Autocomplete<M = ()> {
    on_pick: Option<Box<dyn Fn(usize) -> M>>,
    on_submit: Option<M>,
}

impl<M> Default for Autocomplete<M> {
    fn default() -> Self {
        Self {
            on_pick: None,
            on_submit: None,
        }
    }
}

impl<M> Autocomplete<M> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sent after the text change of a pick, with the index of the suggestion
    /// taken in the list that was offered.
    pub fn on_pick(mut self, message: impl Fn(usize) -> M + 'static) -> Self {
        self.on_pick = Some(Box::new(message));
        self
    }

    /// The message Enter sends when no suggestion is highlighted.
    pub fn on_submit(mut self, message: M) -> Self {
        self.on_submit = Some(message);
        self
    }
}

impl<'a, M: 'static> Ui<'a, M> {
    /// A text input that offers `suggestions` in a list below it while the
    /// user types. The app decides what matches: pass the entries for the
    /// current `value` each frame (see `suggest` for the simple case), from
    /// memory, an index or the latest answer of a remote search. Free text
    /// stays allowed. Up and Down move through the list, Enter or a click
    /// takes a suggestion, Escape closes the list, and Enter without a
    /// highlighted suggestion submits what was typed.
    ///
    /// An edit sends `on_change(text)`, and a pick from the list sends
    /// `on_change` of the suggestion it took.
    #[track_caller]
    pub fn autocomplete<S: AsRef<str>>(
        &mut self,
        label: impl Into<String>,
        value: &str,
        suggestions: &[S],
        on_change: impl Fn(String) -> M + 'static,
    ) -> Response<'a> {
        self.autocomplete_with(label, value, suggestions, on_change, Autocomplete::new())
    }

    /// `autocomplete` that also sends what `extra` names: the index of a
    /// pick (after its `on_change`) and Enter without a highlighted suggestion.
    #[track_caller]
    pub fn autocomplete_with<S: AsRef<str>>(
        &mut self,
        label: impl Into<String>,
        value: &str,
        suggestions: &[S],
        on_change: impl Fn(String) -> M + 'static,
        extra: Autocomplete<M>,
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
        let response = self.text_field(
            key,
            label.into(),
            value,
            "",
            Self::field_style(),
            FieldExtra {
                number: None,
                suggestions: Some(entries),
                children,
                ..FieldExtra::default()
            },
        );
        // Picking a suggestion edits the field to it, and the kernel reports
        // that pick as the index, which names the text in `labels`.
        let Autocomplete { on_pick, on_submit } = extra;
        self.set_handler(Handler::Autocomplete {
            change: Box::new(move |update, out| match update {
                ControlUpdate::Text(text) => out.push(on_change(single_line(&text))),
                ControlUpdate::Choice(index) => {
                    if let Some(label) = labels.get(index) {
                        out.push(on_change(label.clone()));
                        out.extend(on_pick.as_ref().map(|pick| pick(index)));
                    }
                }
                _ => {}
            }),
            submit: on_submit,
        });
        response
    }

    /// A number field showing `value`. Enter, leaving the field, a stepper,
    /// Up and Down, PageUp and PageDown and the wheel commit what was typed:
    /// it parses in the context's locale, clamps and snaps, and `on` of the
    /// number is sent when that differs from `value`. Text that is not a
    /// number reverts without a message.
    #[track_caller]
    pub fn number_input(
        &mut self,
        label: impl Into<String>,
        value: f64,
        options: NumberOptions,
        on: impl Fn(f64) -> M + 'static,
    ) -> Response<'a> {
        let label = label.into();
        let shown = options.constrain(value);
        let mut row = options.style.clone();
        if row.width == Length::Auto {
            row.width = Length::Px(160.0);
        }
        row.gap = 4.0;
        let mut result = Response::default();
        let key = self.next_key();
        let context = self.context;
        self.group(key, Kind::Row, row, |ui| {
            let field = ui
                .context
                .child(ui.parent, &WidgetKey::named("$field"), Kind::TextInput);
            // The app answering with the same number keeps a draft; any other
            // number replaces it.
            let mut text = options.format_in(ui.context, shown);
            let mut drafting = false;
            if let Some(state) = field.and_then(|id| ui.context.field_state(id))
                && state.draft
                && state.number.is_some_and(|last| last.value == shown)
            {
                text = state.text;
                drafting = true;
            }
            let number = NumericValue {
                value: shown,
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
                    &text,
                    "",
                    style,
                    FieldExtra {
                        number: Some(number),
                        draft: drafting,
                        ..FieldExtra::default()
                    },
                )
                .rebind(context);
            let (min, max) = (options.min, options.max);
            ui.set_handler(Handler::Number {
                on: Box::new(on),
                options: Box::new(options.clone()),
            });
            for (key, symbol, name, enabled, steps) in [
                (
                    "$less",
                    "\u{2212}",
                    ui.context.tr("glassmere-number-decrease", &[]),
                    shown > min,
                    -1.0,
                ),
                (
                    "$more",
                    "+",
                    ui.context.tr("glassmere-number-increase", &[]),
                    shown < max,
                    1.0,
                ),
            ] {
                ui.push(Element {
                    text: symbol.into(),
                    style: Style {
                        width: Length::Px(32.0),
                        height: Length::Px(44.0),
                        padding: 10.0,
                        ..Style::button()
                    },
                    enabled: ui.enabled && enabled,
                    tooltip: Some(name),
                    control: Control::Stepper,
                    ..Element::new(WidgetKey::Named(key.into()), Kind::Button)
                });
                ui.set_handler(Handler::Step(steps));
            }
        });
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

    /// A text field over text the framework keeps itself, such as a grid
    /// cell being edited: what was typed since the last declaration is
    /// written to `value`. Returns the field and whether `value` changed.
    #[track_caller]
    pub(crate) fn text_input_at(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        value: &mut String,
        placeholder: &str,
        style: Style,
    ) -> (Response<'a>, bool) {
        let key = WidgetKey::Named(key.into());
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
        let response = self.text_field(
            key,
            label.into(),
            value,
            placeholder,
            style,
            FieldExtra::default(),
        );
        (response, changed)
    }

    /// A drop-down over a choice the framework keeps itself. Returns the
    /// field and whether `selected` changed.
    #[track_caller]
    pub(crate) fn combo_box_at(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        selected: &mut usize,
        items: &[&str],
        style: Style,
    ) -> (Response<'a>, bool) {
        let key = WidgetKey::Named(key.into());
        let old = *selected;
        if let Some(ControlUpdate::Choice(index)) = self.take_update(&key, Kind::ComboBox) {
            *selected = index;
        }
        *selected = (*selected).min(items.len().saturating_sub(1));
        let response = self.combo_shown(ComboBox::new(label).style(style), key, *selected, items);
        (response, old != *selected)
    }

    /// Whether the button the framework is about to declare under `key` was
    /// clicked since the last declaration, for its own composites, whose
    /// buttons send no message.
    #[track_caller]
    pub(crate) fn clicked_at(&mut self, key: impl Into<String>) -> bool {
        let key = WidgetKey::Named(key.into());
        self.enabled
            && self
                .context
                .child(self.parent, &key, Kind::Button)
                .is_some_and(|id| self.clicks.remove(&id))
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
