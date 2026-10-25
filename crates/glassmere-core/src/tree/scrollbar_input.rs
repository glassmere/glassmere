//! Dragging a scrollbar's thumb, and clicking its track to page.

use super::*;

/// How far in from a scrolling node's edge a press takes its scrollbar. The
/// bar is drawn thinner; this is the part a pointer can reliably hit.
const GRIP: f32 = 12.0;
const THIN: f32 = 4.0;
/// While hovered or held, so it is seen to be what the pointer has.
const WIDE: f32 = 8.0;
/// How long a press on the track rests before it pages again, and then how
/// often it does.
const REPEAT_DELAY: Duration = Duration::from_millis(400);
const REPEAT_INTERVAL: Duration = Duration::from_millis(50);

/// A thumb being dragged: which node, on which axis, and where in the thumb
/// it was taken, along the axis. For a grid's bar, the node is the track and
/// `within` is where the pointer last was.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ScrollbarGrab {
    pub node: NodeId,
    pub vertical: bool,
    pub within: f32,
}

/// A press held on a track: which node pages, on which axis, toward which
/// end, and when it pages next. For a grid's bar, the node is the track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TrackHold {
    node: NodeId,
    vertical: bool,
    toward_end: bool,
    next: Duration,
}

/// One axis of a node's scrollbar, in window coordinates.
pub(super) struct Bar {
    pub vertical: bool,
    /// The strip along the edge that takes a press.
    pub grip: Rect,
    /// Where the thumb can be along the axis: its start, and its length.
    pub track: (f32, f32),
    pub thumb: Rect,
}

impl Bar {
    fn along(&self, point: Point) -> f32 {
        if self.vertical { point.y } else { point.x }
    }

    fn thumb_start(&self) -> f32 {
        if self.vertical {
            self.thumb.y
        } else {
            self.thumb.x
        }
    }

    fn thumb_length(&self) -> f32 {
        if self.vertical {
            self.thumb.height
        } else {
            self.thumb.width
        }
    }
}

impl Context {
    /// The scrollbars `id` shows, drawn `width` thick: sideways, then up and down.
    pub(super) fn bars(&self, id: NodeId, width: f32) -> [Option<Bar>; 2] {
        let node = &self.nodes[&id];
        let bounds = node.bounds;
        let rtl = node.resolved.rtl();
        let mut bars = [None, None];
        if node.scroll.maximum_x > 0.0 {
            let track = bounds.width;
            let length = (track * track / (track + node.scroll.maximum_x))
                .max(12.0)
                .min(track);
            let travelled = (track - length) * node.scroll.offset_x / node.scroll.maximum_x;
            bars[0] = Some(Bar {
                vertical: false,
                grip: Rect {
                    y: bounds.y + bounds.height - GRIP,
                    height: GRIP,
                    ..bounds
                },
                track: (bounds.x, track),
                thumb: Rect {
                    // The thumb starts at the start side, like the content.
                    x: if node.scrolls_x_from_end() {
                        bounds.x + track - length - travelled
                    } else {
                        bounds.x + travelled
                    },
                    y: bounds.y + bounds.height - width,
                    width: length,
                    height: width,
                },
            });
        }
        if node.scroll.maximum > 0.0 {
            let track = bounds.height;
            let length = (track * track / (track + node.scroll.maximum))
                .max(12.0)
                .min(track);
            let x = |thick: f32| {
                if rtl {
                    bounds.x
                } else {
                    bounds.x + bounds.width - thick
                }
            };
            // The corner belongs to the vertical bar.
            if let Some(across) = &mut bars[0] {
                across.grip.width -= GRIP;
                if rtl {
                    across.grip.x += GRIP;
                }
            }
            bars[1] = Some(Bar {
                vertical: true,
                grip: Rect {
                    x: x(GRIP),
                    width: GRIP,
                    ..bounds
                },
                track: (bounds.y, track),
                thumb: Rect {
                    x: x(width),
                    y: bounds.y + (track - length) * node.scroll.offset / node.scroll.maximum,
                    width,
                    height: length,
                },
            });
        }
        bars
    }

    /// How thick `id`'s bars are drawn: wide while the pointer is on one.
    pub(super) fn bar_width(&self, id: NodeId) -> f32 {
        let held = self.scrollbar_grab.is_some_and(|grab| grab.node == id);
        if held || self.bar_hovered == Some(id) {
            WIDE
        } else {
            THIN
        }
    }

    /// The innermost scrolling node whose bar is under `point`.
    fn bar_under(&self, point: Point) -> Option<(NodeId, Bar)> {
        let mut at = self.hit_test(point);
        while let Some(id) = at {
            let node = &self.nodes[&id];
            if node.clip.contains(point)
                && let Some(bar) = self
                    .bars(id, THIN)
                    .into_iter()
                    .flatten()
                    .find(|bar| bar.grip.contains(point))
            {
                return Some((id, bar));
            }
            at = node.parent;
        }
        None
    }

    /// The rows of the grid whose scrollbar track `track` is.
    fn grid_of_track(&self, track: NodeId) -> Option<NodeId> {
        let bar = self.nodes.get(&track)?.parent?;
        let grid = self.nodes.get(&bar)?.parent?;
        self.nodes[&grid].children.iter().copied().find(|id| {
            matches!(
                self.nodes[id].control,
                Control::Collection {
                    kind: crate::collection::CollectionKind::Grid,
                    ..
                }
            )
        })
    }

    /// A press on a grid's track: the thumb is taken, anywhere else pages.
    fn grid_bar_press(&mut self, point: Point) -> Option<bool> {
        let track = self
            .hit_test(point)
            .filter(|id| matches!(self.nodes[id].control, Control::GridScrollbar { .. }))?;
        self.grid_of_track(track)?;
        let node = &self.nodes[&track];
        let thumb = self.nodes[node.children.first()?].bounds;
        if (thumb.x..=thumb.x + thumb.width).contains(&point.x) {
            self.scrollbar_grab = Some(ScrollbarGrab {
                node: track,
                vertical: false,
                within: point.x,
            });
            return Some(true);
        }
        Some(self.press_track(track, false, point.x > thumb.x))
    }

    /// Pages `node` toward an end, a view at a time. For a grid's bar, `node`
    /// is the track and pages the columns.
    fn page_track(&mut self, node: NodeId, vertical: bool, toward_end: bool) -> bool {
        let sign = if toward_end { 1.0 } else { -1.0 };
        let bounds = self.nodes[&node].bounds;
        if matches!(self.nodes[&node].control, Control::GridScrollbar { .. }) {
            let Some(rows) = self.grid_of_track(node) else {
                return false;
            };
            return self.grid_scroll_x(rows, sign * bounds.width);
        }
        let scroll = self.nodes[&node].scroll;
        if vertical {
            self.set_scroll(node, scroll.offset + sign * bounds.height)
        } else {
            self.set_scroll_x(node, scroll.offset_x + sign * bounds.width)
        }
    }

    /// A press on a track: it pages once, and again while it is held.
    fn press_track(&mut self, node: NodeId, vertical: bool, toward_end: bool) -> bool {
        self.track_hold = Some(TrackHold {
            node,
            vertical,
            toward_end,
            next: self.now + REPEAT_DELAY,
        });
        self.page_track(node, vertical, toward_end)
    }

    /// Which way the held press would page from `point`, or `None` where it
    /// would not: off its track, or on the thumb, which has reached it.
    fn hold_direction(&self, hold: &TrackHold, point: Point) -> Option<bool> {
        let node = self.nodes.get(&hold.node)?;
        if matches!(node.control, Control::GridScrollbar { .. }) {
            let thumb = self.nodes[node.children.first()?].bounds;
            return (node.bounds.contains(point)
                && !(thumb.x..=thumb.x + thumb.width).contains(&point.x))
            .then_some(point.x > thumb.x);
        }
        let bar = self
            .bars(hold.node, THIN)
            .into_iter()
            .flatten()
            .find(|bar| bar.vertical == hold.vertical)?;
        let along = bar.along(point);
        let start = bar.thumb_start();
        (bar.grip.contains(point) && !(start..=start + bar.thumb_length()).contains(&along))
            .then_some((along > start) != (!bar.vertical && node.scrolls_x_from_end()))
    }

    /// Pages again where a held press is due, until the thumb reaches the
    /// pointer, which lets go or leaves the track.
    pub(crate) fn update_track_hold(&mut self) {
        let Some(hold) = self.track_hold else {
            return;
        };
        if self.now < hold.next {
            return;
        }
        let direction = self
            .pointer
            .and_then(|point| self.hold_direction(&hold, point));
        if direction != Some(hold.toward_end)
            || !self.page_track(hold.node, hold.vertical, hold.toward_end)
        {
            self.track_hold = None;
            return;
        }
        self.track_hold = Some(TrackHold {
            next: self.now + REPEAT_INTERVAL,
            ..hold
        });
        self.view_stale = true;
    }

    pub(super) fn track_wake(&self) -> Option<Duration> {
        self.track_hold.map(|hold| hold.next)
    }

    /// Takes a press on a scrollbar, the moves while it is held, and its
    /// release. `None` leaves the event to everything else.
    pub(super) fn scrollbar_event(&mut self, event: &Event) -> Option<bool> {
        match event {
            Event::PointerDown | Event::PointerDownWithShift => {
                let point = self.pointer?;
                let Some((id, bar)) = self.bar_under(point) else {
                    return self.grid_bar_press(point);
                };
                let along = bar.along(point);
                let start = bar.thumb_start();
                if (start..=start + bar.thumb_length()).contains(&along) {
                    self.scrollbar_grab = Some(ScrollbarGrab {
                        node: id,
                        vertical: bar.vertical,
                        within: along - start,
                    });
                    self.invalidate_paint(id);
                    return Some(true);
                }
                // The track pages toward the press, a view at a time.
                let toward_end =
                    (along > start) != (!bar.vertical && self.nodes[&id].scrolls_x_from_end());
                Some(self.press_track(id, bar.vertical, toward_end))
            }
            Event::PointerMoved(point) => {
                if let Some(hold) = self.track_hold
                    && self.hold_direction(&hold, *point).is_none()
                {
                    self.track_hold = None;
                }
                let over = self.bar_under(*point).map(|(id, _)| id);
                if over != self.bar_hovered {
                    for id in [self.bar_hovered, over].into_iter().flatten() {
                        self.invalidate_paint(id);
                    }
                    self.bar_hovered = over;
                }
                let grab = self.scrollbar_grab?;
                self.pointer = Some(*point);
                if !self.nodes.contains_key(&grab.node) {
                    self.scrollbar_grab = None;
                    return Some(false);
                }
                if let Control::GridScrollbar { ratio } = self.nodes[&grab.node].control {
                    let rows = self.grid_of_track(grab.node)?;
                    let moved = point.x - grab.within;
                    self.scrollbar_grab = Some(ScrollbarGrab {
                        within: point.x,
                        ..grab
                    });
                    return Some(self.grid_scroll_x(rows, moved * ratio));
                }
                let bar = self
                    .bars(grab.node, WIDE)
                    .into_iter()
                    .flatten()
                    .find(|bar| bar.vertical == grab.vertical)?;
                let (track_start, track) = bar.track;
                let room = track - bar.thumb_length();
                if room <= 0.0 {
                    return Some(false);
                }
                let fraction =
                    ((bar.along(*point) - grab.within - track_start) / room).clamp(0.0, 1.0);
                let node = &self.nodes[&grab.node];
                Some(if grab.vertical {
                    self.set_scroll(grab.node, fraction * node.scroll.maximum)
                } else {
                    let fraction = if node.scrolls_x_from_end() {
                        1.0 - fraction
                    } else {
                        fraction
                    };
                    self.set_scroll_x(grab.node, fraction * node.scroll.maximum_x)
                })
            }
            Event::PointerUp => {
                self.track_hold = None;
                let grab = self.scrollbar_grab.take()?;
                self.invalidate_paint(grab.node);
                Some(true)
            }
            Event::PointerLeft | Event::FocusLost => {
                self.track_hold = None;
                for id in [
                    self.bar_hovered.take(),
                    self.scrollbar_grab.take().map(|grab| grab.node),
                ]
                .into_iter()
                .flatten()
                {
                    self.invalidate_paint(id);
                }
                None
            }
            _ => None,
        }
    }
}
