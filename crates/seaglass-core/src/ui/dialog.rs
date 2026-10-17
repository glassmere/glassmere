use super::*;

impl<'a, M: 'static> Ui<'a, M> {
    /// A dialog layer over the whole window while `open`. Modal dialogs paint
    /// a scrim, block pointer and wheel input behind them and trap Tab; focus
    /// moves inside on open and returns to the previously focused node on
    /// close. Escape or a scrim click sends `on_open(false)`. Dialog keys are
    /// unique across the whole view, and later dialogs stack on top.
    #[track_caller]
    pub fn dialog(
        &mut self,
        key: impl Into<String>,
        open: bool,
        options: crate::DialogOptions,
        on_open: impl Fn(bool) -> M + 'static,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        self.dialog_declared(
            key.into(),
            open,
            options,
            content,
            super::overlays::closing(on_open),
        );
    }

    /// A dialog layer whose dismissal sends what `dismiss` makes of it.
    #[track_caller]
    pub(super) fn dialog_declared(
        &mut self,
        key: String,
        open: bool,
        options: crate::DialogOptions,
        content: impl FnOnce(&mut Ui<'_, M>),
        dismiss: crate::app::Handler<M>,
    ) {
        let key = WidgetKey::Named(format!("$dialog:{key}"));
        let layer = self
            .context
            .child(Some(self.context.root()), &key, Kind::Dialog);
        if !open {
            return;
        }
        let popup = self.context.theme().popup;
        let mut panel = options.style;
        panel.width = options.width;
        if panel.background.is_none() {
            panel.background = Some(popup);
        }
        if panel.radius == 0.0 {
            panel.radius = 12.0;
        }
        if panel.padding == 0.0 {
            panel.padding = 20.0;
        }
        let index = self.layers.len();
        let enabled = self.enabled;
        let mut child = self.scope(layer);
        let title = options.title.clone();
        child.group(WidgetKey::named("$panel"), Kind::Container, panel, |ui| {
            if !title.is_empty() {
                ui.label_at(
                    "$title",
                    title.clone(),
                    Style {
                        font_size: Some(20.0),
                        font_weight: Some(700),
                        ..Style::default()
                    },
                );
            }
            content(ui);
        });
        let mut element = Element {
            handler: None,
            seq: 0,
            key,
            kind: Kind::Dialog,
            text: String::new(),
            style: Style {
                width: crate::Length::Percent(1.0),
                height: crate::Length::Percent(1.0),
                align_items: crate::Align::Center,
                justify_content: crate::Justify::Center,
                gap: 0.0,
                ..Style::default()
            },
            enabled,
            children: child.elements,
            custom: None,
            tooltip: None,
            control: Control::Dialog {
                title: options.title,
                modal: options.modal,
                dismissible: options.dismissible,
            },
        };
        // Nested dialogs declared inside `content` were pushed meanwhile; keep
        // this one below them so children stack on top of their parents.
        element.handler = Some(self.handler_index(dismiss));
        self.layers.insert(index, element);
    }
}
