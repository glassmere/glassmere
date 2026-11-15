//! What the keys, the buttons and the days of a calendar do. The month on
//! show is kept here, beside the tree, because a declaration is made again
//! from the application's state and the calendar's own is not in it.

use super::*;
use crate::calendar::{Month, View};
use glassmere_scene::{Path, Stroke};

impl Context {
    pub(crate) fn calendar_view(&self, grid: NodeId) -> Option<View> {
        self.calendar_views.get(&grid).copied()
    }

    /// The month `grid` shows now: as it was declared, moved by what has
    /// reached it since.
    fn calendar_month(&self, grid: NodeId) -> Option<Month> {
        let Control::CalendarGrid { month, .. } = &self.nodes.get(&grid)?.control else {
            return None;
        };
        Some(Month::new(
            self.calendar_view(grid),
            month.selected,
            month.min,
            month.max,
            month.first,
        ))
    }

    /// The grid of the day or the button `id`.
    pub(super) fn calendar_grid_of(&self, id: NodeId) -> Option<NodeId> {
        let node = self.nodes.get(&id)?;
        let container = match node.control {
            // A day is in a week, which is in the grid.
            Control::CalendarDay { .. } => return self.nodes.get(&node.parent?)?.parent,
            // A button is in the header, beside the grid.
            Control::CalendarStep { .. } => self.nodes.get(&node.parent?)?.parent?,
            _ => return None,
        };
        self.nodes
            .get(&container)?
            .children_by_key
            .get(&WidgetKey::named("$grid"))
            .copied()
    }

    /// The day the keys of `grid` act on.
    pub(super) fn calendar_cursor_cell(&self, grid: NodeId) -> Option<NodeId> {
        let node = self.nodes.get(&grid)?;
        let Control::CalendarGrid { month, .. } = &node.control else {
            return None;
        };
        let (row, column) = month.position(month.cursor)?;
        // The weekday names are the first row.
        let week = *node.children.get(row + 1)?;
        self.nodes.get(&week)?.children.get(column).copied()
    }

    /// Moves the cursor of `grid`, and with it the month on show.
    fn calendar_move(&mut self, grid: NodeId, to: crate::Date) -> bool {
        let Some(month) = self.calendar_month(grid) else {
            return false;
        };
        self.calendar_views.insert(grid, month.at(to).view());
        self.invalidate_paint(grid);
        true
    }

    /// Picks `date`: the cursor goes to it, and the application is told.
    pub(super) fn calendar_pick(&mut self, grid: NodeId, date: crate::Date) -> bool {
        if self
            .calendar_month(grid)
            .is_none_or(|month| !month.enabled(date))
        {
            return false;
        }
        self.calendar_move(grid, date);
        self.queue_update(grid, ControlUpdate::Date(date));
        true
    }

    /// Shows the month `months` away.
    pub(super) fn calendar_step(&mut self, grid: NodeId, months: i32) -> bool {
        let Some(month) = self
            .calendar_month(grid)
            .filter(|month| month.can_step(months))
        else {
            return false;
        };
        self.calendar_move(grid, month.stepped(months))
    }

    /// A key pressed while the calendar `grid` has the keyboard. `None` for
    /// one that is not the calendar's.
    pub(super) fn calendar_key(&mut self, grid: NodeId, key: Key) -> Option<bool> {
        let month = self.calendar_month(grid)?;
        if matches!(key, Key::Enter | Key::Space) {
            return Some(self.calendar_pick(grid, month.cursor));
        }
        let rtl = self.nodes[&grid].resolved.rtl();
        let to = month.moved(key, rtl)?;
        Some(self.calendar_move(grid, to))
    }

    /// A ring around the day the keys act on, while the calendar has them.
    pub(super) fn paint_calendar_cursor(&self, id: NodeId, scene: &mut Scene) {
        if self.focused != Some(id) || self.quiet_focus == Some(id) {
            return;
        }
        let Some(cell) = self.calendar_cursor_cell(id).map(|cell| &self.nodes[&cell]) else {
            return;
        };
        const WIDTH: f32 = 2.0;
        // The stroke is centred on the path, so the path is half of it inside.
        let Rect {
            x,
            y,
            width,
            height,
        } = cell.bounds;
        let (left, top) = (x + WIDTH / 2.0, y + WIDTH / 2.0);
        let (right, bottom) = (x + width - WIDTH / 2.0, y + height - WIDTH / 2.0);
        let radius = (cell.style.radius - WIDTH / 2.0)
            .max(0.0)
            .min((right - left) / 2.0)
            .min((bottom - top) / 2.0);
        let at = |x, y| Point { x, y };
        let mut outline = Path::new();
        outline
            .move_to(at(left + radius, top))
            .line_to(at(right - radius, top))
            .quad_to(at(right, top), at(right, top + radius))
            .line_to(at(right, bottom - radius))
            .quad_to(at(right, bottom), at(right - radius, bottom))
            .line_to(at(left + radius, bottom))
            .quad_to(at(left, bottom), at(left, bottom - radius))
            .line_to(at(left, top + radius))
            .quad_to(at(left, top), at(left + radius, top))
            .close();
        scene.primitives.push(Primitive::Path {
            path: outline,
            fill: None,
            stroke: Some(Stroke {
                width: WIDTH,
                color: self.theme.focus,
            }),
        });
    }
}
