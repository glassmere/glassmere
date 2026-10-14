use super::*;
use crate::{
    Align, Effect, Length, Toast, ToastAction, ToastId, ToastKind, ToastSender, Transition,
};
use rust_gui_scene::{Color, TextWrap};

/// What a declaration did with toasts; applied by the context afterwards.
#[derive(Default)]
pub(crate) struct ToastOutbox {
    pub raised: Vec<(ToastId, Toast)>,
    /// Dismissed toasts, and whether through their action button.
    pub closed: Vec<(ToastId, bool)>,
    /// Actions handed to the app by `Ui::toast_action`.
    pub taken: usize,
}

// ponytail: fixed colours; theme tokens when apps need to restyle toasts.
fn accent(kind: ToastKind, info: Color) -> Color {
    match kind {
        ToastKind::Info => info,
        ToastKind::Success => Color::rgb(52, 199, 123),
        ToastKind::Warning => Color::rgb(245, 166, 35),
        ToastKind::Error => Color::rgb(235, 87, 87),
    }
}

impl Ui<'_> {
    /// Shows a short message in the bottom corner at the end of the reading
    /// direction. It leaves after its duration, which pauses while the
    /// pointer or the keyboard is on any toast. Four show at once and later
    /// ones wait. A toast equal to one already there restarts it instead of
    /// stacking. Call it when something happens, not every frame.
    #[track_caller]
    pub fn toast(&mut self, toast: Toast) -> ToastId {
        let id = self.context.next_toast_id();
        self.host.toasts.raised.push((id, toast));
        *self.redraw = true;
        id
    }

    /// The next click on a toast's action button, once.
    #[track_caller]
    pub fn toast_action(&mut self) -> Option<ToastAction> {
        let action = self.context.toast_action_at(self.host.toasts.taken)?;
        self.host.toasts.taken += 1;
        Some(action)
    }

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
                        if let Some(action) = &toast.action
                            && ui
                                .button_at("$action", action.clone(), true, compact.clone())
                                .clicked()
                        {
                            closed.push((*id, true));
                        }
                        if ui.button_at("$close", "\u{00d7}", true, compact).clicked() {
                            closed.push((*id, false));
                        }
                        ui.tooltip(ui.context.tr("rust-gui-toast-dismiss", &[]));
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
