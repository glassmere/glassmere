use rust_gui::{
    AppOptions, Button, DesktopHandle, DialogOptions, FocusScope, GraphicsOptions, Label, Length,
    Size, Style, TextWrap, Ui, WindowKey, run_windows,
};
use std::sync::{Arc, Mutex};

struct State {
    confirm: bool,
    nested: bool,
    preferences: bool,
    deleted: u32,
    dismissed: u32,
    name: String,
    autosave: bool,
    main: Option<WindowKey>,
    native: Option<WindowKey>,
    native_results: u32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let open = std::env::args().any(|arg| arg == "--open");
    let state = Arc::new(Mutex::new(State {
        confirm: smoke || open,
        nested: false,
        preferences: false,
        deleted: 0,
        dismissed: 0,
        name: "Untitled".into(),
        autosave: true,
        main: None,
        native: None,
        native_results: 0,
    }));
    run_windows(GraphicsOptions::default(), move |desktop| {
        let shared = state.clone();
        let handle = desktop.clone();
        let key = desktop
            .open_window(
                AppOptions {
                    title: "Rust GUI / Dialogs and focus".into(),
                    size: Size::new(900.0, 720.0),
                    frame_limit: smoke.then_some(4),
                    ..AppOptions::default()
                },
                move |ui| {
                    let mut state = shared.lock().unwrap();
                    main_window(ui, &handle, &shared, &mut state, smoke);
                },
            )
            .expect("the desktop is open during setup");
        state.lock().unwrap().main = Some(key);
        if smoke {
            open_native(&desktop, &state, &mut state.lock().unwrap(), smoke);
        }
    })?;
    if smoke {
        println!("DIALOGS_SMOKE_OK");
    }
    Ok(())
}

fn main_window(
    ui: &mut Ui<'_>,
    desktop: &DesktopHandle,
    shared: &Arc<Mutex<State>>,
    state: &mut State,
    smoke: bool,
) {
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
            if ui.button("Delete file…").clicked() {
                state.confirm = true;
            }
            if ui.button("Preferences (non-modal)").clicked() {
                state.preferences = true;
            }
            if ui.button("Native modal window…").clicked() {
                open_native(desktop, shared, state, smoke);
            }
        });
        ui.label(format!("Deleted {}  ·  dismissed {}  ·  native results {}  ·  name \"{}\"", state.deleted, state.dismissed, state.native_results, state.name));

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
        &mut state.confirm,
        DialogOptions::new("Delete file?"),
        |ui, open| {
            ui.label(
                Label::new("The file will be removed permanently. This cannot be undone.").style(
                    Style::default()
                        .wrap(TextWrap::Word)
                        .width(Length::Percent(1.0)),
                ),
            );
            ui.row(Style::default().gap(8.0), |ui| {
                if ui.button("Cancel").clicked() {
                    *open = false;
                }
                if ui
                    .button(
                        Button::new("Delete").style(
                            Style::button()
                                .background(theme.accent)
                                .foreground(theme.window),
                        ),
                    )
                    .clicked()
                {
                    state.deleted += 1;
                    *open = false;
                }
                if ui.button("More options…").clicked() {
                    state.nested = true;
                }
            });
            ui.dialog(
                "nested",
                &mut state.nested,
                DialogOptions::new("More options").width(Length::Px(320.0)),
                |ui, open| {
                    ui.checkbox("Keep a backup copy", &mut state.autosave);
                    if ui.button("Done").clicked() {
                        *open = false;
                    }
                },
            );
        },
    );
    let response = ui.dialog(
        "preferences",
        &mut state.preferences,
        DialogOptions::new("Preferences")
            .modal(false)
            .width(Length::Px(360.0)),
        |ui, open| {
            ui.label("Document name");
            ui.text_input("Document name", &mut state.name);
            ui.checkbox("Autosave", &mut state.autosave);
            if ui.button("Close").clicked() {
                *open = false;
            }
        },
    );
    if response.dismissed {
        state.dismissed += 1;
    }
}

fn open_native(
    desktop: &DesktopHandle,
    shared: &Arc<Mutex<State>>,
    state: &mut State,
    smoke: bool,
) {
    let (Some(owner), None) = (state.main, state.native) else {
        return;
    };
    let shared = shared.clone();
    let handle = desktop.clone();
    state.native = desktop
        .open_modal(
            owner,
            AppOptions {
                title: "Native modal".into(),
                size: Size::new(420.0, 220.0),
                frame_limit: smoke.then_some(3),
                ..AppOptions::default()
            },
            move |ui| {
                let mut state = shared.lock().unwrap();
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
                        for (key, label) in [("ok", "OK"), ("cancel", "Cancel")] {
                            if ui.keyed(key, |ui| ui.button(label)).clicked() {
                                if key == "ok" {
                                    state.native_results += 1;
                                }
                                if let Some(native) = state.native.take() {
                                    handle.close_window(native);
                                }
                            }
                        }
                    });
                });
            },
        )
        .ok();
}
