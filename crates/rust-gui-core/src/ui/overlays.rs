use super::*;
use crate::{
    Align, Axis, Length, MenuItem, PopoverOptions, PopoverResponse, SplitOptions, Tab, TabsResponse,
};
use rust_gui_scene::Color;

impl Ui<'_> {
    /// Attaches hover help to the previously declared widget. It appears after a
    /// short dwell and doubles as the widget's accessible description.
    pub fn tooltip(&mut self, text: impl Into<String>) {
        if let Some(element) = self.elements.last_mut() {
            element.tooltip = Some(text.into());
        }
    }

    /// A tab strip over one panel. Clicking or Arrow/Home/End on the focused
    /// tab selects at once; only the selected tab is in the Tab order, and Tab
    /// moves on into the panel. Closable tabs report their close button.
    pub fn tabs(
        &mut self,
        key: impl Into<String>,
        selected: &mut usize,
        tabs: &[Tab],
        content: impl FnOnce(&mut Ui<'_>, usize),
    ) -> TabsResponse {
        self.tabs_with(key, selected, tabs, false, |_, _| {}, content)
    }

    /// `tabs` with extra strip content after the tabs. `fill` stretches the
    /// panel over the parent's remaining height, as dock groups need.
    pub(super) fn tabs_with(
        &mut self,
        key: impl Into<String>,
        selected: &mut usize,
        tabs: &[Tab],
        fill: bool,
        strip_extra: impl FnOnce(&mut Ui<'_>, usize),
        content: impl FnOnce(&mut Ui<'_>, usize),
    ) -> TabsResponse {
        let key = key.into();
        let old = *selected;
        if let Some(ControlUpdate::Choice(index)) = self.take_update(&key, Kind::Tabs) {
            *selected = index;
        }
        *selected = (*selected).min(tabs.len().saturating_sub(1));
        let current = *selected;
        let mut closed = None;
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
                    "$strip".into(),
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
                                key: WidgetKey::Named(index.to_string()),
                                kind: Kind::Tab,
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
                                children: Vec::new(),
                                custom: None,
                                tooltip: None,
                                control: Control::Tab {
                                    index,
                                    selected: active,
                                    closable: tab.closable,
                                },
                            });
                            if tab.closable
                                && ui
                                    .button_styled(
                                        format!("$close-{index}"),
                                        "×",
                                        true,
                                        Style {
                                            padding: 6.0,
                                            radius: 6.0,
                                            background: Some(Color::TRANSPARENT),
                                            ..Style::default()
                                        },
                                    )
                                    .clicked()
                            {
                                closed = Some(index);
                            }
                            if tab.closable {
                                // The button shows only a symbol; this names it.
                                ui.tooltip(ui.context.tr(
                                    "rust-gui-tab-close",
                                    &[("name", tab.label.as_str().into())],
                                ));
                            }
                        }
                        strip_extra(ui, current);
                    },
                );
                ui.elements.last_mut().unwrap().control = Control::TabList;
                ui.group(
                    "$panel".into(),
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
                ui.elements.last_mut().unwrap().control = Control::TabPanel;
            },
        );
        self.elements.last_mut().unwrap().control = Control::Tabs {
            count,
            selected: current,
        };
        TabsResponse {
            changed: old != *selected,
            closed,
        }
    }

    /// Two panes separated by a draggable divider. `fraction` is the first
    /// pane's share of the space along `axis`; pixel minimums clamp both sides.
    /// The divider is focusable and moves with the arrow keys, Home and End.
    pub fn split(
        &mut self,
        key: impl Into<String>,
        fraction: &mut f32,
        axis: Axis,
        options: SplitOptions,
        first: impl FnOnce(&mut Ui<'_>),
        second: impl FnOnce(&mut Ui<'_>),
    ) {
        let key = key.into();
        let kind = match axis {
            Axis::Horizontal => Kind::SplitRow,
            Axis::Vertical => Kind::Split,
        };
        if let Some(ControlUpdate::Number(value)) = self.take_update(&key, kind) {
            *fraction = value as f32;
        }
        if !fraction.is_finite() {
            *fraction = 0.5;
        }
        *fraction = fraction.clamp(0.0, 1.0);
        let share = *fraction;
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
            ui.group("$first".into(), Kind::Container, first_style, first);
            ui.push(Element {
                key: WidgetKey::Named("$divider".into()),
                kind: Kind::Divider,
                text: String::new(),
                style: divider_style,
                enabled: ui.enabled,
                children: Vec::new(),
                custom: None,
                tooltip: None,
                control: Control::Divider {
                    axis,
                    fraction: share,
                    min_first: options.min_first,
                    min_second: options.min_second,
                },
            });
            ui.group("$second".into(), Kind::Container, second_style, second);
        });
        self.elements.last_mut().unwrap().control = Control::Split { axis };
    }

    /// Content with a right-click (or Shift+F10 / Menu key) menu. Returns the
    /// activated item index once, like `Ui::menu`.
    /// Everything the window can do, filtered by what is typed. The
    /// application owns whether it is open and the query; the id of the
    /// command that was chosen comes back once, and closes it.
    ///
    /// Commands are shown with the shortcut the user would press, and a
    /// disabled one cannot be chosen. The palette declares no commands of its
    /// own, so it never takes a shortcut from the window.
    pub fn command_palette(
        &mut self,
        key: impl Into<String>,
        open: &mut bool,
        query: &mut String,
        commands: &[crate::Command],
    ) -> Option<String> {
        let key = key.into();
        let mut chosen = None;
        // Nothing is there on the frame the palette appears on.
        let opened = self
            .context
            .child(
                Some(self.context.root()),
                &WidgetKey::Named(format!("$dialog:{key}")),
                Kind::Dialog,
            )
            .is_none();
        let title = self.context.tr("rust-gui-palette", &[]);
        let options = crate::DialogOptions {
            width: Length::Px(560.0),
            ..crate::DialogOptions::new(title)
        };
        self.dialog(key, open, options, |ui, open| {
            if opened {
                // The palette is typed into the moment it appears.
                ui.request_focus("query");
            }
            let placeholder = ui.context.tr("rust-gui-palette-query", &[]);
            let field = ui.text_input("query", &placeholder, query);
            let matches: Vec<crate::Command> = commands
                .iter()
                .filter(|command| crate::text_matches(&command.label, query))
                .map(|command| ui.command(command))
                .take(12)
                .collect();
            // Enter in the field takes the first command that can run.
            if field.submitted {
                chosen = matches
                    .iter()
                    .find(|command| command.enabled)
                    .map(|command| command.id.clone());
            }
            let row = Style {
                width: Length::Percent(1.0),
                padding: 8.0,
                radius: 6.0,
                background: None,
                ..Style::button()
            };
            ui.column(
                "matches",
                Style::default().gap(2.0).width(Length::Percent(1.0)),
                |ui| {
                    for (index, command) in matches.iter().enumerate() {
                        if ui
                            .button_styled(
                                format!("match{index}"),
                                &command.label,
                                command.enabled,
                                row.clone(),
                            )
                            .clicked()
                        {
                            chosen = Some(command.id.clone());
                        }
                        if let Some(shortcut) = &command.shortcut {
                            ui.elements.last_mut().unwrap().control =
                                Control::Hint(shortcut.describe());
                        }
                    }
                    if matches.is_empty() {
                        let nothing = ui.context.tr("rust-gui-palette-empty", &[]);
                        ui.label_keyed("empty", nothing);
                    }
                },
            );
            if chosen.is_some() {
                *open = false;
            }
        });
        if chosen.is_some() {
            query.clear();
        }
        chosen
    }

    pub fn context_menu(
        &mut self,
        key: impl Into<String>,
        items: &[MenuItem],
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) -> Option<usize> {
        let key = key.into();
        let choice = match self.take_update(&key, Kind::ContextArea) {
            Some(ControlUpdate::Choice(index))
                if items.get(index).is_some_and(|item| item.enabled) =>
            {
                Some(index)
            }
            _ => None,
        };
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
        self.elements.last_mut().unwrap().control = Control::ContextMenu {
            items: items.to_vec(),
        };
        choice
    }

    /// An anchored, non-modal panel below the previously declared widget while
    /// `*open`. Focus moves inside on open and returns to the anchor on close;
    /// Escape or a press outside the panel and its anchor clears `*open`.
    pub fn popover(
        &mut self,
        key: impl Into<String>,
        open: &mut bool,
        options: PopoverOptions,
        content: impl FnOnce(&mut Ui<'_>),
    ) -> PopoverResponse {
        let key = WidgetKey::Named(key.into());
        let Some(anchor) = self.elements.last() else {
            panic!("popover needs a preceding anchor widget in the same parent");
        };
        let anchor_id = self.context.child(self.parent, &anchor.key, anchor.kind);
        let popover_id = anchor_id.and_then(|id| self.context.child(Some(id), &key, Kind::Popover));
        let dismissed = self.enabled
            && popover_id
                .is_some_and(|id| matches!(self.updates.remove(&id), Some(ControlUpdate::Dismiss)));
        if dismissed {
            *open = false;
        }
        if !*open {
            return PopoverResponse { dismissed };
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
        let mut child = Ui {
            context: self.context,
            parent: popover_id,
            clicks: self.clicks,
            elements: Vec::new(),
            keys: HashSet::new(),
            updates: self.updates,
            submitted: self.submitted,
            warnings: self.warnings,
            theme_request: self.theme_request,
            clipboard: self.clipboard,
            redraw: self.redraw,
            focus: self.focus,
            host: self.host,
            layers: self.layers,
            enabled: self.enabled,
        };
        content(&mut child);
        let element = Element {
            key,
            kind: Kind::Popover,
            text: String::new(),
            style: panel,
            enabled: self.enabled,
            children: child.elements,
            custom: None,
            tooltip: None,
            control: Control::Popover,
        };
        self.elements.last_mut().unwrap().children.push(element);
        PopoverResponse { dismissed }
    }
}
