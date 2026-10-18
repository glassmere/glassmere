use glassmere::{
    App, AppOptions, Color, Command, DragPayload, DragSourceBuilder, DropKind, DropTargetBuilder,
    Effects, GridColumn, GridModel, GridMsg, GridState, ItemId, ItemOrder, Key, Label, Length,
    ListModel, ListState, Shortcut, ShortcutKey, Size, SortDirection, Style, TextWrap, TreeItem,
    TreeModel, Ui, VirtualListOptions, WindowId, WindowRequest, run,
};
use std::{sync::mpsc::TrySendError, thread, time::Duration};

/// Application-owned rows. Identity is the ticket number, not the row position,
/// so selection survives sorting and background inserts.
struct Tickets {
    count: u64,
    sort: Option<(usize, SortDirection)>,
    order: ItemOrder,
    columns: Vec<GridColumn>,
}

impl Tickets {
    fn new(count: u64) -> Self {
        let column = |label: &str, width, sortable| GridColumn {
            label: label.into(),
            width,
            sortable,
            editor: None,
            filterable: false,
        };
        let mut tickets = Self {
            count,
            sort: None,
            order: ItemOrder::default(),
            columns: vec![
                column("Ticket", 110.0, true),
                column("Priority", 110.0, true),
                column("Status", 160.0, false),
            ],
        };
        tickets.rebuild();
        tickets
    }
    fn priority(id: ItemId) -> u64 {
        id.0 * 7919 % 5
    }
    fn status(id: ItemId) -> &'static str {
        ["open", "triaged", "in progress"][(id.0 % 3) as usize]
    }
    // Full re-sort on every change; switch to incremental insertion if models grow past ~10^5 rows.
    fn rebuild(&mut self) {
        let mut ids: Vec<_> = (0..self.count).map(ItemId).collect();
        if let Some((column, direction)) = self.sort {
            if column == 1 {
                ids.sort_by_key(|id| (Self::priority(*id), id.0));
            }
            if direction == SortDirection::Descending {
                ids.reverse();
            }
        }
        self.order = ItemOrder::new(ids).expect("ticket numbers are unique");
    }
    fn append(&mut self) {
        self.count += 1;
        self.rebuild();
    }
}

impl ListModel for Tickets {
    fn len(&self) -> usize {
        self.order.ids().len()
    }
    fn item_id(&self, index: usize) -> ItemId {
        self.order.ids()[index]
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        self.order.index_of(id)
    }
    fn label(&self, index: usize) -> String {
        let id = self.item_id(index);
        format!(
            "Ticket #{}  ·  priority {}  ·  {}",
            id.0,
            Self::priority(id),
            Self::status(id)
        )
    }
}

impl GridModel for Tickets {
    fn columns(&self) -> &[GridColumn] {
        &self.columns
    }
    fn cell(&self, row: usize, column: usize) -> String {
        let id = self.item_id(row);
        match column {
            0 => format!("#{}", id.0),
            1 => Self::priority(id).to_string(),
            _ => Self::status(id).into(),
        }
    }
    fn sort(&mut self, column: usize, direction: SortDirection) -> bool {
        self.sort = Some((column, direction));
        self.rebuild();
        true
    }
}

#[derive(Clone)]
enum Message {
    Save,
    NewTicket,
    OpenDetails,
    /// The Delete command: archives the selected ticket.
    Archive,
    /// The worker's tick: a ticket was created in the background.
    TicketCreated,
    /// The ticket list's selection.
    List(GridMsg),
    /// The ticket grid's sorting, filtering and selection.
    Grid(GridMsg),
    /// The area tree's selection and disclosure.
    Tree(GridMsg),
    /// A ticket dropped on the archive target.
    Archived(DragPayload),
    /// Files dropped on the file target.
    Files(DragPayload),
    /// A ticket dropped on the details window's link target.
    Linked(DragPayload),
}

struct Application {
    smoke: bool,
    tickets: Tickets,
    list: ListState,
    grid: GridState,
    tree: TreeModel,
    tree_state: ListState,
    archived: Vec<ItemId>,
    /// Tickets dragged from the main window into the details window.
    linked: Vec<ItemId>,
    files: Vec<String>,
    saves: u32,
    background: u32,
    details: Option<WindowId>,
}

impl Application {
    fn open_details(&mut self, effects: &mut Effects<'_, Message>) {
        if self.details.is_some() {
            return;
        }
        self.details = Some(effects.open_window(WindowRequest {
            title: "Glassmere / Ticket details".into(),
            size: Some(Size::new(440.0, 460.0)),
            modal_owner: None,
        }));
    }
}

impl App for Application {
    type Message = Message;

    fn init(&mut self, effects: &mut Effects<'_, Message>) {
        let sender = effects.channel::<()>(64, |()| Message::TicketCreated);
        let smoke = self.smoke;
        thread::spawn(move || {
            for _ in 0..if smoke { 3 } else { u32::MAX } {
                thread::sleep(Duration::from_millis(if smoke { 100 } else { 2000 }));
                // A full mailbox drops this tick; a closed one ends the worker.
                if let Err(TrySendError::Disconnected(_)) = sender.try_send(()) {
                    break;
                }
            }
        });
        if self.smoke {
            self.open_details(effects);
        }
    }

    fn update(&mut self, message: Message, effects: &mut Effects<'_, Message>) {
        match message {
            Message::Save => self.saves += 1,
            Message::NewTicket => self.tickets.append(),
            Message::OpenDetails => self.open_details(effects),
            Message::Archive => {
                if let Some(id) = self.list.selected() {
                    self.archived.push(id);
                }
            }
            Message::TicketCreated => {
                self.tickets.append();
                self.background += 1;
            }
            Message::List(message) => match message {
                GridMsg::Select(id) => self.list.select(Some(id)),
                GridMsg::Deselect => self.list.select(None),
                GridMsg::Activate(_) => self.open_details(effects),
                _ => {}
            },
            Message::Grid(message) => {
                self.grid.apply(&message);
                match message {
                    GridMsg::Sort(column, direction) => {
                        self.tickets.sort(column, direction);
                    }
                    GridMsg::FiltersApplied => {
                        let filters: Vec<(usize, &str)> = self
                            .grid
                            .applied_filters()
                            .iter()
                            .map(|(column, text)| (*column, text.as_str()))
                            .collect();
                        self.tickets.filter(&filters);
                    }
                    _ => {}
                }
            }
            Message::Tree(message) => match message {
                GridMsg::Select(id) => self.tree_state.select(Some(id)),
                GridMsg::Deselect => self.tree_state.select(None),
                GridMsg::Expand { row, open } => {
                    self.tree.set_expanded(row, open);
                }
                _ => {}
            },
            Message::Archived(payload) => {
                if let DragPayload::Item { id, .. } = payload {
                    self.archived.push(id);
                }
            }
            Message::Files(payload) => {
                if let DragPayload::Files(paths) = payload {
                    self.files = paths
                        .iter()
                        .map(|path| {
                            path.file_name().map_or_else(
                                || path.display().to_string(),
                                |name| name.to_string_lossy().into_owned(),
                            )
                        })
                        .collect();
                }
            }
            Message::Linked(payload) => {
                if let DragPayload::Item { id, .. } = payload {
                    self.linked.push(id);
                }
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Message>) {
        let muted = Style {
            font_size: Some(13.0),
            foreground: Some(Color::rgb(162, 180, 198)),
            ..Style::default()
        };
        let heading = Style {
            font_weight: Some(700),
            ..Style::default()
        };
        let page = Style {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            padding: 24.0,
            gap: 14.0,
            ..Style::default()
        };
        let wrapped = Style {
            width: Length::Percent(1.0),
            wrap: TextWrap::Word,
            ..muted.clone()
        };
        let delete = Shortcut {
            key: ShortcutKey::Named(Key::Delete),
            control: false,
            alt: false,
            shift: false,
            super_key: false,
        };
        let panel = Style {
            padding: 12.0,
            radius: 8.0,
            width: Length::Px(180.0),
            height: Length::Px(72.0),
            ..Style::default()
        };
        let list_options = VirtualListOptions {
            label: "Tickets".into(),
            height: 280.0,
            row_height: 30.0,
            ..VirtualListOptions::default()
        };
        let tree_options = VirtualListOptions {
            label: "Areas".into(),
            height: 180.0,
            row_height: 30.0,
            ..VirtualListOptions::default()
        };
        let grid_options = VirtualListOptions {
            label: "Ticket grid".into(),
            height: 300.0,
            row_height: 30.0,
            style: Style {
                width: Length::Px(380.0),
                ..Style::default()
            },
            ..VirtualListOptions::default()
        };
        ui.scroll(page, |ui| {
            ui.label(Label::new("Application workbench").style(Style {
                font_size: Some(28.0),
                font_weight: Some(700),
                ..Style::default()
            }));
            ui.label(
                Label::new(
                    "Virtualized list, tree and grid over 50,000 application-owned rows, commands \
                     with shortcuts, drag and drop, a worker mailbox and a second window.",
                )
                .style(wrapped.clone()),
            );
            ui.row(Style::default().gap(8.0), |ui| {
                let save = Command::new("Save   Ctrl+S", Some(Shortcut::primary('s')));
                let new = Command::new("New ticket   Ctrl+N", Some(Shortcut::primary('n')));
                let details = Command::new("Details window   Ctrl+D", Some(Shortcut::primary('d')));
                let archive = Command {
                    enabled: self.list.selected().is_some(),
                    ..Command::new("Archive selected   Delete", Some(delete))
                };
                ui.command_button(&save, Message::Save);
                ui.command_button(&new, Message::NewTicket);
                ui.command_button(&details, Message::OpenDetails);
                ui.command_button(&archive, Message::Archive);
            });
            ui.label(format!(
                "Saved {} times  ·  {} tickets created by the worker  ·  {} archived",
                self.saves,
                self.background,
                self.archived.len()
            ));
            ui.row(
                Style::default().gap(24.0).width(Length::Percent(1.0)),
                |ui| {
                    ui.column(Style::default().width(Length::Px(480.0)).gap(10.0), |ui| {
                        ui.label(
                            Label::new(format!(
                                "Tickets ({})  ·  Enter opens details",
                                self.tickets.len()
                            ))
                            .style(heading.clone()),
                        );
                        ui.virtual_list(&self.tickets, &self.list, list_options, Message::List);
                        ui.label(
                            Label::new("Areas  ·  arrows expand and collapse")
                                .style(heading.clone()),
                        );
                        ui.tree_view(&self.tree, &self.tree_state, tree_options, Message::Tree);
                    });
                    ui.column(Style::default().gap(10.0), |ui| {
                        ui.label(
                            Label::new("Grid  ·  click a header to sort").style(heading.clone()),
                        );
                        ui.data_grid(&self.tickets, &self.grid, grid_options, Message::Grid);
                        ui.label(Label::new("Drag and drop").style(heading.clone()));
                        ui.row(Style::default().gap(12.0), |ui| {
                            let selected = self.list.selected();
                            let source = DragSourceBuilder::new(
                                "Selected ticket",
                                DragPayload::Item {
                                    domain: "ticket".into(),
                                    id: selected.unwrap_or(ItemId(0)),
                                },
                            );
                            ui.enabled(selected.is_some(), |ui| {
                                let style = Style {
                                    background: Some(Color::rgb(22, 105, 99)),
                                    ..panel.clone()
                                };
                                ui.drag_source(style, source, |ui| {
                                    ui.label(
                                        selected.map_or("Select a ticket to drag".into(), |id| {
                                            format!("Drag ticket #{}", id.0)
                                        }),
                                    );
                                });
                            });
                            let archive = DropTargetBuilder::new("Archive", DropKind::Item)
                                .on_drop(Message::Archived);
                            let style = Style {
                                background: Some(Color::rgb(64, 48, 96)),
                                ..panel.clone()
                            };
                            ui.drop_target(style, archive, |ui| {
                                ui.label(format!("Archive: {} tickets", self.archived.len()));
                            });
                            let files = DropTargetBuilder::new("Drop files", DropKind::Files)
                                .on_drop(Message::Files);
                            let style = Style {
                                background: Some(Color::rgb(35, 52, 74)),
                                ..panel.clone()
                            };
                            ui.drop_target(style, files, |ui| {
                                ui.label(if self.files.is_empty() {
                                    "Drop files from the OS here".into()
                                } else {
                                    self.files.join(", ")
                                });
                            });
                        });
                    });
                },
            );
            ui.label(
                Label::new(
                    "Space picks up the focused source, Enter drops on the focused target, Escape \
                     cancels. Both windows share one GPU device, font system and clipboard.",
                )
                .style(wrapped),
            );
        });
    }

    fn view_window(&self, _window: WindowId, ui: &mut Ui<'_, Message>) {
        // The only window besides the main one is the ticket details.
        ui.column(Style::default().padding(24.0).gap(12.0), |ui| {
            ui.label(Label::new("Ticket details").style(Style {
                font_size: Some(22.0),
                font_weight: Some(700),
                ..Style::default()
            }));
            match self.list.selected() {
                Some(id) => {
                    ui.label(format!("Ticket #{}", id.0));
                    ui.label(format!("Priority {}", Tickets::priority(id)));
                    ui.label(format!("Status: {}", Tickets::status(id)));
                }
                None => ui.label("No ticket selected in the main window."),
            }
            // A drag that starts in the main window and ends here: the desktop
            // carries the payload across.
            let linked = if self.linked.is_empty() {
                "Drag a ticket here to link it".to_owned()
            } else {
                format!(
                    "Linked: {}",
                    self.linked
                        .iter()
                        .map(|id| format!("#{}", id.0))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            let target = Style {
                padding: 12.0,
                radius: 8.0,
                width: Length::Px(380.0),
                height: Length::Px(64.0),
                background: Some(Color::rgb(64, 48, 96)),
                ..Style::default()
            };
            let link =
                DropTargetBuilder::new("Link ticket", DropKind::Item).on_drop(Message::Linked);
            ui.drop_target(target, link, |ui| {
                ui.label(linked);
            });
            ui.label(
                Label::new(
                    "Selection changes in the main window redraw this one. Close it from the \
                     title bar.",
                )
                .style(Style {
                    font_size: Some(13.0),
                    foreground: Some(Color::rgb(162, 180, 198)),
                    width: Length::Px(380.0),
                    wrap: TextWrap::Word,
                    ..Style::default()
                }),
            );
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let node = |id, parent: Option<u64>, label: &str| TreeItem {
        id: ItemId(id),
        parent: parent.map(ItemId),
        label: label.into(),
    };
    let tree = TreeModel::new(vec![
        node(1, None, "Backlog"),
        node(2, Some(1), "Bugs"),
        node(3, Some(2), "Crash on resize"),
        node(4, Some(2), "Slow sort"),
        node(5, Some(1), "Features"),
        node(6, Some(5), "Dark theme"),
        node(7, None, "Done"),
        node(8, Some(7), "Virtual lists"),
    ])?;
    run(
        AppOptions {
            title: "Glassmere / Application workbench".into(),
            size: Size::new(1080.0, 840.0),
            frame_limit: smoke.then_some(4),
            ..AppOptions::default()
        },
        Application {
            smoke,
            tickets: Tickets::new(50_000),
            list: ListState::default(),
            grid: GridState::default(),
            tree,
            tree_state: ListState::default(),
            archived: Vec::new(),
            linked: Vec::new(),
            files: Vec::new(),
            saves: 0,
            background: 0,
            details: None,
        },
    )?;
    if smoke {
        println!("APPLICATION_SMOKE_OK");
    }
    Ok(())
}
