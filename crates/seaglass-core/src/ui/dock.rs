use super::*;
use crate::app::Handler;
use crate::controls::DockAction;
use crate::{
    DockArea, DockFloat, DockLayout, DockNode, DockTabs, DockTarget, DockWindowId, DockZone,
    Length, MenuItem, SplitOptions, Tab,
};
use seaglass_scene::Color;
use std::cell::RefCell;
use std::rc::Rc;

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

const FLOAT_SIZE: (f32, f32) = (360.0, 260.0);
const WINDOW_SIZE: (f32, f32) = (640.0, 480.0);

/// Where a dock's changes go.
type Sink<M> = Rc<dyn Fn(DockAction) -> M>;

struct Pass<'a, M: 'static> {
    panels: &'a [DockPanel],
    /// The layout as it is declared; changes are never made here.
    layout: &'a DockLayout,
    content: &'a mut dyn FnMut(&mut Ui<'_, M>, &str),
    sink: Sink<M>,
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

/// Where a float is drawn: its declared origin, kept reachable after the
/// window shrinks.
fn float_origin(float: &DockFloat, bounds: Option<(f32, f32)>) -> (f32, f32) {
    match bounds {
        Some((width, height)) => (
            float.x.clamp(0.0, (width - 60.0).max(0.0)),
            float.y.clamp(0.0, (height - 24.0).max(0.0)),
        ),
        None => (float.x, float.y),
    }
}

fn set_handler_on<M>(
    element: &mut Element,
    base: u32,
    handlers: &mut Vec<Handler<M>>,
    handler: Handler<M>,
) {
    element.handler = Some(base + handlers.len() as u32);
    handlers.push(handler);
}

fn find_by_key<'e>(element: &'e mut Element, key: &WidgetKey) -> Option<&'e mut Element> {
    if element.key == *key {
        return Some(element);
    }
    element
        .children
        .iter_mut()
        .find_map(|child| find_by_key(child, key))
}

impl<'a, M: 'static> Ui<'a, M> {
    /// The main dock: the layout's split tree with tab groups, plus its
    /// floating groups. Tabs drag onto drop indicators, other tab strips or
    /// empty space (floating), and every group has a panel menu that offers
    /// the same moves from the keyboard. `content` declares one visible panel.
    ///
    /// The view only reads the layout. A drag, tab pick, close, divider move
    /// or panel-menu pick sends `on(action)`; `update` gives the action to
    /// [`DockLayout::apply`].
    #[track_caller]
    pub fn dock(
        &mut self,
        layout: &DockLayout,
        panels: &[DockPanel],
        options: DockOptions,
        on: impl Fn(DockAction) -> M + 'static,
        content: impl FnMut(&mut Ui<'_, M>, &str),
    ) {
        self.dock_message_area(layout, DockArea::Main, panels, options, on, content)
    }

    /// The dock of one detached window, sending what [`Ui::dock`] sends.
    /// Declares nothing once the window has left the layout, which is the
    /// app's cue to close the OS window.
    #[track_caller]
    pub fn dock_window(
        &mut self,
        layout: &DockLayout,
        window: DockWindowId,
        panels: &[DockPanel],
        options: DockOptions,
        on: impl Fn(DockAction) -> M + 'static,
        content: impl FnMut(&mut Ui<'_, M>, &str),
    ) {
        self.dock_message_area(
            layout,
            DockArea::Window(window),
            panels,
            options,
            on,
            content,
        )
    }

    #[track_caller]
    fn dock_message_area(
        &mut self,
        layout: &DockLayout,
        area: DockArea,
        panels: &[DockPanel],
        options: DockOptions,
        on: impl Fn(DockAction) -> M + 'static,
        mut content: impl FnMut(&mut Ui<'_, M>, &str),
    ) {
        let key = self.next_key();
        let bounds = self.dock_size(&key);
        let sink: Sink<M> = Rc::new(on);
        self.declare_dock(
            key,
            layout,
            area,
            panels,
            options,
            bounds,
            sink,
            &mut content,
        );
    }

    /// The size of the dock declared last time, when it has one.
    fn dock_size(&self, key: &WidgetKey) -> Option<(f32, f32)> {
        self.context
            .child(self.parent, key, Kind::Dock)
            .and_then(|id| self.context.node(id))
            .map(|node| node.bounds)
            .filter(|bounds| bounds.width > 0.0)
            .map(|bounds| (bounds.width, bounds.height))
    }

    /// Declares the dock of `area` as `layout` shows it.
    #[allow(clippy::too_many_arguments)]
    fn declare_dock(
        &mut self,
        key: WidgetKey,
        layout: &DockLayout,
        area: DockArea,
        panels: &[DockPanel],
        options: DockOptions,
        bounds: Option<(f32, f32)>,
        sink: Sink<M>,
        content: &mut dyn FnMut(&mut Ui<'_, M>, &str),
    ) {
        let tree_groups = layout.groups_in(area).len()
            - if area == DockArea::Main {
                layout.floating.len()
            } else {
                0
            };
        let pass = RefCell::new(Pass {
            panels,
            layout,
            content,
            sink: sink.clone(),
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
                DockArea::Main => layout.root.as_ref(),
                DockArea::Window(id) => layout
                    .windows
                    .iter()
                    .find(|window| window.id == id)
                    .map(|window| &window.root),
            };
            if let Some(root) = root {
                node(ui, root, &[], &pass, min_pane);
            }
            if area != DockArea::Main {
                return;
            }
            for float in &layout.floating {
                let Some(first) = float.tabs.panels.first().cloned() else {
                    continue;
                };
                let (x, y) = float_origin(float, bounds);
                let (width, height) = (float.width, float.height);
                let handle = |resize| Control::DockHandle {
                    float: first.clone(),
                    resize,
                    x,
                    y,
                    width,
                    height,
                };
                ui.group(
                    WidgetKey::named(format!("$float:{first}")),
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
                            style: Style::default()
                                .width(Length::Percent(1.0))
                                .height(Length::Px(12.0))
                                .radius(4.0),
                            enabled: ui.enabled,
                            control: handle(false),
                            ..Element::new(WidgetKey::Named("$grip".into()), Kind::DockHandle)
                        });
                        group(ui, &float.tabs, &pass, true);
                        ui.push(Element {
                            style: Style::default()
                                .offset(width - 16.0, height - 16.0)
                                .width(Length::Px(14.0))
                                .height(Length::Px(14.0))
                                .radius(4.0),
                            enabled: ui.enabled,
                            control: handle(true),
                            ..Element::new(WidgetKey::Named("$resize".into()), Kind::DockHandle)
                        });
                    },
                );
                ui.set_control(Control::DockFloat);
            }
        });
        self.set_control(Control::Dock {
            area,
            native: options.native_windows,
        });
        self.set_handler(Handler::Map(Box::new(move |update| match update {
            ControlUpdate::Dock(action) => Some(sink(action)),
            _ => None,
        })));
    }
}

/// A split at `path` from the root of its tree (0 is the first side).
fn node<M: 'static>(
    ui: &mut Ui<'_, M>,
    target: &DockNode,
    path: &[usize],
    pass: &RefCell<Pass<'_, M>>,
    min_pane: f32,
) {
    match target {
        DockNode::Tabs(tabs) => group(ui, tabs, pass, false),
        DockNode::Split {
            axis,
            fraction,
            first,
            second,
        } => {
            let (area, on) = {
                let pass = pass.borrow();
                (pass.area, pass.sink.clone())
            };
            let options = SplitOptions {
                min_first: min_pane,
                min_second: min_pane,
                style: fill(),
                ..SplitOptions::default()
            };
            let mut first_path = path.to_vec();
            first_path.push(0);
            let mut second_path = path.to_vec();
            second_path.push(1);
            let path = path.to_vec();
            ui.at("$split").split(
                *fraction,
                *axis,
                options,
                move |share| {
                    on(DockAction::Ratio {
                        area,
                        path: path.clone(),
                        fraction: share,
                    })
                },
                |ui| node(ui, first, &first_path, pass, min_pane),
                |ui| node(ui, second, &second_path, pass, min_pane),
            );
        }
    }
}

fn group<M: 'static>(
    ui: &mut Ui<'_, M>,
    tabs: &DockTabs,
    pass: &RefCell<Pass<'_, M>>,
    float: bool,
) {
    // Ids the app does not declare are skipped, so tab indexes reported by
    // drags are positions among the visible tabs.
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
        ui.group(WidgetKey::named("$group"), Kind::Container, fill(), |_| {});
        return;
    }
    let labels: Vec<Tab> = visible.iter().map(|(_, tab, _)| tab.clone()).collect();
    let ids: Vec<String> = visible.iter().map(|(_, _, id)| id.clone()).collect();
    let closable: Vec<bool> = labels.iter().map(|tab| tab.closable).collect();
    let selected = visible
        .iter()
        .position(|(index, ..)| *index == tabs.active)
        .unwrap_or(0);
    let count = tabs.panels.len();
    let sink = pass.borrow().sink.clone();
    ui.group(WidgetKey::named("$group"), Kind::DockGroup, fill(), |ui| {
        let strip = ui.at("$tabs");
        strip.tabs_with(
            selected,
            &labels,
            true,
            None,
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
        route_tabs(strip, &ids, &closable, &sink);
    });
    ui.set_control(Control::DockGroup { panels: ids, float });
}

/// Sends a tab strip's selection and its close buttons as messages. Delete on
/// a tab clicks its close button, so it reaches the same handler.
fn route_tabs<M: 'static>(
    ui: &mut Ui<'_, M>,
    ids: &[String],
    closable: &[bool],
    on: &Rc<dyn Fn(DockAction) -> M>,
) {
    let Some(index) = ui.declared else {
        return;
    };
    let base = ui.handler_base;
    let handlers = &mut *ui.handlers;
    let element = &mut ui.elements[index];
    let (order, select) = (ids.to_vec(), on.clone());
    set_handler_on(
        element,
        base,
        handlers,
        Handler::Map(Box::new(move |update| match update {
            ControlUpdate::Choice(choice) => order
                .get(choice)
                .map(|id| select(DockAction::Select(id.clone()))),
            _ => None,
        })),
    );
    for (index, _) in closable
        .iter()
        .enumerate()
        .filter(|(_, closable)| **closable)
    {
        let key = WidgetKey::Named(format!("$close-{index}"));
        let Some(button) = find_by_key(element, &key) else {
            continue;
        };
        let close = on.clone();
        let panel = ids[index].clone();
        set_handler_on(
            button,
            base,
            handlers,
            Handler::Message(close(DockAction::Close(panel))),
        );
    }
}

/// Sends the panel menu's pick as the action it names. A disabled item
/// sends nothing, as `Ui::menu` would not return it.
fn route_menu<M: 'static>(
    ui: &mut Ui<'_, M>,
    actions: Vec<Option<DockAction>>,
    on: Rc<dyn Fn(DockAction) -> M>,
) {
    let Some(index) = ui.declared else {
        return;
    };
    let base = ui.handler_base;
    let handlers = &mut *ui.handlers;
    set_handler_on(
        &mut ui.elements[index],
        base,
        handlers,
        Handler::Map(Box::new(move |update| match update {
            ControlUpdate::Choice(choice) => actions
                .get(choice)
                .cloned()
                .flatten()
                .map(|action| on(action)),
            _ => None,
        })),
    );
}

/// The group's panel menu, right-aligned in the tab strip. It acts on the
/// active panel and also opens from a right-click or Shift+F10 on the strip.
fn menu<M: 'static>(
    ui: &mut Ui<'_, M>,
    panel: &str,
    closable: bool,
    pass: &RefCell<Pass<'_, M>>,
    float: bool,
    count: usize,
) {
    let (native, area, tree_groups, groups, sink, layout) = {
        let pass = pass.borrow();
        (
            pass.native,
            pass.area,
            pass.tree_groups,
            pass.groups,
            pass.sink.clone(),
            pass.layout,
        )
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
    // What each item asks of the layout, worked out from the layout as declared.
    let action = |choice: usize| -> Option<DockAction> {
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
                let areas = layout.groups_in(area);
                let current = areas
                    .iter()
                    .position(|group| group.iter().any(|id| id.as_str() == panel))?;
                DockTarget::Group {
                    anchor: areas[(current + 1) % areas.len()].first()?.clone(),
                    zone: DockZone::Center,
                }
            }
            _ => return Some(DockAction::Close(panel.to_owned())),
        };
        Some(DockAction::Move {
            panel: panel.to_owned(),
            target,
        })
    };
    ui.label_at("$gap", "", Style::default().grow(1.0));
    let style = Style {
        padding: 6.0,
        radius: 6.0,
        background: Some(Color::TRANSPARENT),
        ..Style::default()
    };
    ui.menu_shown(
        WidgetKey::named("$menu"),
        "▾",
        &items,
        style,
        Length::Px(220.0),
    );
    let actions: Vec<Option<DockAction>> = items
        .iter()
        .enumerate()
        .map(|(choice, item)| if item.enabled { action(choice) } else { None })
        .collect();
    route_menu(ui, actions, sink);
    ui.tooltip("Panel options");
}
