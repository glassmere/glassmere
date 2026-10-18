use super::*;
use crate::controls::DockAction;
use crate::{DockArea, DockTarget, DockZone};

/// A tab being dragged out of a dock group.
pub(super) struct DockDrag {
    dock: NodeId,
    panel: String,
    title: String,
    start: Point,
    active: bool,
    /// The group under the pointer, whose compass is shown.
    group: Option<NodeId>,
    /// Where a release would dock, and the preview rectangle.
    target: Option<(DockTarget, Rect)>,
}

/// A floating group's grip or resize handle being dragged.
pub(super) struct DockGrab {
    handle: NodeId,
    start: Point,
    origin: Rect,
}

const INDICATOR: f32 = 32.0;
const FLOAT_SIZE: (f32, f32) = (360.0, 260.0);
const WINDOW_SIZE: (f32, f32) = (640.0, 480.0);
const MIN_FLOAT: (f32, f32) = (160.0, 100.0);

fn square(center: Point) -> Rect {
    Rect {
        x: center.x - INDICATOR / 2.0,
        y: center.y - INDICATOR / 2.0,
        width: INDICATOR,
        height: INDICATOR,
    }
}

/// Drop indicators at the middle of a group. Floating groups only take tabs.
fn compass(bounds: Rect, float: bool) -> Vec<(DockZone, Rect)> {
    let center = Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    };
    let step = INDICATOR + 4.0;
    let mut zones = vec![(DockZone::Center, square(center))];
    if !float {
        for (zone, dx, dy) in [
            (DockZone::Left, -step, 0.0),
            (DockZone::Right, step, 0.0),
            (DockZone::Top, 0.0, -step),
            (DockZone::Bottom, 0.0, step),
        ] {
            zones.push((
                zone,
                square(Point {
                    x: center.x + dx,
                    y: center.y + dy,
                }),
            ));
        }
    }
    zones
}

/// Drop indicators at the middle of each edge of the whole dock.
fn edges(bounds: Rect) -> [(DockZone, Rect); 4] {
    let inset = INDICATOR / 2.0 + 8.0;
    let (cx, cy) = (
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );
    [
        (
            DockZone::Left,
            square(Point {
                x: bounds.x + inset,
                y: cy,
            }),
        ),
        (
            DockZone::Right,
            square(Point {
                x: bounds.x + bounds.width - inset,
                y: cy,
            }),
        ),
        (
            DockZone::Top,
            square(Point {
                x: cx,
                y: bounds.y + inset,
            }),
        ),
        (
            DockZone::Bottom,
            square(Point {
                x: cx,
                y: bounds.y + bounds.height - inset,
            }),
        ),
    ]
}

/// The `share` of `bounds` on the zone's side; all of it for `Center`.
fn side(bounds: Rect, zone: DockZone, share: f32) -> Rect {
    let (width, height) = (bounds.width * share, bounds.height * share);
    match zone {
        DockZone::Center => bounds,
        DockZone::Left => Rect { width, ..bounds },
        DockZone::Right => Rect {
            x: bounds.x + bounds.width - width,
            width,
            ..bounds
        },
        DockZone::Top => Rect { height, ..bounds },
        DockZone::Bottom => Rect {
            y: bounds.y + bounds.height - height,
            height,
            ..bounds
        },
    }
}

impl Context {
    /// Whether a docking drag is showing its indicators.
    pub fn dock_dragging(&self) -> bool {
        self.dock_drag.as_ref().is_some_and(|drag| drag.active)
    }

    fn ancestor(&self, id: NodeId, kind: Kind) -> Option<NodeId> {
        let mut next = self.nodes.get(&id)?.parent;
        while let Some(id) = next {
            let node = self.nodes.get(&id)?;
            if node.kind == kind {
                return Some(id);
            }
            next = node.parent;
        }
        None
    }

    /// The panel menu button of the tab strip that holds `id` (a tab or the strip).
    pub(super) fn strip_menu(&self, id: NodeId) -> Option<NodeId> {
        let node = self.nodes.get(&id)?;
        let strip = match node.kind {
            Kind::Tab => node.parent?,
            Kind::TabList => id,
            _ => return None,
        };
        self.nodes[&strip]
            .children_by_key
            .get(&WidgetKey::Named("$menu".into()))
            .copied()
            .filter(|menu| self.nodes[menu].enabled)
    }

    /// Opens the panel menu for a right-click on a dock tab strip.
    pub(super) fn open_strip_menu_at(&mut self, point: Point) -> Option<bool> {
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        let hit = order.into_iter().rev().find(|id| {
            let node = &self.nodes[id];
            matches!(node.kind, Kind::Tab | Kind::TabList) && node.clip.contains(point)
        })?;
        let menu = self.strip_menu(hit)?;
        // The menu acts on the active panel, so a right-clicked tab becomes active.
        if matches!(
            self.nodes[&hit].control,
            Control::Tab {
                selected: false,
                ..
            }
        ) {
            self.select_tab(hit);
        }
        Some(self.open_popup_at(menu, Some(point)))
    }

    fn float_update(&mut self, handle: NodeId, rect: Rect) -> bool {
        let Some(dock) = self.ancestor(handle, Kind::Dock) else {
            return false;
        };
        let Control::DockHandle { float, .. } = &self.nodes[&handle].control else {
            return false;
        };
        let float = float.clone();
        let bounds = self.nodes[&dock].bounds;
        let width = rect.width.clamp(MIN_FLOAT.0, bounds.width.max(MIN_FLOAT.0));
        let height = rect
            .height
            .clamp(MIN_FLOAT.1, bounds.height.max(MIN_FLOAT.1));
        self.queue_update(
            dock,
            ControlUpdate::Dock(DockAction::FloatRect {
                float,
                x: rect.x.clamp(0.0, (bounds.width - 60.0).max(0.0)),
                y: rect.y.clamp(0.0, (bounds.height - 24.0).max(0.0)),
                width,
                height,
            }),
        );
        true
    }

    fn handle_rect(&self, handle: NodeId) -> Option<(Rect, bool)> {
        let Control::DockHandle {
            resize,
            x,
            y,
            width,
            height,
            ..
        } = self.nodes.get(&handle)?.control
        else {
            return None;
        };
        Some((
            Rect {
                x,
                y,
                width,
                height,
            },
            resize,
        ))
    }

    /// Arrow keys move a focused grip or resize from a focused resize handle.
    pub(super) fn dock_handle_key(&mut self, handle: NodeId, key: Key) -> Option<bool> {
        let (mut rect, resize) = self.handle_rect(handle)?;
        let (dx, dy) = match key {
            Key::ArrowLeft => (-10.0, 0.0),
            Key::ArrowRight => (10.0, 0.0),
            Key::ArrowUp => (0.0, -10.0),
            Key::ArrowDown => (0.0, 10.0),
            _ => return None,
        };
        if resize {
            rect.width += dx;
            rect.height += dy;
        } else {
            rect.x += dx;
            rect.y += dy;
        }
        Some(self.float_update(handle, rect))
    }

    pub(super) fn dock_event(&mut self, event: &Event) -> Option<bool> {
        match event {
            Event::FocusLost => {
                self.dock_grab = None;
                if self.dock_drag.take().is_some() {
                    self.pending |= Dirty::PAINT;
                }
                None
            }
            Event::KeyDown {
                key: Key::Escape, ..
            } if self.dock_dragging() => {
                self.dock_drag = None;
                self.pending |= Dirty::PAINT;
                Some(true)
            }
            Event::PointerDown | Event::PointerDownWithShift => {
                let point = self.pointer?;
                let hit = self.hit_test(point)?;
                self.raise_float(hit);
                let node = &self.nodes[&hit];
                if !node.enabled {
                    return None;
                }
                if node.kind == Kind::DockHandle {
                    let (origin, _) = self.handle_rect(hit)?;
                    self.focus_control(hit);
                    self.captured = Some(hit);
                    self.dock_grab = Some(DockGrab {
                        handle: hit,
                        start: point,
                        origin,
                    });
                    return Some(true);
                }
                if let Control::Tab { index, .. } = node.control
                    && let Some(group) = self.ancestor(hit, Kind::DockGroup)
                    && let Some(dock) = self.ancestor(group, Kind::Dock)
                    && let Control::DockGroup { panels, .. } = &self.nodes[&group].control
                    && let Some(panel) = panels.get(index)
                {
                    self.dock_drag = Some(Box::new(DockDrag {
                        dock,
                        panel: panel.clone(),
                        title: node.text.clone(),
                        start: point,
                        active: false,
                        group: None,
                        target: None,
                    }));
                }
                // The press itself still selects or focuses as usual.
                None
            }
            Event::PointerMoved(point) => {
                if let Some(grab) = &self.dock_grab {
                    let (handle, start, mut rect) = (grab.handle, grab.start, grab.origin);
                    let (_, resize) = self.handle_rect(handle)?;
                    let (dx, dy) = (point.x - start.x, point.y - start.y);
                    if resize {
                        rect.width += dx;
                        rect.height += dy;
                    } else {
                        rect.x += dx;
                        rect.y += dy;
                    }
                    self.pointer = Some(*point);
                    self.float_update(handle, rect);
                    return Some(true);
                }
                let drag = self.dock_drag.as_ref()?;
                let (dx, dy) = (point.x - drag.start.x, point.y - drag.start.y);
                if !drag.active && dx * dx + dy * dy < 36.0 {
                    return None;
                }
                let (group, target) = self.dock_target(drag.dock, *point);
                let drag = self.dock_drag.as_mut().unwrap();
                drag.active = true;
                drag.group = group;
                drag.target = target;
                self.pointer = Some(*point);
                self.captured = None;
                self.keyboard_pressed = None;
                self.overlay_damage();
                Some(true)
            }
            Event::PointerUp => {
                if self.dock_grab.take().is_some() {
                    self.captured = None;
                    return Some(true);
                }
                let drag = self.dock_drag.take()?;
                if !drag.active {
                    return None;
                }
                self.captured = None;
                self.overlay_damage();
                if let Some((target, _)) = drag.target {
                    self.queue_update(
                        drag.dock,
                        ControlUpdate::Dock(DockAction::Move {
                            panel: drag.panel,
                            target,
                        }),
                    );
                }
                Some(true)
            }
            _ => None,
        }
    }

    fn overlay_damage(&mut self) {
        let viewport = self.nodes[&self.root].bounds;
        self.add_damage(viewport);
        self.pending |= Dirty::PAINT;
    }

    /// A press inside a floating group that is not frontmost raises it.
    fn raise_float(&mut self, hit: NodeId) {
        let float = if self.nodes[&hit].kind == Kind::DockFloat {
            Some(hit)
        } else {
            self.ancestor(hit, Kind::DockFloat)
        };
        let Some(float) = float else {
            return;
        };
        let Some(dock) = self.nodes[&float].parent else {
            return;
        };
        if self.nodes[&dock].children.last() == Some(&float) {
            return;
        }
        if let WidgetKey::Named(key) = &self.nodes[&float].key
            && let Some(first) = key.strip_prefix("$float:")
        {
            self.queue_update(
                dock,
                ControlUpdate::Dock(DockAction::Raise(first.to_owned())),
            );
            self.pending |= Dirty::PAINT;
        }
    }

    fn dock_target(
        &self,
        dock: NodeId,
        point: Point,
    ) -> (Option<NodeId>, Option<(DockTarget, Rect)>) {
        let Some(node) = self.nodes.get(&dock) else {
            return (None, None);
        };
        let Control::Dock { area, native } = node.control else {
            return (None, None);
        };
        let bounds = node.bounds;
        let window = DockTarget::Window {
            width: WINDOW_SIZE.0,
            height: WINDOW_SIZE.1,
        };
        if !self.nodes[&self.root].bounds.contains(point) {
            return (None, native.then_some((window, Rect::default())));
        }
        // Released over no indicator: float in the main window, or tear off again
        // from a detached one.
        let loose = || {
            if area != DockArea::Main {
                return native.then_some((window.clone(), Rect::default()));
            }
            let x = (point.x - bounds.x - 40.0).clamp(0.0, (bounds.width - FLOAT_SIZE.0).max(0.0));
            let y = (point.y - bounds.y - 10.0).clamp(0.0, (bounds.height - FLOAT_SIZE.1).max(0.0));
            Some((
                DockTarget::Float {
                    x,
                    y,
                    width: FLOAT_SIZE.0,
                    height: FLOAT_SIZE.1,
                },
                Rect {
                    x: bounds.x + x,
                    y: bounds.y + y,
                    width: FLOAT_SIZE.0.min(bounds.width),
                    height: FLOAT_SIZE.1.min(bounds.height),
                },
            ))
        };
        if !bounds.contains(point) {
            return (None, loose());
        }
        let mut order = Vec::new();
        self.order(dock, &mut order);
        let group = order.into_iter().rev().find(|id| {
            let node = &self.nodes[id];
            node.kind == Kind::DockGroup && node.clip.contains(point)
        });
        if let Some((zone, _)) = edges(bounds)
            .into_iter()
            .find(|(_, rect)| rect.contains(point))
        {
            return (
                group,
                Some((DockTarget::Edge { area, zone }, side(bounds, zone, 0.25))),
            );
        }
        let Some(group) = group else {
            let inside = DockTarget::Edge {
                area,
                zone: DockZone::Center,
            };
            return (None, Some((inside, bounds)));
        };
        let node = &self.nodes[&group];
        let Control::DockGroup { panels, float } = &node.control else {
            return (None, None);
        };
        let Some(anchor) = panels.first().cloned() else {
            return (None, None);
        };
        let strip = node
            .children
            .first()
            .and_then(|tabs| {
                self.nodes[tabs]
                    .children_by_key
                    .get(&WidgetKey::Named("$strip".into()))
            })
            .map(|strip| &self.nodes[strip]);
        if let Some(strip) = strip.filter(|strip| strip.bounds.contains(point)) {
            let tabs: Vec<Rect> = strip
                .children
                .iter()
                .filter(|id| self.nodes[id].kind == Kind::Tab)
                .map(|id| self.nodes[id].bounds)
                .collect();
            let index = tabs
                .iter()
                .filter(|tab| tab.x + tab.width / 2.0 < point.x)
                .count();
            let x = tabs.get(index).map_or_else(
                || tabs.last().map_or(strip.bounds.x, |tab| tab.x + tab.width),
                |tab| tab.x,
            );
            let bar = Rect {
                x: x - 2.0,
                y: strip.bounds.y,
                width: 4.0,
                height: strip.bounds.height,
            };
            return (Some(group), Some((DockTarget::Tab { anchor, index }, bar)));
        }
        let target = compass(node.bounds, *float)
            .into_iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(zone, _)| {
                (
                    DockTarget::Group { anchor, zone },
                    side(node.bounds, zone, 0.5),
                )
            })
            .or_else(loose);
        (Some(group), target)
    }

    /// Drop preview, indicators and the dragged tab's label. Painted above content.
    pub(super) fn paint_dock_drag(&mut self, scene: &mut Scene, measurer: &mut impl TextMeasurer) {
        let Some(drag) = self.dock_drag.as_ref().filter(|drag| drag.active) else {
            return;
        };
        // Nothing here is a node, so every rectangle names itself as damage.
        let mut painted = Vec::new();
        let theme = &self.theme;
        let accent = theme.accent;
        if let Some((_, preview)) = &drag.target
            && preview.width > 0.0
        {
            painted.push(*preview);
            scene.primitives.push(Primitive::Quad {
                rect: *preview,
                color: Color(accent.0, accent.1, accent.2, 70),
                radius: 6.0,
            });
        }
        let mut indicators = Vec::new();
        if let Some(dock) = self.nodes.get(&drag.dock) {
            indicators.extend(edges(dock.bounds));
        }
        if let Some(group) = drag.group.and_then(|id| self.nodes.get(&id)) {
            let float = matches!(group.control, Control::DockGroup { float: true, .. });
            indicators.extend(compass(group.bounds, float));
        }
        let pointer = self.pointer.unwrap_or_default();
        for (zone, rect) in indicators {
            painted.push(rect);
            scene.primitives.push(Primitive::Quad {
                rect,
                color: theme.popup,
                radius: 6.0,
            });
            let inner = Rect {
                x: rect.x + 6.0,
                y: rect.y + 6.0,
                width: rect.width - 12.0,
                height: rect.height - 12.0,
            };
            scene.primitives.push(Primitive::Quad {
                rect: side(inner, zone, 0.5),
                color: if rect.contains(pointer) {
                    accent
                } else {
                    theme.text_muted
                },
                radius: 2.0,
            });
        }
        let style = TextStyle {
            font_size: 13.0,
            family: theme.font_family.clone(),
            ..TextStyle::default()
        };
        let measured = measurer.measure_styled(&drag.title, &style, None);
        let rect = Rect {
            x: pointer.x + 14.0,
            y: pointer.y + 14.0,
            width: measured.width + 16.0,
            height: measured.height + 12.0,
        };
        painted.push(rect);
        scene.primitives.push(Primitive::Quad {
            rect,
            color: theme.popup,
            radius: 6.0,
        });
        scene.primitives.push(Primitive::StyledText {
            rect: Rect {
                x: rect.x + 8.0,
                y: rect.y + 6.0,
                width: measured.width,
                height: measured.height,
            },
            text: drag.title.clone(),
            style,
            color: theme.text,
        });
        for rect in painted {
            self.painted_overlay(rect);
        }
    }
}
