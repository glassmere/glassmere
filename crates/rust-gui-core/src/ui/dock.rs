use super::*;
use crate::controls::DockAction;
use crate::{
    DockArea, DockLayout, DockNode, DockTabs, DockTarget, DockWindowId, DockZone, Length, MenuItem,
    SplitOptions, Tab, TabsResponse,
};
use rust_gui_scene::Color;
use std::cell::RefCell;

/// A panel the app can show in a dock. Layout ids without a declared panel are
/// skipped, and declared panels missing from the layout stay hidden until
/// `DockLayout::show`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockPanel {
    pub id: String,
    pub title: String,
    pub closable: bool,
}

impl DockPanel {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            closable: true,
        }
    }
    /// A panel without a close button.
    pub fn pinned(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            closable: false,
            ..Self::new(id, title)
        }
    }
}

#[derive(Clone, Debug)]
pub struct DockOptions {
    pub style: Style,
    /// Offer "Move to new window" and detach panels dropped outside the
    /// window. The app opens an OS window per `DockLayout::windows` entry.
    pub native_windows: bool,
    /// Smallest pane extent in logical pixels when dragging dividers.
    pub min_pane: f32,
}

impl Default for DockOptions {
    fn default() -> Self {
        Self {
            style: Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Percent(1.0)),
            native_windows: false,
            min_pane: 80.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DockResponse {
    /// The layout changed during this declaration; a good moment to save it.
    pub changed: bool,
    /// Panels closed by their tab button, the Delete key or the panel menu.
    pub closed: Vec<String>,
    /// Detached windows created during this declaration.
    pub detached: Vec<DockWindowId>,
}

const FLOAT_SIZE: (f32, f32) = (360.0, 260.0);
const WINDOW_SIZE: (f32, f32) = (640.0, 480.0);

struct Pass<'a> {
    panels: &'a [DockPanel],
    content: &'a mut dyn FnMut(&mut Ui<'_>, &str),
    /// Panel menu choices, applied once the tree is no longer borrowed.
    menu: Vec<(String, usize)>,
    closed: Vec<String>,
    changed: bool,
    native: bool,
    area: DockArea,
    tree_groups: usize,
    groups: usize,
}

fn fill() -> Style {
    Style::default()
        .width(Length::Percent(1.0))
        .grow(1.0)
        .shrink(1.0)
        .gap(0.0)
}

impl Ui<'_> {
    /// The main dock: the layout's split tree with tab groups, plus its
    /// floating groups. Tabs drag onto drop indicators, other tab strips or
    /// empty space (floating), and every group has a panel menu that offers
    /// the same moves from the keyboard. `content` declares one visible panel.
    pub fn dock(
        &mut self,
        key: impl Into<String>,
        layout: &mut DockLayout,
        panels: &[DockPanel],
        options: DockOptions,
        content: impl FnMut(&mut Ui<'_>, &str),
    ) -> DockResponse {
        self.dock_area(key.into(), layout, DockArea::Main, panels, options, content)
    }

    /// The dock of one detached window. Declares nothing once the window has
    /// left the layout, which is the app's cue to close the OS window.
    pub fn dock_window(
        &mut self,
        key: impl Into<String>,
        layout: &mut DockLayout,
        window: DockWindowId,
        panels: &[DockPanel],
        options: DockOptions,
        content: impl FnMut(&mut Ui<'_>, &str),
    ) -> DockResponse {
        self.dock_area(
            key.into(),
            layout,
            DockArea::Window(window),
            panels,
            options,
            content,
        )
    }

    fn dock_area(
        &mut self,
        key: String,
        layout: &mut DockLayout,
        area: DockArea,
        panels: &[DockPanel],
        options: DockOptions,
        mut content: impl FnMut(&mut Ui<'_>, &str),
    ) -> DockResponse {
        layout.normalize();
        let windows: Vec<_> = layout.windows.iter().map(|window| window.id).collect();
        let mut changed = false;
        let dock_bounds = self
            .context
            .child(self.parent, &WidgetKey::Named(key.clone()), Kind::Dock)
            .and_then(|id| self.context.node(id))
            .map(|node| node.bounds);
        match self.take_update(&key, Kind::Dock) {
            Some(ControlUpdate::Dock(DockAction::Move { panel, target })) => {
                changed |= layout.move_panel(&panel, target);
            }
            Some(ControlUpdate::Dock(DockAction::FloatRect {
                float,
                x,
                y,
                width,
                height,
            })) => {
                // A float being moved or resized is also the frontmost one.
                if let Some(index) = layout
                    .floating
                    .iter()
                    .position(|candidate| candidate.tabs.panels.first() == Some(&float))
                {
                    let mut target = layout.floating.remove(index);
                    (target.x, target.y, target.width, target.height) = (x, y, width, height);
                    layout.floating.push(target);
                    changed = true;
                }
            }
            Some(ControlUpdate::Dock(DockAction::Raise(float))) => {
                if let Some(index) = layout
                    .floating
                    .iter()
                    .position(|candidate| candidate.tabs.panels.first() == Some(&float))
                {
                    let raised = layout.floating.remove(index);
                    layout.floating.push(raised);
                    changed = true;
                }
            }
            _ => {}
        }
        let tree_groups = layout.groups_in(area).len()
            - if area == DockArea::Main {
                layout.floating.len()
            } else {
                0
            };
        let pass = RefCell::new(Pass {
            panels,
            content: &mut content,
            menu: Vec::new(),
            closed: Vec::new(),
            changed: false,
            native: options.native_windows,
            area,
            tree_groups,
            groups: layout.groups_in(area).len(),
        });
        let popup = self.context.theme().popup;
        let mut style = options.style.clone();
        style.gap = 0.0;
        let min_pane = options.min_pane;
        self.group(key, Kind::Dock, style, |ui| {
            let root = match area {
                DockArea::Main => layout.root.as_mut(),
                DockArea::Window(id) => layout
                    .windows
                    .iter_mut()
                    .find(|window| window.id == id)
                    .map(|window| &mut window.root),
            };
            if let Some(root) = root {
                node(ui, root, &pass, min_pane);
            }
            if area != DockArea::Main {
                return;
            }
            for float in &mut layout.floating {
                let Some(first) = float.tabs.panels.first().cloned() else {
                    continue;
                };
                // Keep the grip reachable after the window shrinks.
                if let Some(bounds) = dock_bounds.filter(|bounds| bounds.width > 0.0) {
                    float.x = float.x.clamp(0.0, (bounds.width - 60.0).max(0.0));
                    float.y = float.y.clamp(0.0, (bounds.height - 24.0).max(0.0));
                }
                let (x, y, width, height) = (float.x, float.y, float.width, float.height);
                let handle = |resize| Control::DockHandle {
                    float: first.clone(),
                    resize,
                    x,
                    y,
                    width,
                    height,
                };
                ui.group(
                    format!("$float:{first}"),
                    Kind::DockFloat,
                    Style::default()
                        .offset(x, y)
                        .width(Length::Px(width))
                        .height(Length::Px(height))
                        .padding(4.0)
                        .gap(0.0)
                        .radius(8.0)
                        .background(popup),
                    |ui| {
                        ui.push(Element {
                            key: WidgetKey::Named("$grip".into()),
                            kind: Kind::DockHandle,
                            text: String::new(),
                            style: Style::default()
                                .width(Length::Percent(1.0))
                                .height(Length::Px(12.0))
                                .radius(4.0),
                            enabled: ui.enabled,
                            children: Vec::new(),
                            custom: None,
                            tooltip: None,
                            control: handle(false),
                        });
                        group(ui, &mut float.tabs, &pass, true);
                        ui.push(Element {
                            key: WidgetKey::Named("$resize".into()),
                            kind: Kind::DockHandle,
                            text: String::new(),
                            style: Style::default()
                                .offset(width - 16.0, height - 16.0)
                                .width(Length::Px(14.0))
                                .height(Length::Px(14.0))
                                .radius(4.0),
                            enabled: ui.enabled,
                            children: Vec::new(),
                            custom: None,
                            tooltip: None,
                            control: handle(true),
                        });
                    },
                );
                ui.elements.last_mut().unwrap().control = Control::DockFloat;
            }
        });
        self.elements.last_mut().unwrap().control = Control::Dock {
            area,
            native: options.native_windows,
        };
        let Pass {
            menu,
            mut closed,
            changed: pass_changed,
            ..
        } = pass.into_inner();
        changed |= pass_changed;
        for panel in &closed {
            changed |= layout.close(panel);
        }
        for (panel, choice) in menu {
            let target = match choice {
                0 => {
                    let step = 24.0 * layout.floating.len() as f32;
                    DockTarget::Float {
                        x: 40.0 + step,
                        y: 40.0 + step,
                        width: FLOAT_SIZE.0,
                        height: FLOAT_SIZE.1,
                    }
                }
                1 => DockTarget::Window {
                    width: WINDOW_SIZE.0,
                    height: WINDOW_SIZE.1,
                },
                2 => DockTarget::Edge {
                    area: DockArea::Main,
                    zone: DockZone::Center,
                },
                3..=6 => DockTarget::Edge {
                    area,
                    zone: [
                        DockZone::Left,
                        DockZone::Right,
                        DockZone::Top,
                        DockZone::Bottom,
                    ][choice - 3],
                },
                7 => {
                    let groups = layout.groups_in(area);
                    let Some(current) = groups.iter().position(|group| group.contains(&panel))
                    else {
                        continue;
                    };
                    DockTarget::Group {
                        anchor: groups[(current + 1) % groups.len()][0].clone(),
                        zone: DockZone::Center,
                    }
                }
                _ => {
                    if layout.close(&panel) {
                        closed.push(panel);
                        changed = true;
                    }
                    continue;
                }
            };
            changed |= layout.move_panel(&panel, target);
        }
        DockResponse {
            changed,
            closed,
            detached: layout
                .windows
                .iter()
                .map(|window| window.id)
                .filter(|id| !windows.contains(id))
                .collect(),
        }
    }
}

fn node(ui: &mut Ui<'_>, target: &mut DockNode, pass: &RefCell<Pass<'_>>, min_pane: f32) {
    match target {
        DockNode::Tabs(tabs) => group(ui, tabs, pass, false),
        DockNode::Split {
            axis,
            fraction,
            first,
            second,
        } => {
            let old = *fraction;
            ui.split(
                "$split",
                fraction,
                *axis,
                SplitOptions {
                    min_first: min_pane,
                    min_second: min_pane,
                    style: fill(),
                    ..SplitOptions::default()
                },
                |ui| node(ui, first, pass, min_pane),
                |ui| node(ui, second, pass, min_pane),
            );
            if old != *fraction {
                pass.borrow_mut().changed = true;
            }
        }
    }
}

fn group(ui: &mut Ui<'_>, tabs: &mut DockTabs, pass: &RefCell<Pass<'_>>, float: bool) {
    // ponytail: ids the app does not declare are skipped here, so tab indexes
    // reported by drags are positions among the visible tabs.
    let visible: Vec<(usize, Tab, String)> = {
        let pass = pass.borrow();
        tabs.panels
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let panel = pass.panels.iter().find(|panel| panel.id == *id)?;
                let tab = if panel.closable {
                    Tab::closable(panel.title.clone())
                } else {
                    Tab::new(panel.title.clone())
                };
                Some((index, tab, id.clone()))
            })
            .collect()
    };
    if visible.is_empty() {
        ui.group("$group".into(), Kind::Container, fill(), |_| {});
        return;
    }
    let labels: Vec<Tab> = visible.iter().map(|(_, tab, _)| tab.clone()).collect();
    let ids: Vec<String> = visible.iter().map(|(_, _, id)| id.clone()).collect();
    let mut selected = visible
        .iter()
        .position(|(index, ..)| *index == tabs.active)
        .unwrap_or(0);
    let count = tabs.panels.len();
    let mut response = TabsResponse::default();
    ui.group("$group".into(), Kind::DockGroup, fill(), |ui| {
        response = ui.tabs_with(
            "$tabs",
            &mut selected,
            &labels,
            true,
            |ui, current| {
                menu(
                    ui,
                    &ids[current],
                    labels[current].closable,
                    pass,
                    float,
                    count,
                )
            },
            |ui, current| {
                let mut pass = pass.borrow_mut();
                (pass.content)(ui, &ids[current]);
            },
        );
    });
    ui.elements.last_mut().unwrap().control = Control::DockGroup {
        panels: ids.clone(),
        float,
    };
    let mut pass = pass.borrow_mut();
    if response.changed {
        tabs.active = visible[selected].0;
        pass.changed = true;
    }
    if let Some(index) = response.closed {
        pass.closed.push(ids[index].clone());
    }
}

/// The group's panel menu, right-aligned in the tab strip. It acts on the
/// active panel and also opens from a right-click or Shift+F10 on the strip.
fn menu(
    ui: &mut Ui<'_>,
    panel: &str,
    closable: bool,
    pass: &RefCell<Pass<'_>>,
    float: bool,
    count: usize,
) {
    let (native, area, tree_groups, groups) = {
        let pass = pass.borrow();
        (pass.native, pass.area, pass.tree_groups, pass.groups)
    };
    let detached = matches!(area, DockArea::Window(_));
    // The only panel of an area has nowhere to go inside it.
    let alone = !float && tree_groups == 1 && count == 1;
    let item = |label: &str, enabled: bool| {
        if enabled {
            MenuItem::new(label)
        } else {
            MenuItem::disabled(label)
        }
    };
    let items = [
        item("Float", !detached && !(float && count == 1)),
        item("Move to new window", native && !(detached && alone)),
        item("Move to main window", detached),
        item("Dock left", !alone),
        item("Dock right", !alone),
        item("Dock top", !alone),
        item("Dock bottom", !alone),
        item("Move to next group", groups > 1),
        item("Close", closable),
    ];
    ui.label_styled("$gap", "", Style::default().grow(1.0));
    let style = Style {
        padding: 6.0,
        radius: 6.0,
        background: Some(Color::TRANSPARENT),
        ..Style::default()
    };
    if let Some(choice) = ui.menu_styled("$menu", "▾", &items, style, Length::Px(220.0)) {
        pass.borrow_mut().menu.push((panel.to_owned(), choice));
    }
    ui.tooltip("Panel options");
}
