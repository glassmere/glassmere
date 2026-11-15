//! A submenu follows the pointer by the clock. It opens once the pointer has
//! rested on its row for a moment, and the one that is open stays while the
//! pointer crosses other rows on the way to it: a row only takes the menu
//! from the submenu after the pointer has rested on it just as long.

use super::*;

/// How long the pointer rests on a row of an open menu before the row takes
/// effect: it opens a submenu, or closes the one open beside its menu.
const SUBMENU_DELAY: Duration = Duration::from_millis(200);

/// The row of an open menu the pointer has been on since a time.
#[derive(Clone, Copy)]
pub(super) struct MenuDwell {
    row: NodeId,
    since: Duration,
}

impl Context {
    /// The pointer moved onto `row`, row `index` of the open menu `owner`
    /// owns. A menu with nothing open beside it highlights the row at once;
    /// one with a submenu open keeps it until the pointer has rested here.
    pub(super) fn hover_menu_row(&mut self, row: NodeId, owner: NodeId, index: usize) {
        let innermost = self.popup.is_some_and(|popup| popup.owner == owner);
        let opens = self.opens_submenu(row);
        if innermost && let Some(popup) = &mut self.popup {
            popup.highlighted = index;
            popup.reveal_highlight = false;
        }
        if !innermost || opens {
            // The wait starts when the pointer came onto the row, not over.
            if self.menu_dwell.is_none_or(|dwell| dwell.row != row) {
                self.menu_dwell = Some(MenuDwell {
                    row,
                    since: self.now,
                });
            }
        } else {
            self.menu_dwell = None;
        }
    }

    /// Whether choosing `row` opens a submenu.
    fn opens_submenu(&self, row: NodeId) -> bool {
        self.is_submenu(row) && self.nodes.get(&row).is_some_and(|node| node.enabled)
    }

    /// Before a batch decides whether to declare: the row the pointer has
    /// rested on takes effect.
    pub(super) fn update_menu_dwell(&mut self) {
        let Some(dwell) = self.menu_dwell else {
            return;
        };
        if self.now < dwell.since + SUBMENU_DELAY {
            return;
        }
        self.menu_dwell = None;
        let Some((owner, index)) = self.row_place(dwell.row) else {
            return;
        };
        if self.popup_of(owner).is_none() {
            return;
        }
        self.close_levels_below(owner);
        if let Some(popup) = &mut self.popup {
            popup.highlighted = index;
            popup.reveal_highlight = false;
        }
        if self.opens_submenu(dwell.row) {
            self.open_popup(dwell.row);
        }
        self.invalidate_paint(owner);
        self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        self.view_stale = true;
    }

    /// When the row the pointer is resting on is due to take effect.
    pub(super) fn menu_dwell_wake(&self) -> Option<Duration> {
        self.menu_dwell.map(|dwell| dwell.since + SUBMENU_DELAY)
    }
}
