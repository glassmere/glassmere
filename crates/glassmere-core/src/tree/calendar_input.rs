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
        // The calendar of a date picker leaves the pick to the field, which
        // closes it.
        if let Some(field) = self.calendar_field(grid) {
            self.queue_update(field, ControlUpdate::Date(date));
            self.close_popup();
            return true;
        }
        self.calendar_move(grid, date);
        self.queue_update(grid, ControlUpdate::Date(date));
        true
    }

    /// The date picker whose popup holds the calendar `grid`.
    fn calendar_field(&self, grid: NodeId) -> Option<NodeId> {
        let node = self.nodes.get(&grid)?;
        let Control::CalendarGrid { popup: true, .. } = node.control else {
            return None;
        };
        self.nodes.get(&node.parent?)?.parent
    }

    /// The calendar in the popup of the date picker `field`, while it is open.
    pub(super) fn date_picker_grid(&self, field: NodeId) -> Option<NodeId> {
        self.popup_of(field)?;
        let panel = self
            .nodes
            .get(&field)?
            .children
            .iter()
            .copied()
            .find(|child| self.nodes[child].kind == Kind::Popup)?;
        self.nodes
            .get(&panel)?
            .children_by_key
            .get(&WidgetKey::named("$grid"))
            .copied()
    }

    /// The day the keys of the open date picker `field` act on.
    pub(super) fn date_picker_cursor(&self, field: NodeId) -> Option<NodeId> {
        self.calendar_cursor_cell(self.date_picker_grid(field)?)
    }

    /// Where a press on `id` leaves the keyboard when `id` is part of a
    /// calendar: with the date picker whose popup it is in, or else with the
    /// calendar of the day or button.
    pub(super) fn calendar_focus_of(&self, id: NodeId) -> Option<NodeId> {
        let mut at = self.nodes.get(&id);
        while let Some(node) = at {
            if matches!(node.control, Control::CalendarPopup { .. }) {
                return node.parent;
            }
            at = node.parent.and_then(|parent| self.nodes.get(&parent));
        }
        self.calendar_grid_of(id)
    }

    /// A key pressed while the popup of the date picker `field` is open.
    /// `None` for one that is not the popup's.
    pub(super) fn date_picker_key(&mut self, field: NodeId, key: Key) -> Option<bool> {
        match key {
            Key::Escape => Some(self.close_popup()),
            Key::Tab => {
                self.close_popup();
                None
            }
            _ => {
                let grid = self.date_picker_grid(field)?;
                self.calendar_key(grid, key)
            }
        }
    }

    /// Alt+Down opens the calendar of the focused date picker.
    pub(super) fn date_picker_shortcut(&mut self, shortcut: &crate::Shortcut) -> bool {
        let alt_down = crate::Shortcut {
            key: crate::ShortcutKey::Named(Key::ArrowDown),
            control: false,
            alt: true,
            shift: false,
            super_key: false,
        };
        let Some(field) = self
            .focused
            .filter(|id| matches!(self.nodes[id].control, Control::DatePicker { .. }))
        else {
            return false;
        };
        *shortcut == alt_down && self.popup_of(field).is_none() && self.open_popup_at(field, None)
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
        let keys = match self.calendar_field(id) {
            Some(field) => self.focused == Some(field) && self.popup_of(field).is_some(),
            None => self.focused == Some(id),
        };
        if !keys || self.quiet_focus == Some(id) {
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
