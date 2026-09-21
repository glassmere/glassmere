use rust_gui::{
    AppOptions, Color, Command, DesktopHandle, DragPayload, DropKind, GraphicsOptions, GridColumn,
    GridModel, GridState, ItemId, ItemOrder, Key, Length, ListModel, ListState, Shortcut,
    ShortcutKey, Size, SortDirection, Style, TextWrap, TreeItem, TreeModel, Ui, VirtualListOptions,
    WindowKey, run_windows,
};
use std::{
    sync::{Arc, Mutex, mpsc::TrySendError},
    thread,
    time::Duration,
};

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
    // ponytail: full re-sort on every change; switch to incremental insertion if models grow past ~10^5 rows.
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

struct State {
    tickets: Tickets,
    list: ListState,
    grid: GridState,
    tree: TreeModel,
    tree_state: ListState,
    archived: Vec<ItemId>,
    files: Vec<String>,
    saves: u32,
    background: u32,
    details: Option<WindowKey>,
}

type Shared = Arc<Mutex<State>>;

enum Message {
    TicketCreated,
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
    let shared: Shared = Arc::new(Mutex::new(State {
        tickets: Tickets::new(50_000),
        list: ListState::default(),
        grid: GridState::default(),
        tree,
        tree_state: ListState::default(),
        archived: Vec::new(),
        files: Vec::new(),
        saves: 0,
        background: 0,
        details: None,
    }));
    run_windows(GraphicsOptions::default(), move |desktop| {
        let (sender, mailbox) = desktop.wakeup().channel::<Message>(64);
        thread::spawn(move || {
            for _ in 0..if smoke { 3 } else { u32::MAX } {
                thread::sleep(Duration::from_millis(if smoke { 100 } else { 2000 }));
                // A full mailbox drops this tick; a closed one ends the worker.
                if let Err(TrySendError::Disconnected(_)) = sender.try_send(Message::TicketCreated)
                {
                    break;
                }
            }
        });
        if smoke {
            open_details(&desktop, &shared, &mut shared.lock().unwrap(), smoke);
        }
        let main = shared.clone();
        let handle = desktop.clone();
        desktop
            .open_window(
                AppOptions {
                    title: "Rust GUI / Application workbench".into(),
                    size: Size::new(1080.0, 840.0),
                    frame_limit: smoke.then_some(4),
                    ..AppOptions::default()
                },
                move |ui| {
                    let mut state = main.lock().unwrap();
                    for Message::TicketCreated in mailbox.drain(64) {
                        state.tickets.append();
                        state.background += 1;
                    }
                    main_window(ui, &handle, &main, &mut state, smoke);
                },
            )
            .expect("the desktop is open during setup");
    })?;
    if smoke {
        println!("APPLICATION_SMOKE_OK");
    }
    Ok(())
}

fn main_window(
    ui: &mut Ui<'_>,
    desktop: &DesktopHandle,
    shared: &Shared,
    state: &mut State,
    smoke: bool,
) {
    let muted = Style {
        font_size: Some(13.0),
        foreground: Some(Color::rgb(162, 180, 198)),
        ..Style::default()
    };
    let heading = Style {
        font_weight: Some(700),
        ..Style::default()
    };
    ui.scroll("page", Style { width: Length::Percent(1.0), height: Length::Percent(1.0), padding: 24.0, gap: 14.0, ..Style::default() }, |ui| {
        ui.label_styled("title", "Application workbench", Style { font_size: Some(28.0), font_weight: Some(700), ..Style::default() });
        ui.label_styled("intro", "Virtualized list, tree and grid over 50,000 application-owned rows, commands with shortcuts, drag and drop, a worker mailbox and a second window.", Style {
            width: Length::Percent(1.0), wrap: TextWrap::Word, ..muted.clone()
        });
        ui.row("commands", Style { gap: 8.0, ..Style::default() }, |ui| {
            if ui.command_button("save", &Command::new("Save   Ctrl+S", Some(Shortcut::primary('s')))).clicked() {
                state.saves += 1;
            }
            if ui.command_button("new", &Command::new("New ticket   Ctrl+N", Some(Shortcut::primary('n')))).clicked() {
                state.tickets.append();
            }
            if ui.command_button("details", &Command::new("Details window   Ctrl+D", Some(Shortcut::primary('d')))).clicked() {
                open_details(desktop, shared, state, smoke);
            }
            let archive = Command {
                enabled: state.list.selected().is_some(),
                ..Command::new("Archive selected   Delete", Some(Shortcut { key: ShortcutKey::Named(Key::Delete), control: false, alt: false, shift: false, super_key: false }))
            };
            if ui.command_button("archive", &archive).clicked()
                && let Some(id) = state.list.selected()
            {
                state.archived.push(id);
            }
        });
        ui.label_keyed("status", format!("Saved {} times  ·  {} tickets created by the worker  ·  {} archived", state.saves, state.background, state.archived.len()));
        ui.row("panels", Style { gap: 24.0, width: Length::Percent(1.0), ..Style::default() }, |ui| {
            ui.column("left", Style { width: Length::Px(480.0), gap: 10.0, ..Style::default() }, |ui| {
                ui.label_styled("list-heading", format!("Tickets ({})  ·  Enter opens details", state.tickets.len()), heading.clone());
                let response = ui.virtual_list("tickets", &mut state.tickets, &mut state.list, VirtualListOptions {
                    label: "Tickets".into(), height: 280.0, row_height: 30.0, ..VirtualListOptions::default()
                });
                if response.changed && let Some(key) = state.details {
                    desktop.redraw(key);
                }
                if response.activated.is_some() {
                    open_details(desktop, shared, state, smoke);
                }
                ui.label_styled("tree-heading", "Areas  ·  arrows expand and collapse", heading.clone());
                ui.tree_view("areas", &mut state.tree, &mut state.tree_state, VirtualListOptions {
                    label: "Areas".into(), height: 180.0, row_height: 30.0, ..VirtualListOptions::default()
                });
            });
            ui.column("right", Style { gap: 10.0, ..Style::default() }, |ui| {
                ui.label_styled("grid-heading", "Grid  ·  click a header to sort", heading.clone());
                ui.data_grid("grid", &mut state.tickets, &mut state.grid, VirtualListOptions {
                    label: "Ticket grid".into(), height: 300.0, row_height: 30.0,
                    style: Style { width: Length::Px(380.0), ..Style::default() }, ..VirtualListOptions::default()
                });
                ui.label_styled("dnd-heading", "Drag and drop", heading.clone());
                ui.row("dnd", Style { gap: 12.0, ..Style::default() }, |ui| {
                    let selected = state.list.selected();
                    let panel = Style { padding: 12.0, radius: 8.0, width: Length::Px(180.0), height: Length::Px(72.0), ..Style::default() };
                    ui.enabled(selected.is_some(), |ui| {
                        ui.drag_source("source", "Selected ticket", DragPayload::Item { domain: "ticket".into(), id: selected.unwrap_or(ItemId(0)) }, Style { background: Some(Color::rgb(22, 105, 99)), ..panel.clone() }, |ui| {
                            ui.label_keyed("text", selected.map_or("Select a ticket to drag".into(), |id| format!("Drag ticket #{}", id.0)));
                        });
                    });
                    let dropped = ui.drop_target("archive", "Archive", DropKind::Item, Style { background: Some(Color::rgb(64, 48, 96)), ..panel.clone() }, |ui| {
                        ui.label_keyed("text", format!("Archive: {} tickets", state.archived.len()));
                    });
                    if let Some(DragPayload::Item { id, .. }) = dropped {
                        state.archived.push(id);
                    }
                    let files = ui.drop_target("files", "Drop files", DropKind::Files, Style { background: Some(Color::rgb(35, 52, 74)), ..panel.clone() }, |ui| {
                        ui.label_keyed("text", if state.files.is_empty() { "Drop files from the OS here".into() } else { state.files.join(", ") });
                    });
                    if let Some(DragPayload::Files(paths)) = files {
                        state.files = paths.iter().map(|path| path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())).collect();
                    }
                });
            });
        });
        ui.label_styled("footer", "Space picks up the focused source, Enter drops on the focused target, Escape cancels. Both windows share one GPU device, font system and clipboard.", Style {
            width: Length::Percent(1.0), wrap: TextWrap::Word, ..muted
        });
    });
}

fn open_details(desktop: &DesktopHandle, shared: &Shared, state: &mut State, smoke: bool) {
    if state.details.is_some() {
        return;
    }
    let shared = shared.clone();
    state.details = desktop
        .open_window(
            AppOptions {
                title: "Rust GUI / Ticket details".into(),
                size: Size::new(440.0, 280.0),
                frame_limit: smoke.then_some(3),
                ..AppOptions::default()
            },
            move |ui| {
                let state = shared.lock().unwrap();
                ui.column("page", Style { padding: 24.0, gap: 12.0, ..Style::default() }, |ui| {
                    ui.label_styled("title", "Ticket details", Style { font_size: Some(22.0), font_weight: Some(700), ..Style::default() });
                    match state.list.selected() {
                        Some(id) => {
                            ui.label_keyed("ticket", format!("Ticket #{}", id.0));
                            ui.label_keyed("priority", format!("Priority {}", Tickets::priority(id)));
                            ui.label_keyed("status", format!("Status: {}", Tickets::status(id)));
                        }
                        None => ui.label_keyed("ticket", "No ticket selected in the main window."),
                    }
                    ui.label_styled("hint", "Selection changes in the main window redraw this one. Close it from the title bar.", Style {
                        font_size: Some(13.0), foreground: Some(Color::rgb(162, 180, 198)), width: Length::Px(380.0), wrap: TextWrap::Word, ..Style::default()
                    });
                });
            },
        )
        .ok();
}
