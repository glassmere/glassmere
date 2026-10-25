use super::*;
use crate::{Align, Axis, Length, MenuItem, PopoverOptions, SplitOptions, Tab, TextInput};
use glassmere_scene::Color;
use std::rc::Rc;

/// What a command palette sends; see [`Ui::command_palette`].
pub struct CommandPalette<M = ()> {
    on_query: Box<dyn Fn(String) -> M>,
    on_open: Option<Box<dyn Fn(bool) -> M>>,
    on_pick: Option<Rc<dyn Fn(usize) -> M>>,
}

impl<M> std::fmt::Debug for CommandPalette<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandPalette").finish_non_exhaustive()
    }
}

impl<M> CommandPalette<M> {
    /// A palette that sends `on_query(text)` as the query is typed.
    pub fn new(on_query: impl Fn(String) -> M + 'static) -> Self {
        Self {
            on_query: Box::new(on_query),
            on_open: None,
            on_pick: None,
        }
    }

    /// Escape or a scrim click sends `message(false)`.
    pub fn on_open(mut self, message: impl Fn(bool) -> M + 'static) -> Self {
        self.on_open = Some(Box::new(message));
        self
    }

    /// Enter, or a click on a row, sends `message(index)`, where `index` is
    /// into the commands the palette was given.
    pub fn on_pick(mut self, message: impl Fn(usize) -> M + 'static) -> Self {
        self.on_pick = Some(Rc::new(message));
        self
    }
}

impl<'a, M: 'static> Ui<'a, M> {
    /// Attaches hover help to the previously declared widget. It appears after a
    /// short dwell and doubles as the widget's accessible description.
    #[track_caller]
    pub fn tooltip(&mut self, text: impl Into<String>) {
        if let Some(element) = self.declared() {
            element.tooltip = Some(text.into());
        }
    }

    /// A tab strip over one panel, showing tab `selected`. Clicking or
    /// Arrow/Home/End on the focused tab sends `on_select(index)`; only the
    /// selected tab is in the Tab order, and Tab moves on into the panel.
    /// Closing a closable tab is not reported here; `tabs_close` reports it.
    #[track_caller]
    pub fn tabs(
        &mut self,
        selected: usize,
        tabs: &[Tab],
        on_select: impl Fn(usize) -> M + 'static,
        content: impl FnOnce(&mut Ui<'_, M>, usize),
    ) {
        self.tabs_close_with(selected, tabs, on_select, None, content);
    }

    /// `tabs` that also sends `on_close(index)` when the close button of a
    /// closable tab is pressed, or Delete on the focused tab.
    #[track_caller]
    pub fn tabs_close(
        &mut self,
        selected: usize,
        tabs: &[Tab],
        on_select: impl Fn(usize) -> M + 'static,
        on_close: impl Fn(usize) -> M,
        content: impl FnOnce(&mut Ui<'_, M>, usize),
    ) {
        self.tabs_close_with(selected, tabs, on_select, Some(&on_close), content);
    }

    #[track_caller]
    fn tabs_close_with(
        &mut self,
        selected: usize,
        tabs: &[Tab],
        on_select: impl Fn(usize) -> M + 'static,
        on_close: Option<&dyn Fn(usize) -> M>,
        content: impl FnOnce(&mut Ui<'_, M>, usize),
    ) {
        self.tabs_with(selected, tabs, false, on_close, |_, _| {}, content);
        // The strip's choices reach the tabs group, which `tabs` declared last.
        self.set_handler(crate::app::Handler::Map(Box::new(
            move |update| match update {
                ControlUpdate::Choice(index) => Some(on_select(index)),
                _ => None,
            },
        )));
    }

    /// `tabs` with extra strip content after the tabs. `fill` stretches the
    /// panel over the parent's remaining height, as dock groups need.
    #[track_caller]
    pub(super) fn tabs_with(
        &mut self,
        selected: usize,
        tabs: &[Tab],
        fill: bool,
        on_close: Option<&dyn Fn(usize) -> M>,
        strip_extra: impl FnOnce(&mut Ui<'_, M>, usize),
        content: impl FnOnce(&mut Ui<'_, M>, usize),
    ) {
        let key = self.next_key();
        let current = selected.min(tabs.len().saturating_sub(1));
        let (surface, hover) = {
            let theme = self.context.theme();
            (theme.surface, theme.control_hover)
        };
        let count = tabs.len();
        let stretch = |style: Style| {
            if fill {
                style.grow(1.0).shrink(1.0)
            } else {
                style
            }
        };
        self.group(
            key,
            Kind::Tabs,
            stretch(Style::default().width(Length::Percent(1.0)).gap(0.0)),
            |ui| {
                let strip = Style::default().gap(2.0).align_items(Align::End);
                ui.group(
                    WidgetKey::named("$strip"),
                    Kind::TabList,
                    if fill {
                        strip.width(Length::Percent(1.0))
                    } else {
                        strip
                    },
                    |ui| {
                        for (index, tab) in tabs.iter().enumerate() {
                            let active = index == current;
                            ui.push(Element {
                                text: tab.label.clone(),
                                style: Style {
                                    padding: 10.0,
                                    radius: 6.0,
                                    background: active.then_some(surface),
                                    states: (!active).then(|| {
                                        Box::new(crate::StateStyles {
                                            hover: crate::Paint::background(hover),
                                            ..Default::default()
                                        })
                                    }),
                                    ..Style::default()
                                },
                                enabled: ui.enabled,
                                control: Control::Tab {
                                    index,
                                    selected: active,
                                    closable: tab.closable,
                                },
                                ..Element::new(WidgetKey::Named(index.to_string()), Kind::Tab)
                            });
                            if tab.closable {
                                ui.button_at(
                                    format!("$close-{index}"),
                                    "×",
                                    true,
                                    Style {
                                        padding: 6.0,
                                        radius: 6.0,
                                        background: Some(Color::TRANSPARENT),
                                        ..Style::default()
                                    },
                                );
                                if let Some(on_close) = on_close {
                                    ui.set_handler(crate::app::Handler::Message(on_close(index)));
                                }
                                // The button shows only a symbol; this names it.
                                ui.tooltip(ui.context.tr(
                                    "glassmere-tab-close",
                                    &[("name", tab.label.as_str().into())],
                                ));
                            }
                        }
                        strip_extra(ui, current);
                    },
                );
                ui.set_control(Control::TabList);
                ui.group(
                    WidgetKey::named("$panel"),
                    Kind::TabPanel,
                    stretch(
                        Style::default()
                            .width(Length::Percent(1.0))
                            .padding(if fill { 10.0 } else { 16.0 })
                            .radius(8.0)
                            .background(surface),
                    ),
                    |ui| content(ui, current),
                );
                ui.set_control(Control::TabPanel);
            },
        );
        self.set_control(Control::Tabs {
            count,
            selected: current,
        });
    }

    /// Two panes separated by a draggable divider. `ratio` is the first
    /// pane's share of the space along `axis`, between 0.0 and 1.0; pixel
    /// minimums clamp both sides. Dragging the divider sends `on` of the new
    /// share. The divider is focusable and moves with the arrow keys, Home
    /// and End.
    #[track_caller]
    pub fn split(
        &mut self,
        ratio: f32,
        axis: Axis,
        options: SplitOptions,
        on: impl Fn(f32) -> M + 'static,
        first: impl FnOnce(&mut Ui<'_, M>),
        second: impl FnOnce(&mut Ui<'_, M>),
    ) {
        let key = self.next_key();
        let kind = match axis {
            Axis::Horizontal => Kind::SplitRow,
            Axis::Vertical => Kind::Split,
        };
        let share = if ratio.is_finite() {
            ratio.clamp(0.0, 1.0)
        } else {
            0.5
        };
        let mut style = options.style.clone();
        style.gap = 0.0;
        let first_style = match axis {
            Axis::Horizontal => Style::default()
                .width(Length::Percent(share))
                .min_width(Length::Px(options.min_first))
                .height(Length::Percent(1.0)),
            Axis::Vertical => Style::default()
                .height(Length::Percent(share))
                .min_height(Length::Px(options.min_first))
                .width(Length::Percent(1.0)),
        }
        .gap(0.0);
        let divider_style = match axis {
            Axis::Horizontal => Style::default()
                .width(Length::Px(options.divider))
                .height(Length::Percent(1.0)),
            Axis::Vertical => Style::default()
                .height(Length::Px(options.divider))
                .width(Length::Percent(1.0)),
        };
        let second_style = match axis {
            Axis::Horizontal => Style::default()
                .min_width(Length::Px(options.min_second))
                .height(Length::Percent(1.0)),
            Axis::Vertical => Style::default()
                .min_height(Length::Px(options.min_second))
                .width(Length::Percent(1.0)),
        }
        .grow(1.0)
        .shrink(1.0)
        .gap(0.0);
        self.group(key, kind, style, |ui| {
            ui.group(
                WidgetKey::named("$first"),
                Kind::Container,
                first_style,
                first,
            );
            ui.push(Element {
                style: divider_style,
                enabled: ui.enabled,
                control: Control::Divider {
                    axis,
                    fraction: share,
                    min_first: options.min_first,
                    min_second: options.min_second,
                },
                ..Element::new(WidgetKey::Named("$divider".into()), Kind::Divider)
            });
            ui.group(
                WidgetKey::named("$second"),
                Kind::Container,
                second_style,
                second,
            );
        });
        self.set_control(Control::Split { axis });
        self.set_handler(crate::app::Handler::Map(Box::new(
            move |update| match update {
                ControlUpdate::Number(number) if number.is_finite() => {
                    Some(on((number as f32).clamp(0.0, 1.0)))
                }
                _ => None,
            },
        )));
    }

    /// Everything the window can do, filtered by what is typed. The
    /// application owns whether it is open and the query. Commands are shown
    /// with the shortcut the user would press, and a disabled one cannot be
    /// chosen. The palette declares no commands of its own, so it never takes
    /// a shortcut from the window.
    ///
    /// Escape or a scrim click sends `palette`'s `on_open(false)`; typing sends
    /// its `on_query(text)`; Enter or a click on a row sends its
    /// `on_pick(index)`, where `index` is into `commands`. The application
    /// closes the palette and clears the query in `update`.
    #[track_caller]
    pub fn command_palette(
        &mut self,
        open: bool,
        query: &str,
        commands: &[crate::Command],
        palette: CommandPalette<M>,
    ) {
        let CommandPalette {
            on_query,
            on_open,
            on_pick,
        } = palette;
        // Dialogs need a plain name; a call-site key has none of its own.
        let name = match self.next_key() {
            WidgetKey::Named(name) => name,
            WidgetKey::Site(..) => "$palette".to_owned(),
        };
        // Nothing is there on the frame the palette appears on.
        let opened = self
            .context
            .child(
                Some(self.context.root()),
                &WidgetKey::Named(format!("$dialog:{name}")),
                Kind::Dialog,
            )
            .is_none();
        let title = self.context.tr("glassmere-palette", &[]);
        let options = crate::DialogOptions {
            width: Length::Px(560.0),
            ..crate::DialogOptions::new(title)
        };
        self.dialog_declared(
            name,
            open,
            options,
            |ui| {
                if opened {
                    // The palette is typed into the moment it appears.
                    ui.request_focus("query");
                }
                let matches: Vec<(usize, crate::Command)> = commands
                    .iter()
                    .enumerate()
                    .filter(|(_, command)| crate::text_matches(&command.label, query))
                    .map(|(index, command)| (index, ui.command(command)))
                    .take(12)
                    .collect();
                // Enter in the field picks the first command that can run.
                let first = matches
                    .iter()
                    .find(|(_, command)| command.enabled)
                    .map(|(index, _)| *index);
                let mut input = TextInput::new(ui.context.tr("glassmere-palette-query", &[]));
                if let (Some(index), Some(on_pick)) = (first, &on_pick) {
                    input = input.on_submit(on_pick(index));
                }
                ui.at("query").text_input(input, query, on_query);
                let row = Style {
                    width: Length::Percent(1.0),
                    padding: 8.0,
                    radius: 6.0,
                    background: None,
                    ..Style::button()
                };
                ui.column_at(
                    "matches",
                    Style::default().gap(2.0).width(Length::Percent(1.0)),
                    |ui| {
                        for (index, command) in &matches {
                            ui.button_at(
                                format!("match{index}"),
                                &command.label,
                                command.enabled,
                                row.clone(),
                            );
                            if let Some(on_pick) = &on_pick {
                                ui.set_handler(crate::app::Handler::Message(on_pick(*index)));
                            }
                            if let Some(shortcut) = &command.shortcut {
                                ui.set_control(Control::Hint(shortcut.describe()));
                            }
                        }
                        if matches.is_empty() {
                            let nothing = ui.context.tr("glassmere-palette-empty", &[]);
                            ui.label_keyed_at("empty", nothing);
                        }
                    },
                );
            },
            on_open.map_or_else(|| crate::app::Handler::Map(Box::new(|_| None)), closing),
        );
    }

    /// Content with a right-click (or Shift+F10 / Menu key) menu that sends
    /// `on_pick(index)` when an enabled item is chosen.
    #[track_caller]
    pub fn context_menu(
        &mut self,
        items: &[MenuItem],
        style: Style,
        on_pick: impl Fn(usize) -> M + 'static,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        let pick = crate::app::Handler::Map(Box::new(move |update| match update {
            ControlUpdate::Choice(index) => Some(on_pick(index)),
            _ => None,
        }));
        self.context_menu_with(items, style, pick, content);
    }

    /// `context_menu` whose picks go to `pick`.
    #[track_caller]
    pub(super) fn context_menu_with(
        &mut self,
        items: &[MenuItem],
        style: Style,
        pick: crate::app::Handler<M>,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        let key = self.next_key();
        let popup = self.popup_children(
            &key,
            Kind::ContextArea,
            items,
            usize::MAX,
            true,
            Length::Px(220.0),
        );
        self.group(key, Kind::ContextArea, style, |ui| {
            // The popup panel must be the first child, like combo and menu owners.
            ui.elements.extend(popup);
            for element in &ui.elements {
                ui.keys.insert(element.key.clone());
            }
            content(ui);
        });
        self.set_handler(pick);
        self.set_control(Control::ContextMenu {
            items: items.to_vec(),
        });
    }

    /// An anchored, non-modal panel below the previously declared widget,
    /// shown while `open`. Focus moves inside on open and returns to the
    /// anchor on close; Escape or a press outside the panel and its anchor
    /// sends `on_open(false)`.
    #[track_caller]
    pub fn popover(
        &mut self,
        open: bool,
        options: PopoverOptions,
        on_open: impl Fn(bool) -> M + 'static,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        let key = self.next_key();
        let Some(anchor) = self.declared.and_then(|index| self.elements.get(index)) else {
            self.misuse("popover needs a preceding anchor widget in the same parent".to_owned());
            return;
        };
        let anchor_id = self.context.child(self.parent, &anchor.key, anchor.kind);
        let popover_id = anchor_id.and_then(|id| self.context.child(Some(id), &key, Kind::Popover));
        if !open {
            return;
        }
        let popup = self.context.theme().popup;
        let mut panel = options.style;
        panel.width = options.width;
        if panel.background.is_none() {
            panel.background = Some(popup);
        }
        if panel.radius == 0.0 {
            panel.radius = 8.0;
        }
        if panel.padding == 0.0 {
            panel.padding = 12.0;
        }
        let enabled = self.enabled;
        let handler = Some(self.handler_index(closing(on_open)));
        let mut child = self.scope(popover_id);
        content(&mut child);
        let element = Element {
            handler,
            seq: 0,
            key,
            kind: Kind::Popover,
            text: String::new(),
            style: panel,
            enabled,
            children: child.elements,
            custom: None,
            tooltip: None,
            control: Control::Popover,
        };
        if let Some(anchor) = self.declared() {
            anchor.children.push(element);
        }
    }

    /// Stores a handler for an element declared by hand, which `set_handler`
    /// cannot reach because it is not the last one pushed.
    pub(super) fn handler_index(&mut self, handler: crate::app::Handler<M>) -> u32 {
        let index = self.handler_base + self.handlers.len() as u32;
        self.handlers.push(handler);
        index
    }
}

/// The handler of a dismissible layer: its dismissal sends `on_open(false)`.
pub(super) fn closing<M: 'static>(on_open: impl Fn(bool) -> M + 'static) -> crate::app::Handler<M> {
    crate::app::Handler::Map(Box::new(move |update| match update {
        ControlUpdate::Dismiss => Some(on_open(false)),
        _ => None,
    }))
}
