use rust_gui::{
    App, AppOptions, Button, DockAction, DockLayout, DockNode, DockOptions, DockPanel, DockTarget,
    DockWindowId, Effects, Label, Length, Size, Style, TextWrap, Ui, WindowId, WindowRequest, run,
};
use std::path::PathBuf;

#[derive(Clone)]
enum Msg {
    Dock(DockAction),
    Reset,
    ShowAll,
    Notes(String),
    Wrap(bool),
}

struct Docking {
    layout: DockLayout,
    notes: String,
    wrap: bool,
    saves: u32,
    save_to: Option<PathBuf>,
    /// The OS window shown for each detached dock window of the layout.
    windows: Vec<(DockWindowId, WindowId)>,
}

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

impl Docking {
    fn panel(&self, ui: &mut Ui<'_, Msg>, id: &str) {
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
                ui.text_input("Notes", &self.notes, Msg::Notes);
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
                ui.checkbox("Wrap long lines", self.wrap, Msg::Wrap);
                ui.label(format!("Layout saved {} times", self.saves));
            }
            other => {
                let (body, style) = text(&format!("The {other} panel."));
                ui.label(Label::new(body).style(style));
            }
        }
    }

    fn options() -> DockOptions {
        DockOptions {
            native_windows: true,
            ..DockOptions::default()
        }
    }

    /// Saves a changed layout, and opens and closes the OS windows to match
    /// the layout's detached ones. Windows closed here are not reported back
    /// through `window_closed`.
    fn layout_changed(&mut self, effects: &mut Effects<'_, Msg>) {
        if let Some(path) = &self.save_to
            && std::fs::write(path, self.layout.to_text()).is_ok()
        {
            self.saves += 1;
        }
        self.sync_windows(effects);
    }

    fn sync_windows(&mut self, effects: &mut Effects<'_, Msg>) {
        let layout = &self.layout;
        self.windows.retain(|(id, window)| {
            let keep = layout.window(*id).is_some();
            if !keep {
                effects.close_window(*window);
            }
            keep
        });
        for dock in &self.layout.windows {
            if self.windows.iter().any(|(id, _)| *id == dock.id) {
                continue;
            }
            let window = effects.open_window(WindowRequest {
                title: "Rust GUI / Docking (detached)".into(),
                size: Some(Size::new(dock.width, dock.height)),
                modal_owner: None,
            });
            self.windows.push((dock.id, window));
        }
    }
}

impl App for Docking {
    type Message = Msg;

    fn init(&mut self, effects: &mut Effects<'_, Msg>) {
        self.sync_windows(effects);
    }

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        let changed = match message {
            Msg::Dock(action) => self.layout.apply(action),
            Msg::Reset => {
                self.layout = default_layout();
                true
            }
            Msg::ShowAll => {
                for panel in panels() {
                    self.layout.show(&panel.id);
                }
                true
            }
            Msg::Notes(notes) => {
                self.notes = notes;
                false
            }
            Msg::Wrap(wrap) => {
                self.wrap = wrap;
                false
            }
        };
        if changed {
            self.layout_changed(effects);
        }
    }

    // Closing a detached window returns its panels to the main window.
    fn window_closed(&mut self, window: WindowId, effects: &mut Effects<'_, Msg>) {
        let Some(at) = self.windows.iter().position(|(_, shown)| *shown == window) else {
            return;
        };
        let (id, _) = self.windows.remove(at);
        if self.layout.redock(id) {
            self.layout_changed(effects);
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        let page = Style::default()
            .width(Length::Percent(1.0))
            .height(Length::Percent(1.0))
            .padding(12.0)
            .gap(8.0);
        ui.column(page, |ui| {
            ui.row(Style::default().gap(8.0), |ui| {
                ui.button(Button::new("Reset layout").on_click(Msg::Reset));
                ui.button(Button::new("Show all panels").on_click(Msg::ShowAll));
            });
            ui.dock(
                &self.layout,
                &panels(),
                DockOptions {
                    style: Style::default()
                        .width(Length::Percent(1.0))
                        .grow(1.0)
                        .shrink(1.0),
                    ..Self::options()
                },
                Msg::Dock,
                |ui, id| self.panel(ui, id),
            );
        });
    }

    fn view_window(&self, window: WindowId, ui: &mut Ui<'_, Msg>) {
        let Some((id, _)) = self.windows.iter().find(|(_, shown)| *shown == window) else {
            return;
        };
        ui.dock_window(
            &self.layout,
            *id,
            &panels(),
            Self::options(),
            Msg::Dock,
            |ui, panel| self.panel(ui, panel),
        );
    }
}

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
    run(
        AppOptions {
            title: "Rust GUI / Docking".into(),
            size: Size::new(1100.0, 760.0),
            frame_limit: smoke.then_some(3),
            ..AppOptions::default()
        },
        Docking {
            layout,
            notes: String::from("Docking keeps this text while panels move."),
            wrap: true,
            saves: 0,
            save_to: (!smoke && !open).then_some(path),
            windows: Vec::new(),
        },
    )
}
