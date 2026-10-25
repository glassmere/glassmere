//! Scrolling an editor while a selection drag is held outside its viewport.

use super::*;

/// How often a host is asked to run a context that is scrolling by the clock.
const TICK: Duration = Duration::from_millis(16);
/// Speed, in pixels per second, just past the edge, and what each pixel
/// further out adds, up to the fastest.
const BASE_SPEED: f32 = 120.0;
const SPEED_PER_PIXEL: f32 = 10.0;
const TOP_SPEED: f32 = 1500.0;
/// A frame that comes late scrolls as far as this much time, not further.
const LONGEST_STEP: Duration = Duration::from_millis(100);

impl Context {
    /// The part of an editor's box that shows text: the content box, less the
    /// gutter of a code editor. Rich editors wrap, so they only scroll up
    /// and down.
    fn editor_viewport(&self, id: NodeId) -> Option<Rect> {
        let node = self.nodes.get(&id)?;
        let content = node.content_box();
        match node.kind {
            Kind::TextArea => {
                let gutter = node.area.as_ref()?.gutter.min(content.width);
                Some(Rect {
                    x: content.x + gutter,
                    width: content.width - gutter,
                    ..content
                })
            }
            Kind::RichEditor => Some(content),
            _ => None,
        }
    }

    /// `point` moved inside the viewport of the editor `id`.
    pub(super) fn clamp_to_viewport(&self, id: NodeId, point: Point) -> Option<Point> {
        let view = self.editor_viewport(id)?;
        Some(Point {
            x: point
                .x
                .clamp(view.x, (view.x + view.width - 1.0).max(view.x)),
            y: point
                .y
                .clamp(view.y, (view.y + view.height - 1.0).max(view.y)),
        })
    }

    /// The editor a held pointer is selecting in, and how far the pointer is
    /// past its viewport on each axis (negative above and left of it), while
    /// the editor can still scroll that way.
    fn edge_drag(&self) -> Option<(NodeId, Point)> {
        let id = self.captured?;
        let pointer = self.pointer?;
        let view = self.editor_viewport(id)?;
        let past = |value: f32, start: f32, length: f32| {
            if value < start {
                value - start
            } else if value > start + length {
                value - start - length
            } else {
                0.0
            }
        };
        let scroll = self.nodes[&id].scroll;
        // Scrolling that has reached the end of the text goes nowhere.
        let room = |distance: f32, offset: f32, maximum: f32| {
            if (distance < 0.0 && offset > 0.0) || (distance > 0.0 && offset < maximum) {
                distance
            } else {
                0.0
            }
        };
        let over = Point {
            x: room(
                past(pointer.x, view.x, view.width),
                scroll.offset_x,
                scroll.maximum_x,
            ),
            y: room(
                past(pointer.y, view.y, view.height),
                scroll.offset,
                scroll.maximum,
            ),
        };
        (over.x != 0.0 || over.y != 0.0).then_some((id, over))
    }

    /// When the scrolling of a drag held outside an editor is next due.
    pub(super) fn edge_scroll_wake(&self) -> Option<Duration> {
        self.edge_drag()?;
        Some(self.edge_scroll_at.unwrap_or(self.now) + TICK)
    }

    /// Scrolls the editor under a held drag toward the pointer by the time
    /// since it last did, faster the further out the pointer is, and extends
    /// the selection to the text that comes into view.
    pub(crate) fn update_edge_scroll(&mut self) {
        let Some((id, over)) = self.edge_drag() else {
            self.edge_scroll_at = None;
            return;
        };
        let last = *self.edge_scroll_at.get_or_insert(self.now);
        let seconds = self
            .now
            .saturating_sub(last)
            .min(LONGEST_STEP)
            .as_secs_f32();
        if seconds <= 0.0 {
            return;
        }
        self.edge_scroll_at = Some(self.now);
        let step = |distance: f32| {
            let speed = (BASE_SPEED + SPEED_PER_PIXEL * distance.abs()).min(TOP_SPEED);
            distance.signum() * speed * seconds
        };
        let offset = self.nodes[&id].scroll.offset;
        let offset_x = self.nodes[&id].scroll.offset_x;
        let vertical = over.y != 0.0 && self.set_scroll(id, offset + step(over.y));
        let horizontal = over.x != 0.0 && self.set_scroll_x(id, offset_x + step(over.x));
        if !(vertical || horizontal) {
            return;
        }
        let Some(pointer) = self.pointer else {
            return;
        };
        if self.nodes[&id].kind == Kind::TextArea {
            self.area_drag(id, pointer);
        } else {
            self.rich_drag(id, pointer);
        }
    }
}
