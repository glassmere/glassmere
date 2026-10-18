use super::*;
use crate::{Align, Effect, Length, Toast, ToastId, ToastKind, ToastSender, Transition};
use seaglass_scene::{Color, TextWrap};

/// What a declaration did with toasts; applied by the context afterwards.
#[derive(Default)]
pub(crate) struct ToastOutbox {
    pub raised: Vec<(ToastId, Toast)>,
    /// Dismissed toasts, and whether through their action button.
    pub closed: Vec<(ToastId, bool)>,
}

// Fixed colours: the theme has no toast tokens.
fn accent(kind: ToastKind, info: Color) -> Color {
    match kind {
        ToastKind::Info => info,
        ToastKind::Success => Color::rgb(52, 199, 123),
        ToastKind::Warning => Color::rgb(245, 166, 35),
        ToastKind::Error => Color::rgb(235, 87, 87),
    }
}

impl<'a, M: 'static> Ui<'a, M> {
    /// For raising toasts in this window from other threads.
    #[track_caller]
    pub fn toast_sender(&self) -> ToastSender {
        self.context.toast_sender()
    }

    /// The toast stack, declared by the context after the app's view as the
    /// topmost layer.
    #[track_caller]
    pub(crate) fn declare_toasts(&mut self) {
        let toasts: Vec<(ToastId, Toast)> = self
            .context
            .toasts()
            .map(|(id, toast)| (id, toast.clone()))
            .collect();
        if toasts.is_empty() {
            return;
        }
        let theme = self.context.theme().clone();
        let stack = Style {
            gap: 8.0,
            align_items: Align::End,
            ..Style::default()
        };
        let mut closed = Vec::new();
        self.group(WidgetKey::named("$toasts"), Kind::Toasts, stack, |ui| {
            for (id, toast) in &toasts {
                let panel = Style {
                    width: Length::Px(360.0),
                    padding: 12.0,
                    gap: 10.0,
                    radius: 10.0,
                    align_items: Align::Center,
                    background: Some(theme.popup),
                    enter: Some(Effect::slide(0.0, 16.0, 180)),
                    exit: Some(Effect::fade(160)),
                    layout_transition: Some(Transition::new(160)),
                    ..Style::default()
                };
                ui.group(
                    WidgetKey::named(format!("{}", id.0)),
                    Kind::Toast,
                    panel,
                    |ui| {
                        ui.container_at(
                            "$accent",
                            Style {
                                width: Length::Px(4.0),
                                align_self: Some(Align::Stretch),
                                radius: 2.0,
                                background: Some(accent(toast.kind, theme.accent)),
                                ..Style::default()
                            },
                            |_| {},
                        );
                        ui.label_at(
                            "$text",
                            toast.text.clone(),
                            Style {
                                grow: 1.0,
                                shrink: 1.0,
                                wrap: TextWrap::Word,
                                ..Style::default()
                            },
                        );
                        let compact = Style {
                            padding: 8.0,
                            ..Style::button()
                        };
                        if let Some(action) = &toast.action {
                            if ui.clicked_at("$action") {
                                closed.push((*id, true));
                            }
                            ui.button_at("$action", action.clone(), true, compact.clone());
                        }
                        if ui.clicked_at("$close") {
                            closed.push((*id, false));
                        }
                        ui.button_at("$close", "\u{00d7}", true, compact);
                        ui.tooltip(ui.context.tr("seaglass-toast-dismiss", &[]));
                    },
                );
                ui.set_control(Control::Toast {
                    label: toast.text.clone(),
                    assertive: toast.kind == ToastKind::Error,
                });
            }
        });
        // Above dialogs: a toast raised from a modal dialog must be visible.
        if let Some(stack) = self.take_declared() {
            self.layers.push(stack);
        }
        self.host.toasts.closed.extend(closed);
    }
}
