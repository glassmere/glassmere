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
    /// maximizes or restores. A [`Ui::menu_bar`] in it is drawn flat, as part
    /// of the bar. Unless the style gives a height, it is as tall as the
    /// system's caption. Whatever is declared inside it keeps working,
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
        let mut style = style;
        if style.height == crate::Length::Auto {
            style.height = crate::Length::Px(caption::HEIGHT);
        }
        // A row, so that a title, a menu and the buttons sit side by side
        // the way a title bar's do.
        let outer = std::mem::replace(&mut self.host.in_title_bar, true);
        self.row(style, content);
        self.host.in_title_bar = outer;
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
        // Side by side with nothing between them, as the system's are.
        self.row(Style::default().gap(0.0), |ui| {
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
                // The glyph is what is drawn; the name is what is announced,
                // and what the tooltip says where the system does not show
                // its own.
                let label = ui.tr(name);
                ui.tooltip(label);
                // Windows asks where these are, to show its own tooltips over
                // them and offer its snap layouts over the maximize button.
                ui.set_control(match command {
                    crate::WindowCommand::Minimize => Control::MinimizeButton,
                    crate::WindowCommand::Close => Control::CloseButton,
                    _ => Control::MaximizeButton,
                });
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
    /// A submenu inside a menu is a row that opens it beside the menu, to
    /// any depth: on hover, on a click, on Enter and on the arrow key that
    /// points the reading way. The other arrow key and Escape go back out.
    #[track_caller]
    pub fn menu_bar(&mut self, menu: &crate::Menu) {
        let (style, heading) = if self.host.in_title_bar {
            // In a title bar it is part of that bar, its headings flat and
            // no taller than it.
            let foreground = self.theme().text;
            // A wash of the text colour, as the caption buttons use but
            // fainter, for a heading is no button: light over a dark bar,
            // dark over a light one.
            let wash = |alpha: u8| Color(foreground.0, foreground.1, foreground.2, alpha);
            let heading = Style {
                padding: 5.0,
                radius: 4.0,
                background: Some(Color(0, 0, 0, 0)),
                ..Style::default()
            }
            .on_hover(crate::Paint {
                background: Some(wash(8)),
                ..crate::Paint::default()
            })
            // A heading is pressed while its dropdown is open.
            .on_pressed(crate::Paint {
                background: Some(wash(14)),
                ..crate::Paint::default()
            });
            let style = Style {
                gap: 2.0,
                align_items: crate::Align::Center,
                ..Style::default()
            };
            (style, heading)
        } else {
            let style = Style {
                gap: 4.0,
                ..Style::default()
            };
            let heading = Style {
                padding: 8.0,
                ..Style::button()
            };
            (style, heading)
        };
        let mut chosen = None;
        self.row(style, |ui| {
            for (index, entry) in menu.entries.iter().enumerate() {
                let crate::MenuEntry::Submenu { label, entries } = entry else {
                    continue;
                };
                if let Some(command) = ui.menu_tree(
                    WidgetKey::named(format!("menu{index}")),
                    label.clone(),
                    entries,
                    heading.clone(),
                    crate::Length::Px(260.0),
                ) {
                    chosen = Some(command);
                }
            }
        });
        if let Some(id) = chosen {
            self.host.commands.push(id);
        }
    }

    /// A menu button over `entries`, whose submenus open beside the menu
    /// they are in. Returns the id of the command picked since the last
    /// declaration.
    #[track_caller]
    fn menu_tree(
        &mut self,
        key: WidgetKey,
        text: String,
        entries: &[crate::MenuEntry],
        style: Style,
        popup_width: crate::Length,
    ) -> Option<String> {
        let path = match self.take_update(&key, Kind::MenuButton) {
            Some(ControlUpdate::Choice(index)) => vec![index],
            Some(ControlUpdate::Path(path)) => path,
            _ => Vec::new(),
        };
        let chosen = crate::platform::command_at(entries, &path).map(|command| command.id.clone());
        let items = crate::platform::level_items(entries);
        let owner = self.context.child(self.parent, &key, Kind::MenuButton);
        let children = self.menu_level(owner, entries, &items, popup_width);
        self.push(Element {
            text,
            style,
            enabled: self.enabled && items.iter().any(|item| item.enabled),
            children,
            control: Control::Menu { items },
            ..Element::new(key, Kind::MenuButton)
        });
        chosen
    }

    /// The popup of one level of a menu while `owner` has it open, with the
    /// popups of the submenus open under it.
    #[track_caller]
    fn menu_level(
        &mut self,
        owner: Option<NodeId>,
        entries: &[crate::MenuEntry],
        items: &[crate::MenuItem],
        width: crate::Length,
    ) -> Vec<Element> {
        let Some(popup) = owner
            .and_then(|owner| self.context.popup_of(owner))
            .filter(|_| self.enabled)
        else {
            return Vec::new();
        };
        let selection = self.context.theme().selection;
        let mut child = self.scope(owner);
        child.group(
            WidgetKey::named("$popup"),
            Kind::Popup,
            Style {
                width,
                height: crate::Length::Px((items.len() as f32 * 34.0 + 8.0).min(280.0)),
                gap: 2.0,
                padding: 4.0,
                ..Style::default()
            },
            |ui| {
                for (index, (entry, item)) in entries.iter().zip(items).enumerate() {
                    let key = WidgetKey::Named(index.to_string());
                    let style = Style {
                        width: crate::Length::Percent(1.0),
                        height: crate::Length::Px(32.0),
                        padding: 5.0,
                        background: (index == popup.highlighted).then_some(selection),
                        ..Style::default()
                    };
                    let (children, control) = match entry {
                        crate::MenuEntry::Submenu { entries, .. } => {
                            let row = ui.context.child(ui.parent, &key, Kind::Choice);
                            let inner = crate::platform::level_items(entries);
                            let children = ui.menu_level(row, entries, &inner, width);
                            let control = Control::Submenu {
                                items: inner,
                                hint: "\u{25b8}".into(),
                            };
                            (children, control)
                        }
                        _ => (
                            Vec::new(),
                            Control::Choice {
                                hint: item.hint.clone(),
                                index,
                                selected: false,
                                menu: true,
                            },
                        ),
                    };
                    ui.push(Element {
                        text: item.label.clone(),
                        style,
                        enabled: item.enabled,
                        children,
                        control,
                        ..Element::new(key, Kind::Choice)
                    });
                }
            },
        );
        child.elements[0].control = Control::Popup { menu: true };
        child.elements
    }

    /// The application's tray icon, or `None` to take it away.
    #[track_caller]
    pub fn set_tray(&mut self, tray: Option<crate::Tray>) {
        self.host.tray = Some(tray);
    }
}
