use rust_gui::{
    AppOptions, Axis, Length, MenuItem, PopoverOptions, Size, SplitOptions, Style, Tab, TextWrap,
    run,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let open = std::env::args().any(|arg| arg == "--open");
    let mut tabs = vec![
        Tab::new("Overview"),
        Tab::closable("notes.md"),
        Tab::closable("main.rs"),
    ];
    let mut selected = 0usize;
    let mut sidebar = 0.28f32;
    let mut editor_split = 0.65f32;
    let mut share = open;
    let mut link = String::from("https://example.test/doc/42");
    let mut last_menu = String::from("none");
    let mut closed = 0u32;
    run(
        AppOptions {
            title: "Rust GUI / Tabs, panes and overlays".into(),
            size: Size::new(960.0, 720.0),
            frame_limit: smoke.then_some(3),
            ..AppOptions::default()
        },
        move |ui| {
            let theme = ui.theme().clone();
            let muted = Style::default()
                .font_size(13.0)
                .foreground(theme.text_muted)
                .wrap(TextWrap::Word)
                .width(Length::Percent(1.0));
            ui.column("page", Style::default().width(Length::Percent(1.0)).height(Length::Percent(1.0)).padding(20.0).gap(12.0), |ui| {
                ui.row("toolbar", Style::default().gap(8.0), |ui| {
                    ui.button_keyed("new", "New");
                    ui.tooltip("Create a new document (Ctrl+N)");
                    ui.button_keyed("open", "Open…");
                    ui.tooltip("Open a document from disk");
                    if ui.button_keyed("share", "Share ▾").clicked() {
                        share = !share;
                    }
                    ui.tooltip("Share a link to this document");
                    ui.popover("share-popover", &mut share, PopoverOptions { width: Length::Px(340.0), ..PopoverOptions::default() }, |ui| {
                        ui.label_styled("title", "Share link", Style::default().font_weight(700));
                        ui.text_input("link", "Link", &mut link);
                        ui.row("actions", Style::default().gap(8.0), |ui| {
                            ui.button_keyed("copy", "Copy");
                            ui.button_keyed("email", "Email…");
                        });
                    });
                    ui.label_styled("status", format!("Context menu: {last_menu}  ·  closed tabs: {closed}"), muted.clone());
                });
                ui.label_styled("hint", "Hover the toolbar for tooltips. Right-click the editor (or press Shift+F10 while it has focus) for a context menu. Drag the dividers or focus one and use the arrow keys.", muted.clone());
                let response = ui.tabs("documents", &mut selected, &tabs, |ui, index| {
                    ui.split("workspace", &mut sidebar, Axis::Horizontal, SplitOptions { min_first: 140.0, min_second: 240.0, style: Style::default().width(Length::Percent(1.0)).height(Length::Px(460.0)), ..SplitOptions::default() }, |ui| {
                        ui.column("files", Style::default().padding(8.0).gap(4.0).width(Length::Percent(1.0)), |ui| {
                            ui.label_styled("heading", "Files", Style::default().font_weight(700));
                            for name in ["README.md", "notes.md", "main.rs", "Cargo.toml"] {
                                ui.button_styled(name, name, true, Style::button().width(Length::Percent(1.0)).padding(8.0).background(rust_gui::Color::TRANSPARENT));
                            }
                        });
                    }, |ui| {
                        ui.split("editor", &mut editor_split, Axis::Vertical, SplitOptions { min_first: 120.0, min_second: 80.0, ..SplitOptions::default() }, |ui| {
                            let items = [MenuItem::new("Copy"), MenuItem::new("Paste"), MenuItem::disabled("Cut"), MenuItem::new("Delete")];
                            if let Some(choice) = ui.context_menu("editor-menu", &items, Style::default().width(Length::Percent(1.0)).height(Length::Percent(1.0)).padding(12.0).background(theme.surface), |ui| {
                                ui.label_styled("body", format!("Editor for tab {index}: right-click anywhere in this area."), Style::default().wrap(TextWrap::Word).width(Length::Percent(1.0)));
                                ui.button_keyed("focusable", "A focusable control inside the area");
                            }) {
                                last_menu = items[choice].label.clone();
                            }
                        }, |ui| {
                            ui.column("output", Style::default().padding(12.0).width(Length::Percent(1.0)), |ui| {
                                ui.label_styled("out", "Output panel · drag the divider above", Style::default().font_size(13.0).foreground(theme.text_muted));
                            });
                        });
                    });
                });
                if let Some(index) = response.closed
                    && tabs.len() > 1
                {
                    tabs.remove(index);
                    closed += 1;
                    if selected >= tabs.len() {
                        selected = tabs.len() - 1;
                    }
                }
            });
        },
    )
}
