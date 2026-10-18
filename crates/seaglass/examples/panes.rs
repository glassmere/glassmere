use seaglass::{
    App, AppOptions, Axis, Button, Color, DirectionChoice, Effects, Label, Length, MenuItem,
    PopoverOptions, Size, SplitOptions, Style, Tab, TextWrap, Ui, run,
};

#[derive(Clone)]
enum Msg {
    Select(usize),
    Close(usize),
    Sidebar(f32),
    EditorSplit(f32),
    ToggleShare,
    Share(bool),
    Link(String),
    Menu(usize),
}

/// The editor's context menu, shared by the view and the `Menu` message.
fn editor_menu() -> [MenuItem; 4] {
    [
        MenuItem::new("Copy"),
        MenuItem::new("Paste"),
        MenuItem::disabled("Cut"),
        MenuItem::new("Delete"),
    ]
}

struct Panes {
    tabs: Vec<Tab>,
    selected: usize,
    sidebar: f32,
    editor_split: f32,
    share: bool,
    link: String,
    last_menu: String,
    closed: u32,
}

impl App for Panes {
    type Message = Msg;

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Select(index) => self.selected = index,
            Msg::Close(index) => {
                if self.tabs.len() > 1 {
                    self.tabs.remove(index);
                    self.closed += 1;
                    self.selected = self.selected.min(self.tabs.len() - 1);
                }
            }
            Msg::Sidebar(fraction) => self.sidebar = fraction,
            Msg::EditorSplit(fraction) => self.editor_split = fraction,
            Msg::ToggleShare => self.share = !self.share,
            Msg::Share(open) => self.share = open,
            Msg::Link(link) => self.link = link,
            Msg::Menu(choice) => self.last_menu = editor_menu()[choice].label.clone(),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        let theme = ui.theme().clone();
        let muted = Style::default()
            .font_size(13.0)
            .foreground(theme.text_muted)
            .wrap(TextWrap::Word)
            .width(Length::Percent(1.0));
        let page = Style::default()
            .width(Length::Percent(1.0))
            .height(Length::Percent(1.0))
            .padding(20.0)
            .gap(12.0);
        let share_popover = PopoverOptions {
            width: Length::Px(340.0),
            ..PopoverOptions::default()
        };
        let sidebar = SplitOptions {
            min_first: 140.0,
            min_second: 240.0,
            style: Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Px(460.0)),
            ..SplitOptions::default()
        };
        let editor_split = SplitOptions {
            min_first: 120.0,
            min_second: 80.0,
            ..SplitOptions::default()
        };
        let file_list = Style::default()
            .padding(8.0)
            .gap(4.0)
            .width(Length::Percent(1.0));
        let output_panel = Style::default().padding(12.0).width(Length::Percent(1.0));
        let wrapped = Style::default()
            .wrap(TextWrap::Word)
            .width(Length::Percent(1.0));
        let file_button = Style::button()
            .width(Length::Percent(1.0))
            .padding(8.0)
            .background(Color::TRANSPARENT);
        let editor_area = Style::default()
            .width(Length::Percent(1.0))
            .height(Length::Percent(1.0))
            .padding(12.0)
            .background(theme.surface);
        ui.column(page, |ui| {
            ui.row(Style::default().gap(8.0), |ui| {
                ui.button("New");
                ui.tooltip("Create a new document (Ctrl+N)");
                ui.button("Open…");
                ui.tooltip("Open a document from disk");
                ui.button(Button::new("Share ▾").on_click(Msg::ToggleShare));
                ui.tooltip("Share a link to this document");
                ui.popover(self.share, share_popover.clone(), Msg::Share, |ui| {
                    ui.label(Label::new("Share link").style(Style::default().font_weight(700)));
                    ui.text_input("Link", &self.link, Msg::Link);
                    ui.row(Style::default().gap(8.0), |ui| {
                        ui.button("Copy");
                        ui.button("Email…");
                    });
                });
                ui.label(
                    Label::new(format!(
                        "Context menu: {}  ·  closed tabs: {}",
                        self.last_menu, self.closed
                    ))
                    .style(muted.clone()),
                );
            });
            ui.label(
                Label::new(
                    "Hover the toolbar for tooltips. Right-click the editor (or press Shift+F10 \
                     while it has focus) for a context menu. Drag the dividers or focus one and \
                     use the arrow keys.",
                )
                .style(muted),
            );
            ui.tabs_close(
                self.selected,
                &self.tabs,
                Msg::Select,
                Msg::Close,
                |ui, index| {
                    ui.split(
                        self.sidebar,
                        Axis::Horizontal,
                        sidebar.clone(),
                        Msg::Sidebar,
                        |ui| {
                            ui.column(file_list.clone(), |ui| {
                                ui.label(
                                    Label::new("Files").style(Style::default().font_weight(700)),
                                );
                                for name in ["README.md", "notes.md", "main.rs", "Cargo.toml"] {
                                    ui.keyed(name, |ui| {
                                        ui.button(Button::new(name).style(file_button.clone()))
                                    });
                                }
                            });
                        },
                        |ui| {
                            ui.split(
                                self.editor_split,
                                Axis::Vertical,
                                editor_split.clone(),
                                Msg::EditorSplit,
                                |ui| {
                                    ui.context_menu(
                                        &editor_menu(),
                                        editor_area.clone(),
                                        Msg::Menu,
                                        |ui| {
                                            ui.label(
                                Label::new(format!(
                                    "Editor for tab {index}: right-click anywhere in this area."
                                ))
                                .style(wrapped.clone()),
                            );
                                            ui.button("A focusable control inside the area");
                                        },
                                    );
                                },
                                |ui| {
                                    ui.column(output_panel.clone(), |ui| {
                                        ui.label(
                                            Label::new("Output panel · drag the divider above")
                                                .style(
                                                    Style::default()
                                                        .font_size(13.0)
                                                        .foreground(theme.text_muted),
                                                ),
                                        );
                                    });
                                },
                            );
                        },
                    );
                },
            );
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    // `--rtl` mirrors the window as a right-to-left locale would.
    let direction = if std::env::args().any(|arg| arg == "--rtl") {
        DirectionChoice::Rtl
    } else {
        DirectionChoice::Auto
    };
    let open = std::env::args().any(|arg| arg == "--open");
    run(
        AppOptions {
            title: "Seaglass / Tabs, panes and overlays".into(),
            size: Size::new(960.0, 720.0),
            frame_limit: smoke.then_some(3),
            direction,
            ..AppOptions::default()
        },
        Panes {
            tabs: vec![
                Tab::new("Overview"),
                Tab::closable("notes.md"),
                Tab::closable("main.rs"),
            ],
            selected: 0,
            sidebar: 0.28,
            editor_split: 0.65,
            share: open,
            link: String::from("https://example.test/doc/42"),
            last_menu: String::from("none"),
            closed: 0,
        },
    )
}
