use seaglass::{
    App, AppOptions, Button, DialogOptions, Effects, FocusScope, Label, Length, Size, Style,
    TextWrap, Ui, WindowId, WindowRequest, run,
};

/// The window `run` opens first. Its id is 0 by construction; the native
/// modal names it as its owner.
const MAIN: WindowId = WindowId(0);

#[derive(Clone)]
enum Msg {
    /// The confirm dialog opened or closed.
    Confirm(bool),
    Delete,
    MoreOptions,
    /// The nested dialog opened or closed.
    Nested(bool),
    /// The preferences dialog was opened, or dismissed with Escape or a click
    /// outside it.
    Preferences(bool),
    ClosePreferences,
    Name(String),
    Autosave(bool),
    OpenNative,
    CloseNative {
        ok: bool,
    },
}

struct Dialogs {
    smoke: bool,
    confirm: bool,
    nested: bool,
    preferences: bool,
    deleted: u32,
    dismissed: u32,
    name: String,
    autosave: bool,
    native: Option<WindowId>,
    native_results: u32,
}

impl Dialogs {
    fn open_native(&mut self, effects: &mut Effects<'_, Msg>) {
        if self.native.is_some() {
            return;
        }
        self.native = Some(effects.open_window(WindowRequest {
            title: "Native modal".into(),
            size: Some(Size::new(420.0, 220.0)),
            modal_owner: Some(MAIN),
        }));
    }
}

impl App for Dialogs {
    type Message = Msg;

    fn init(&mut self, effects: &mut Effects<'_, Msg>) {
        if self.smoke {
            self.open_native(effects);
        }
    }

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Confirm(open) => self.confirm = open,
            Msg::Delete => {
                self.deleted += 1;
                self.confirm = false;
            }
            Msg::MoreOptions => self.nested = true,
            Msg::Nested(open) => self.nested = open,
            Msg::Preferences(open) => {
                if !open {
                    self.dismissed += 1;
                }
                self.preferences = open;
            }
            Msg::ClosePreferences => self.preferences = false,
            Msg::Name(name) => self.name = name,
            Msg::Autosave(on) => self.autosave = on,
            Msg::OpenNative => self.open_native(effects),
            Msg::CloseNative { ok } => {
                if ok {
                    self.native_results += 1;
                }
                if let Some(native) = self.native.take() {
                    effects.close_window(native);
                }
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        let theme = ui.theme().clone();
        let muted = Style::default()
            .font_size(13.0)
            .foreground(theme.text_muted)
            .wrap(TextWrap::Word)
            .width(Length::Percent(1.0));
        let heading = Style::default().font_weight(700);
        ui.column(Style::default().width(Length::Percent(1.0)).padding(28.0).gap(16.0), |ui| {
            ui.label(Label::new("Dialogs and focus").style(Style::default().font_size(28.0).font_weight(700)));
            ui.label(Label::new("Modal dialogs trap Tab, block the content behind them and return focus when they close. Focus scopes remember their last child and can move focus with arrow keys. Escape or the scrim dismisses.").style(muted.clone()));
            ui.row(Style::default().gap(8.0), |ui| {
                ui.button(Button::new("Delete file…").on_click(Msg::Confirm(true)));
                ui.button(Button::new("Preferences (non-modal)").on_click(Msg::Preferences(true)));
                ui.button(Button::new("Native modal window…").on_click(Msg::OpenNative));
            });
            ui.label(format!("Deleted {}  ·  dismissed {}  ·  native results {}  ·  name \"{}\"", self.deleted, self.dismissed, self.native_results, self.name));

            ui.label(Label::new("Arrow-key toolbar  ·  focus_scope(arrows: true)").style(heading.clone()));
            ui.focus_scope(FocusScope { arrows: true, ..FocusScope::default() }, Style::default(), |ui| {
                ui.row(Style::default().gap(8.0), |ui| {
                    for tool in ["Cut", "Copy", "Paste", "Find"] {
                        ui.keyed(tool.to_lowercase(), |ui| ui.button(tool));
                    }
                });
                ui.row(Style::default().gap(8.0), |ui| {
                    for tool in ["Undo", "Redo", "Format"] {
                        ui.keyed(tool.to_lowercase(), |ui| ui.button(tool));
                    }
                });
            });
            ui.label(Label::new("Remembered scope  ·  Tab away and back lands on the last used button").style(heading.clone()));
            ui.focus_scope(FocusScope::default(), Style::default(), |ui| {
                ui.row(Style::default().gap(8.0), |ui| {
                    for choice in ["Alpha", "Beta", "Gamma"] {
                        ui.keyed(choice.to_lowercase(), |ui| ui.button(choice));
                    }
                });
            });
            ui.button("A button after the scopes");
            ui.label(Label::new("Native modal windows block their owner until closed and, on Windows, stay above it as owned windows.").style(muted));
        });

        ui.dialog(
            "confirm",
            self.confirm,
            DialogOptions::new("Delete file?"),
            Msg::Confirm,
            |ui| {
                ui.label(
                    Label::new("The file will be removed permanently. This cannot be undone.")
                        .style(
                            Style::default()
                                .wrap(TextWrap::Word)
                                .width(Length::Percent(1.0)),
                        ),
                );
                ui.row(Style::default().gap(8.0), |ui| {
                    ui.button(Button::new("Cancel").on_click(Msg::Confirm(false)));
                    ui.button(
                        Button::new("Delete")
                            .style(
                                Style::button()
                                    .background(theme.accent)
                                    .foreground(theme.window),
                            )
                            .on_click(Msg::Delete),
                    );
                    ui.button(Button::new("More options…").on_click(Msg::MoreOptions));
                });
                ui.dialog(
                    "nested",
                    self.nested,
                    DialogOptions::new("More options").width(Length::Px(320.0)),
                    Msg::Nested,
                    |ui| {
                        ui.checkbox("Keep a backup copy", self.autosave, Msg::Autosave);
                        ui.button(Button::new("Done").on_click(Msg::Nested(false)));
                    },
                );
            },
        );
        ui.dialog(
            "preferences",
            self.preferences,
            DialogOptions::new("Preferences")
                .modal(false)
                .width(Length::Px(360.0)),
            Msg::Preferences,
            |ui| {
                ui.label("Document name");
                ui.text_input("Document name", &self.name, Msg::Name);
                ui.checkbox("Autosave", self.autosave, Msg::Autosave);
                ui.button(Button::new("Close").on_click(Msg::ClosePreferences));
            },
        );
    }

    fn view_window(&self, _window: WindowId, ui: &mut Ui<'_, Msg>) {
        // The only window besides the main one is the native modal.
        ui.column(Style::default().padding(24.0).gap(12.0), |ui| {
            ui.label(
                Label::new("Native modal window")
                    .style(Style::default().font_size(20.0).font_weight(700)),
            );
            ui.label(
                Label::new("The owner window ignores input while this is open.").style(
                    Style::default()
                        .wrap(TextWrap::Word)
                        .width(Length::Px(370.0)),
                ),
            );
            ui.row(Style::default().gap(8.0), |ui| {
                ui.keyed("ok", |ui| {
                    ui.button(Button::new("OK").on_click(Msg::CloseNative { ok: true }))
                });
                ui.keyed("cancel", |ui| {
                    ui.button(Button::new("Cancel").on_click(Msg::CloseNative { ok: false }))
                });
            });
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let open = std::env::args().any(|arg| arg == "--open");
    run(
        AppOptions {
            title: "Seaglass / Dialogs and focus".into(),
            size: Size::new(900.0, 720.0),
            frame_limit: smoke.then_some(4),
            ..AppOptions::default()
        },
        Dialogs {
            smoke,
            confirm: smoke || open,
            nested: false,
            preferences: false,
            deleted: 0,
            dismissed: 0,
            name: "Untitled".into(),
            autosave: true,
            native: None,
            native_results: 0,
        },
    )?;
    if smoke {
        println!("DIALOGS_SMOKE_OK");
    }
    Ok(())
}
