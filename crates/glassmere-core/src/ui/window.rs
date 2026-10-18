use super::*;

impl<'a, M: 'static> Ui<'a, M> {
    /// The window's native menu bar. Declaring the same menu again changes
    /// nothing, so a view may set it every frame. Choosing an item sends
    /// `Event::Command` with that command's id, which activates the command
    /// the view declared, exactly as its button would.
    #[track_caller]
    pub fn set_menu(&mut self, menu: crate::Menu) {
        self.host.menu = Some(menu);
    }

    /// What the host says about the window: whether it is maximized and
    /// whether it has the keyboard. Of the last frame, like everything a
    /// declaration is given.
    #[track_caller]
    pub fn window(&self) -> crate::WindowInfo {
        self.context.window_info()
    }

    /// The window's logical size, which is what `Ui::resize_frame` places
    /// its grips by.
    #[track_caller]
    pub fn window_size(&self) -> glassmere_scene::Size {
        self.context.size()
    }

    /// A strip that moves the window, for a window with the system's title
    /// bar turned off. Pressing it starts a move; pressing it twice
    /// maximizes or restores. Whatever is declared inside it keeps working,
    /// because a press that lands on a control there is that control's.
    ///
    /// ```ignore
    /// ui.title_bar(Style::default().padding(6.0), |ui| {
    ///     ui.label("My application");
    ///     ui.window_buttons(Msg::Window);
    /// });
    /// ```
    #[track_caller]
    pub fn title_bar(&mut self, style: Style, content: impl FnOnce(&mut Ui<'_, M>)) {
        // A row, so that a title, a menu and the buttons sit side by side
        // the way a title bar's do.
        self.row(style, content);
        self.set_control(Control::TitleBar);
    }

    /// Minimize, maximize or restore, and close, as three buttons. The
    /// middle one says which it is from `Ui::window`. A click sends
    /// `on_command(command)`; `Effects::window_command` in `update` carries
    /// it out.
    #[track_caller]
    pub fn window_buttons(&mut self, on_command: impl Fn(crate::WindowCommand) -> M) {
        let maximized = self.window().maximized;
        let foreground = self.theme().text;
        // A wash of the text colour, which is how a desktop marks a caption
        // button: light over a dark title bar, dark over a light one.
        let wash = |alpha: u8| Color(foreground.0, foreground.1, foreground.2, alpha);
        let (hover, pressed) = (wash(caption::WASH_HOVER), wash(caption::WASH_PRESSED));
        self.row(Style::default(), |ui| {
            for (key, glyph, name, command) in [
                (
                    "minimize",
                    caption::MINIMIZE,
                    "glassmere-minimize",
                    crate::WindowCommand::Minimize,
                ),
                (
                    "maximize",
                    if maximized {
                        caption::RESTORE
                    } else {
                        caption::MAXIMIZE
                    },
                    if maximized {
                        "glassmere-restore"
                    } else {
                        "glassmere-maximize"
                    },
                    crate::WindowCommand::ToggleMaximize,
                ),
                (
                    "close",
                    caption::CLOSE,
                    "glassmere-close",
                    crate::WindowCommand::Close,
                ),
            ] {
                // Closing is the one that is dangerous, and every desktop
                // says so the same way: red under the pointer, white on it.
                let closing = matches!(command, crate::WindowCommand::Close);
                let over = |background: Color| crate::Paint {
                    background: Some(background),
                    foreground: closing.then_some(Color::WHITE),
                    ..crate::Paint::default()
                };
                let style = Style {
                    width: crate::Length::Px(caption::WIDTH),
                    height: crate::Length::Px(caption::HEIGHT),
                    // The glyph sits in the middle of the box, which is the
                    // size the shell's own buttons are.
                    justify_content: crate::Justify::Center,
                    align_items: crate::Align::Center,
                    padding: 0.0,
                    radius: 0.0,
                    background: Some(Color(0, 0, 0, 0)),
                    foreground: Some(foreground),
                    font_family: caption::FONT
                        .map(|name| glassmere_scene::FontFamily::Named((*name).to_owned())),
                    font_size: Some(caption::SIZE),
                    ..Style::default()
                }
                .on_hover(over(if closing { caption::CLOSE_HOVER } else { hover }))
                .on_pressed(over(if closing {
                    caption::CLOSE_PRESSED
                } else {
                    pressed
                }));
                ui.button_at(key, glyph, true, style);
                ui.set_handler(crate::app::Handler::Message(on_command(command)));
                // The glyph is what is drawn; the name is what is announced.
                let label = ui.tr(name);
                ui.tooltip(label);
                if matches!(command, crate::WindowCommand::ToggleMaximize) {
                    // Windows asks where this one is, to offer its snap
                    // layouts when the pointer rests over it.
                    ui.set_control(Control::MaximizeButton);
                }
            }
        });
    }

    /// The eight edges and corners of a window that has no system border,
    /// as narrow strips over everything else. Declare it once, at the end of
    /// the view, so it lies above the content; a press on one starts a
    /// resize. `thickness` is in logical pixels, and six is comfortable.
    ///
    /// The grips are placed from the size the window had last frame, so a
    /// resize moves them a frame later, which nothing can see while the
    /// platform is drawing the drag.
    #[track_caller]
    pub fn resize_frame(&mut self, thickness: f32) {
        let size = self.window_size();
        let side = thickness.max(1.0);
        if size.width <= side * 2.0 || size.height <= side * 2.0 {
            return;
        }
        // Out of flow and over the whole window, so that the grips sit on
        // top of the content and displace none of it.
        let frame = Style {
            offset: Some((0.0, 0.0)),
            width: crate::Length::Px(size.width),
            height: crate::Length::Px(size.height),
            ..Style::default()
        };
        let key = self.next_key();
        self.group(key, Kind::Container, frame, |ui| {
            for edge in crate::ResizeEdge::ALL {
                use crate::ResizeEdge::{
                    East, North, NorthEast, NorthWest, South, SouthEast, SouthWest, West,
                };
                let (x, y, width, height) = match edge {
                    North => (side, 0.0, size.width - side * 2.0, side),
                    South => (side, size.height - side, size.width - side * 2.0, side),
                    West => (0.0, side, side, size.height - side * 2.0),
                    East => (size.width - side, side, side, size.height - side * 2.0),
                    NorthWest => (0.0, 0.0, side, side),
                    NorthEast => (size.width - side, 0.0, side, side),
                    SouthWest => (0.0, size.height - side, side, side),
                    SouthEast => (size.width - side, size.height - side, side, side),
                };
                ui.group(
                    WidgetKey::named(edge.name()),
                    Kind::Container,
                    Style {
                        offset: Some((x, y)),
                        width: crate::Length::Px(width.max(0.0)),
                        height: crate::Length::Px(height.max(0.0)),
                        ..Style::default()
                    },
                    |_| {},
                );
                ui.set_control(Control::ResizeGrip(edge));
            }
        });
    }

    /// Draws `menu` as a menu bar, for the platforms where a menu belongs
    /// to the window rather than to the application. Choosing an item
    /// activates that command, exactly as the platform's own menu does, so a
    /// view handles both the same way: through the command it declared.
    ///
    /// Two levels are shown. A submenu inside a submenu is drawn as a
    /// heading with its items under it, because the framework's own menus
    /// do not nest.
    #[track_caller]
    pub fn menu_bar(&mut self, menu: &crate::Menu) {
        let style = Style {
            gap: 4.0,
            ..Style::default()
        };
        let mut chosen = None;
        self.row(style, |ui| {
            for (index, entry) in menu.entries.iter().enumerate() {
                let crate::MenuEntry::Submenu { label, entries } = entry else {
                    continue;
                };
                let mut flat = Vec::new();
                crate::platform::flatten(entries, 0, &mut flat);
                let items: Vec<crate::MenuItem> = flat
                    .iter()
                    .map(|(depth, command)| match command {
                        Some(command) => crate::MenuItem {
                            label: format!("{}{}", "  ".repeat(*depth), command.label),
                            enabled: command.enabled,
                            hint: command
                                .shortcut
                                .as_ref()
                                .map(crate::Shortcut::describe)
                                .unwrap_or_default(),
                        },
                        None => crate::MenuItem {
                            label: "─".repeat(12),
                            enabled: false,
                            hint: String::new(),
                        },
                    })
                    .collect();
                if let Some(picked) = ui.menu_pick(
                    WidgetKey::named(format!("menu{index}")),
                    label.clone(),
                    &items,
                    Style {
                        padding: 8.0,
                        ..Style::button()
                    },
                    crate::Length::Px(260.0),
                ) && let Some((_, Some(command))) = flat.get(picked)
                {
                    chosen = Some(command.id.clone());
                }
            }
        });
        if let Some(id) = chosen {
            self.host.commands.push(id);
        }
    }

    /// The application's tray icon, or `None` to take it away.
    #[track_caller]
    pub fn set_tray(&mut self, tray: Option<crate::Tray>) {
        self.host.tray = Some(tray);
    }
}
