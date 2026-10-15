use rust_gui::{
    AppOptions, DesktopHandle, DockLayout, DockNode, DockOptions, DockPanel, DockTarget,
    DockWindowId, DockWindowView, DockWindows, GraphicsOptions, Label, Length, Size, Style,
    TextWrap, Ui, WindowKey, run_windows,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

struct State {
    layout: DockLayout,
    notes: String,
    wrap: bool,
    saves: u32,
    /// Set by detached windows so the main window refreshes the others.
    stale: bool,
    save_to: Option<PathBuf>,
    main: Option<WindowKey>,
}

type Shared = Arc<Mutex<State>>;

fn default_layout() -> DockLayout {
    DockLayout::new(DockNode::row(
        0.22,
        DockNode::tabs(["files", "search"]),
        DockNode::row(
            0.72,
            DockNode::column(
                0.7,
                DockNode::tabs(["editor", "preview"]),
                DockNode::tabs(["output", "problems"]),
            ),
            DockNode::tabs(["inspector"]),
        ),
    ))
}

fn panels() -> Vec<DockPanel> {
    vec![
        DockPanel::pinned("editor", "Editor"),
        DockPanel::new("files", "Files"),
        DockPanel::new("search", "Search"),
        DockPanel::new("preview", "Preview"),
        DockPanel::new("output", "Output"),
        DockPanel::new("problems", "Problems"),
        DockPanel::new("inspector", "Inspector"),
    ]
}

fn panel(ui: &mut Ui<'_>, state: &mut State, id: &str) {
    let muted = ui.theme().text_muted;
    let text = |body: &str| {
        (
            body.to_owned(),
            Style::default()
                .wrap(TextWrap::Word)
                .width(Length::Percent(1.0))
                .foreground(muted),
        )
    };
    match id {
        "editor" => {
            ui.label("Notes");
            ui.text_input("Notes", &mut state.notes);
            let (body, style) = text(
                "Drag a tab onto a drop indicator, another tab strip or empty space. \
                 Right-click a tab, press Shift+F10 on it or use the ▾ button for the same moves.",
            );
            ui.label(Label::new(body).style(style));
        }
        "files" => {
            for name in ["Cargo.toml", "README.md", "src/main.rs", "src/lib.rs"] {
                ui.keyed(name, |ui| ui.button(name));
            }
        }
        "inspector" => {
            ui.checkbox("Wrap long lines", &mut state.wrap);
            ui.label(format!("Layout saved {} times", state.saves));
        }
        other => {
            let (body, style) = text(&format!("The {other} panel."));
            ui.label(Label::new(body).style(style));
        }
    }
}

fn changed(state: &mut State) {
    if let Some(path) = &state.save_to
        && std::fs::write(path, state.layout.to_text()).is_ok()
    {
        state.saves += 1;
    }
}

fn detached(
    shared: &Shared,
    desktop: &DesktopHandle,
    id: DockWindowId,
    size: Size,
) -> DockWindowView {
    let (view_state, close_state) = (shared.clone(), shared.clone());
    let (view_desktop, close_desktop) = (desktop.clone(), desktop.clone());
    DockWindowView {
        options: AppOptions {
            title: "Rust GUI / Docking (detached)".into(),
            size,
            ..AppOptions::default()
        },
        view: Box::new(move |ui| {
            let mut state = view_state.lock().unwrap();
            let state = &mut *state;
            let mut layout = std::mem::take(&mut state.layout);
            let response = ui.dock_window(
                &mut layout,
                id,
                &panels(),
                DockOptions {
                    native_windows: true,
                    ..DockOptions::default()
                },
                |ui, panel_id| panel(ui, state, panel_id),
            );
            state.layout = layout;
            if response.changed {
                changed(state);
                state.stale = true;
                if let Some(main) = state.main {
                    view_desktop.redraw(main);
                }
            }
        }),
        // Closing a detached window returns its panels to the main window.
        on_close: Box::new(move || {
            let mut state = close_state.lock().unwrap();
            if state.layout.redock(id) {
                changed(&mut state);
            }
            if let Some(main) = state.main {
                close_desktop.redraw(main);
            }
        }),
    }
}

// M35: stays on the closure API. Detached dock windows are `DockWindows` over
// `DesktopHandle::open_window`, and each one's `on_close` returns its panels to
// the layout. `App` has no message for a window closing, so under `run_app` a
// closed detached panel could never go back. The main window's dock would
// convert with `Ui::dock_msg` once that exists.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let reset = std::env::args().any(|arg| arg == "--reset");
    let open = std::env::args().any(|arg| arg == "--open");
    let path = std::env::temp_dir().join("rust-gui-docking.layout");
    let mut layout = std::fs::read_to_string(&path)
        .ok()
        .filter(|_| !reset && !smoke && !open)
        .and_then(|text| DockLayout::parse(&text).ok())
        .unwrap_or_else(default_layout);
    if open {
        layout.move_panel(
            "preview",
            DockTarget::Float {
                x: 300.0,
                y: 140.0,
                width: 320.0,
                height: 220.0,
            },
        );
    }
    let shared: Shared = Arc::new(Mutex::new(State {
        layout,
        notes: String::from("Docking keeps this text while panels move."),
        wrap: true,
        saves: 0,
        stale: false,
        save_to: (!smoke && !open).then_some(path),
        main: None,
    }));
    run_windows(GraphicsOptions::default(), move |desktop| {
        let handle = desktop.clone();
        let main_state = shared.clone();
        let mut windows = DockWindows::new();
        let key = desktop
            .open_window(
                AppOptions {
                    title: "Rust GUI / Docking".into(),
                    size: Size::new(1100.0, 760.0),
                    frame_limit: smoke.then_some(3),
                    ..AppOptions::default()
                },
                move |ui| {
                    let mut guard = main_state.lock().unwrap();
                    let state = &mut *guard;
                    let page = Style::default()
                        .width(Length::Percent(1.0))
                        .height(Length::Percent(1.0))
                        .padding(12.0)
                        .gap(8.0);
                    let mut layout = std::mem::take(&mut state.layout);
                    let mut dirty = false;
                    ui.column(page, |ui| {
                        ui.row(Style::default().gap(8.0), |ui| {
                            if ui.button("Reset layout").clicked() {
                                layout = default_layout();
                                dirty = true;
                            }
                            if ui.button("Show all panels").clicked() {
                                for panel in panels() {
                                    layout.show(&panel.id);
                                }
                                dirty = true;
                            }
                        });
                        let response = ui.dock(
                            &mut layout,
                            &panels(),
                            DockOptions {
                                style: Style::default()
                                    .width(Length::Percent(1.0))
                                    .grow(1.0)
                                    .shrink(1.0),
                                native_windows: true,
                                ..DockOptions::default()
                            },
                            |ui, id| panel(ui, state, id),
                        );
                        dirty |= response.changed;
                    });
                    state.layout = layout;
                    if dirty {
                        changed(state);
                    }
                    if dirty || std::mem::take(&mut state.stale) {
                        windows.redraw(&handle);
                    }
                    windows.sync(&handle, &state.layout, |window| {
                        detached(
                            &main_state,
                            &handle,
                            window.id,
                            Size::new(window.width, window.height),
                        )
                    });
                },
            )
            .expect("the desktop is running during setup");
        shared.lock().unwrap().main = Some(key);
    })
}
