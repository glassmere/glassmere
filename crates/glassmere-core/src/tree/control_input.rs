use super::*;
use crate::{TextMove, controls::slider_value};

/// What a text field holds between declarations.
pub(crate) struct FieldState {
    pub text: String,
    /// The text is the user's typing rather than what the field wrote itself.
    pub draft: bool,
    pub number: Option<NumericValue>,
    pub focused: bool,
    /// A password field showing its text.
    pub revealed: bool,
}

impl Context {
    pub(super) fn cancel_compositions_except(&mut self, keep: Option<NodeId>) {
        let mut changed = Vec::new();
        for (id, node) in &mut self.nodes {
            if Some(*id) != keep
                && let Some(editor) = &mut node.editor
                && editor.apply(TextEdit::CancelComposition)
            {
                changed.push(*id);
            }
        }
        for id in changed {
            self.invalidate_paint(id);
        }
    }

    pub(super) fn edit(&mut self, id: NodeId, edit: TextEdit) -> bool {
        let Some(node) = self.nodes.get_mut(&id).filter(|node| node.enabled) else {
            return false;
        };
        let Some(editor) = &mut node.editor else {
            return false;
        };
        editor.visual = self.caret_movement == crate::CaretMovement::Visual;
        let old_text = editor.text.clone();
        if !editor.apply(edit) {
            return false;
        }
        if old_text != editor.text {
            let text = editor.text.clone();
            // Typing offers suggestions; nothing is highlighted, so Enter
            // still submits what was typed.
            let suggests = matches!(
                node.control,
                Control::TextInput {
                    suggestions: Some(_),
                    ..
                }
            );
            self.queue_update(id, ControlUpdate::Text(text));
            if suggests {
                self.popup_parents.clear();
                self.popup = Some(PopupState {
                    owner: id,
                    highlighted: usize::MAX,
                    reveal_highlight: false,
                    anchor: None,
                    return_focus: None,
                });
            }
        }
        self.pending |= Dirty::ACCESSIBILITY;
        self.invalidate_paint(id);
        true
    }

    fn set_number(&mut self, id: NodeId, number: f64) -> bool {
        let Some(node) = self.nodes.get_mut(&id).filter(|node| node.enabled) else {
            return false;
        };
        let Control::Slider {
            value,
            min,
            max,
            step,
            ..
        } = &mut node.control
        else {
            return false;
        };
        if !number.is_finite() {
            return false;
        }
        let next = slider_value(number, *min, *max, *step);
        if *value == next {
            return false;
        }
        *value = next;
        self.queue_update(id, ControlUpdate::Number(next));
        self.pending |= Dirty::ACCESSIBILITY;
        self.invalidate_paint(id);
        true
    }

    fn slider_pointer(&mut self, id: NodeId, position: Point) -> bool {
        let node = &self.nodes[&id];
        let Control::Slider { min, max, .. } = node.control else {
            return false;
        };
        let fraction = ((position.x - node.bounds.x - 12.0) / (node.bounds.width - 24.0).max(1.0))
            .clamp(0.0, 1.0);
        // A right-to-left slider has its minimum on the right.
        let fraction = if node.resolved.rtl() {
            1.0 - fraction
        } else {
            fraction
        };
        self.set_number(id, min + f64::from(fraction) * (max - min))
    }

    pub(super) fn editor_pointer(&mut self, id: NodeId, position: Point, extend: bool) -> bool {
        let node = &self.nodes[&id];
        let Some(editor) = &node.editor else {
            return false;
        };
        let local = Point {
            x: position.x - node.bounds.x - node.style.padding + editor.scroll_x - editor.align_x,
            y: position.y - node.bounds.y - node.style.padding,
        };
        let (cursor, affinity) = editor.geometry.hit_affinity(local);
        let cursor = editor.display_offset(cursor);
        let anchor = if extend {
            editor.selection.anchor
        } else {
            cursor
        };
        let changed = self.edit(id, TextEdit::Select { anchor, cursor });
        // The caret shows on the side of the character that was clicked.
        if let Some(editor) = self
            .nodes
            .get_mut(&id)
            .and_then(|node| node.editor.as_mut())
            && editor.affinity != affinity
        {
            editor.affinity = affinity;
            self.invalidate_paint(id);
            return true;
        }
        changed
    }

    pub(super) fn focus_control(&mut self, id: NodeId) {
        if let Some(old) = self.focused {
            self.invalidate_paint(old);
        }
        self.focused = Some(id);
        self.keyboard_pressed = None;
        self.cancel_compositions_except(Some(id));
        self.scroll_into_view(id);
        self.pending |= Dirty::ACCESSIBILITY;
        self.invalidate_paint(id);
    }

    fn popup_items(&self, owner: NodeId) -> Option<&[crate::MenuItem]> {
        match &self.nodes.get(&owner)?.control {
            Control::Combo { items, .. }
            | Control::Menu { items }
            | Control::Submenu { items, .. }
            | Control::ContextMenu { items }
            | Control::TextInput {
                suggestions: Some(items),
                ..
            } => Some(items),
            _ => None,
        }
    }

    /// The open menus, outermost first.
    pub(super) fn popup_levels(&self) -> impl Iterator<Item = PopupState> + '_ {
        self.popup_parents.iter().copied().chain(self.popup)
    }

    /// The open popup `owner` owns, at whatever depth.
    pub(crate) fn popup_of(&self, owner: NodeId) -> Option<PopupState> {
        self.popup_levels().find(|popup| popup.owner == owner)
    }

    /// The panel of each open menu, outermost first.
    pub(super) fn popup_panels(&self) -> Vec<NodeId> {
        self.popup_levels()
            .filter_map(|popup| {
                self.nodes
                    .get(&popup.owner)?
                    .children
                    .iter()
                    .copied()
                    .find(|child| self.nodes[child].kind == Kind::Popup)
            })
            .collect()
    }

    /// The owner of the popup `row` is a row of, and its place among the rows.
    pub(super) fn row_place(&self, row: NodeId) -> Option<(NodeId, usize)> {
        let node = self.nodes.get(&row)?;
        if node.kind != Kind::Choice {
            return None;
        }
        let panel = self.nodes.get(&node.parent?)?;
        if panel.kind != Kind::Popup {
            return None;
        }
        let index = panel.children.iter().position(|child| *child == row)?;
        Some((panel.parent?, index))
    }

    fn is_submenu(&self, id: NodeId) -> bool {
        self.nodes
            .get(&id)
            .is_some_and(|node| matches!(node.control, Control::Submenu { .. }))
    }

    /// The row a popup has highlighted.
    fn highlighted_row(&self, popup: PopupState) -> Option<NodeId> {
        let panel = self
            .nodes
            .get(&popup.owner)?
            .children
            .iter()
            .find(|child| self.nodes[*child].kind == Kind::Popup)?;
        self.nodes[panel].children.get(popup.highlighted).copied()
    }

    /// Closes the submenus opened from `owner`'s menu, so that it is the
    /// innermost one. False when `owner` has no open menu.
    fn close_levels_below(&mut self, owner: NodeId) -> bool {
        if self.popup_of(owner).is_none() {
            return false;
        }
        let mut changed = false;
        while self.popup.is_some_and(|popup| popup.owner != owner) {
            self.popup = self.popup_parents.pop();
            changed = true;
        }
        if changed {
            self.invalidate_paint(owner);
            self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
            self.view_stale = true;
        }
        changed
    }

    /// Closes the innermost menu: a submenu gives the keyboard back to the
    /// menu it opened from, and the outermost closes the popup.
    fn close_submenu(&mut self) -> bool {
        let Some(parent) = self.popup_parents.pop() else {
            return self.close_popup();
        };
        self.popup = Some(parent);
        self.invalidate_paint(parent.owner);
        self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        self.view_stale = true;
        true
    }

    fn close_popup(&mut self) -> bool {
        let innermost = self.popup.take();
        // Focus goes back to where the outermost menu took it from.
        let Some(popup) = self.popup_parents.drain(..).next().or(innermost) else {
            return false;
        };
        if self.nodes.get(&popup.owner).is_some_and(Node::focusable) {
            self.focus_control(popup.owner);
        } else if let Some(previous) = popup
            .return_focus
            .filter(|id| self.nodes.get(id).is_some_and(Node::focusable))
        {
            self.focus_control(previous);
        }
        true
    }

    fn open_popup(&mut self, owner: NodeId) -> bool {
        self.open_popup_at(owner, None)
    }

    pub(super) fn open_popup_at(&mut self, owner: NodeId, anchor: Option<Point>) -> bool {
        let Some(node) = self.nodes.get(&owner).filter(|node| node.enabled) else {
            return false;
        };
        let selected = match &node.control {
            Control::Combo { selected, .. } => *selected,
            _ => 0,
        };
        let Some(items) = self.popup_items(owner) else {
            return false;
        };
        let highlighted = if items.get(selected).is_some_and(|item| item.enabled) {
            selected
        } else if let Some(index) = items.iter().position(|item| item.enabled) {
            index
        } else {
            return false;
        };
        // A row of an open menu opens its own beside it, and that menu stays.
        if let Some((parent, index)) = self
            .row_place(owner)
            .filter(|(parent, _)| self.is_submenu(owner) && self.popup_of(*parent).is_some())
        {
            self.close_levels_below(parent);
            let mut outer = self.popup.take().expect("the parent menu is open");
            outer.highlighted = index;
            self.popup_parents.push(outer);
            self.popup = Some(PopupState {
                owner,
                highlighted,
                reveal_highlight: true,
                anchor: None,
                return_focus: None,
            });
            self.invalidate_paint(parent);
            self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
            self.view_stale = true;
            return true;
        }
        let return_focus = self.focused;
        self.close_popup();
        self.popup = Some(PopupState {
            owner,
            highlighted,
            reveal_highlight: true,
            anchor,
            return_focus,
        });
        if self.nodes[&owner].focusable() {
            self.focus_control(owner);
        } else {
            self.invalidate_paint(owner);
            self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        }
        true
    }

    fn choose(&mut self, owner: NodeId, index: usize) -> bool {
        if !self
            .popup_items(owner)
            .and_then(|items| items.get(index))
            .is_some_and(|item| item.enabled)
        {
            return false;
        }
        // An autocomplete field takes the suggestion as its text, caret at the end.
        if self.nodes[&owner].editor.is_some() {
            let label = self.popup_items(owner).unwrap()[index].label.clone();
            self.edit(owner, TextEdit::SelectAll);
            self.edit(owner, TextEdit::Insert(label));
        }
        // A row that opens a submenu is not a pick.
        if let Some(row) = self
            .popup_of(owner)
            .map(|popup| PopupState {
                highlighted: index,
                ..popup
            })
            .and_then(|popup| self.highlighted_row(popup))
            .filter(|row| self.is_submenu(*row))
        {
            return self.open_popup(row);
        }
        // A pick in a submenu is told to the menu it all hangs from, as
        // the way down to it.
        let mut path = vec![index];
        let mut top = owner;
        while self.is_submenu(top)
            && let Some((parent, place)) = self.row_place(top)
        {
            path.push(place);
            top = parent;
        }
        if path.len() == 1 {
            self.queue_update(top, ControlUpdate::Choice(index));
        } else {
            path.reverse();
            self.queue_update(top, ControlUpdate::Path(path));
        }
        self.close_popup();
        true
    }

    /// The typed part of an autocomplete suggestion row, as a byte range of
    /// its label: the first match ignoring case.
    pub(super) fn suggestion_match(&self, row: &Node) -> Option<(usize, usize)> {
        let Control::Choice { .. } = row.control else {
            return None;
        };
        let owner = self.nodes.get(&row.parent?)?.parent?;
        let Control::TextInput {
            value,
            suggestions: Some(_),
            ..
        } = &self.nodes.get(&owner)?.control
        else {
            return None;
        };
        let typed: Vec<char> = value.trim().chars().flat_map(char::to_lowercase).collect();
        if typed.is_empty() {
            return None;
        }
        // Lowercasing can change byte lengths, so compare from every start
        // in the label itself instead of searching a lowercased copy.
        row.text.char_indices().find_map(|(start, _)| {
            let mut wanted = typed.iter();
            let mut next = wanted.next();
            for (offset, character) in row.text[start..].char_indices() {
                for lower in character.to_lowercase() {
                    if next != Some(&lower) {
                        return None;
                    }
                    next = wanted.next();
                }
                if next.is_none() {
                    return Some((start, start + offset + character.len_utf8()));
                }
            }
            None
        })
    }

    /// A text field's current text, its declared number and whether it has focus.
    pub(crate) fn field_state(&self, id: NodeId) -> Option<FieldState> {
        let node = self.nodes.get(&id)?;
        let Control::TextInput {
            value,
            number,
            draft,
            ..
        } = &node.control
        else {
            return None;
        };
        let editor = node.editor.as_ref()?;
        let (text, revealed) = (editor.text.clone(), editor.revealed);
        Some(FieldState {
            revealed,
            // Typed since the field last declared its text, or still being typed.
            draft: *draft || text != *value,
            text,
            number: *number,
            focused: self.focused == Some(id),
        })
    }

    /// Queues steps for a number field; several between two frames add up.
    pub(super) fn step_number(&mut self, id: NodeId, steps: f64) -> bool {
        let numeric = self.nodes.get(&id).is_some_and(|node| {
            node.enabled
                && matches!(
                    node.control,
                    Control::TextInput {
                        number: Some(_),
                        ..
                    }
                )
        });
        if !numeric {
            return false;
        }
        let queued = match self.updates.get(&id) {
            Some(ControlUpdate::Number(queued)) => *queued,
            _ => 0.0,
        };
        self.queue_update(id, ControlUpdate::Number(queued + steps));
        true
    }

    /// The password field beside the reveal button `id`.
    pub(super) fn reveal_field(&self, id: NodeId) -> Option<NodeId> {
        let node = self.nodes.get(&id)?;
        if !matches!(node.control, Control::Reveal { .. }) {
            return None;
        }
        self.nodes
            .get(&node.parent?)?
            .children_by_key
            .get(&WidgetKey::named("$field"))
            .copied()
    }

    /// Shows or hides the text of the password field beside the reveal
    /// button `id`. The button has no handler, so its click makes the view
    /// stale and the next declaration names the button for what it now does.
    fn toggle_reveal(&mut self, id: NodeId) {
        let Some(field) = self.reveal_field(id) else {
            return;
        };
        if let Some(editor) = self
            .nodes
            .get_mut(&field)
            .and_then(|node| node.editor.as_mut())
        {
            editor.revealed = !editor.revealed;
            editor.masked = !editor.revealed;
            self.invalidate_paint(field);
        }
    }

    pub(super) fn activate(&mut self, id: NodeId) -> bool {
        if let Some(handled) = self.collection_action(id, &Action::Activate) {
            return handled;
        }
        let Some(node) = self.nodes.get(&id).filter(|node| node.activatable()) else {
            return false;
        };
        match node.control {
            Control::Combo { .. } | Control::Menu { .. } => {
                if self.popup_of(id).is_some() {
                    self.close_popup()
                } else {
                    self.open_popup(id)
                }
            }
            Control::Submenu { .. } => {
                if self.popup_of(id).is_some() {
                    // Its menu is open already: pressing it again leaves it so.
                    false
                } else {
                    self.open_popup(id)
                }
            }
            Control::Choice { index, .. } => {
                let Some(owner) = node.parent.and_then(|panel| self.nodes[&panel].parent) else {
                    return false;
                };
                self.choose(owner, index)
            }
            Control::Tab { .. } => self.select_tab(id),
            Control::StripScroll { end } => {
                let Some(strip) = node.parent else {
                    return false;
                };
                self.scroll_strip_step(strip, end)
            }
            Control::Reveal { .. } => {
                self.toggle_reveal(id);
                self.queue_click(id);
                self.invalidate_paint(id);
                true
            }
            Control::Radio { .. } => {
                let siblings = self.nodes[&node.parent.unwrap()].children.clone();
                for sibling in siblings {
                    if self.nodes[&sibling].kind == Kind::Radio {
                        self.clicks.remove(&sibling);
                    }
                }
                self.queue_click(id);
                self.invalidate_paint(id);
                true
            }
            _ => {
                self.queue_click(id);
                self.invalidate_paint(id);
                true
            }
        }
    }

    fn popup_key(&mut self, key: Key) -> Option<bool> {
        let popup = self.popup?;
        // An autocomplete field keeps typing while its list is open: only the
        // arrows and Escape belong to the list, and Enter only with a highlight.
        if self
            .nodes
            .get(&popup.owner)
            .is_some_and(|owner| owner.editor.is_some())
        {
            let items = self.popup_items(popup.owner).map_or(0, <[_]>::len);
            match key {
                // An empty list shows nothing: forget it when the field is left.
                Key::Escape | Key::Tab if items == 0 => {
                    self.close_popup();
                    return None;
                }
                _ if items == 0 => return None,
                Key::Enter if popup.highlighted >= items => {
                    self.close_popup();
                    return None;
                }
                Key::Enter | Key::Escape | Key::Tab | Key::ArrowUp | Key::ArrowDown => {}
                _ => return None,
            }
        }
        let popup = self.popup?;
        // The arrow that points the reading way opens the submenu of the
        // highlighted row, and the other one goes back out of a submenu.
        if matches!(key, Key::ArrowLeft | Key::ArrowRight)
            && self.popup_items(popup.owner).is_some()
            && self.nodes[&popup.owner].editor.is_none()
        {
            let rtl = self.nodes[&popup.owner].resolved.rtl();
            let inward = (key == Key::ArrowRight) != rtl;
            if inward {
                let row = self
                    .highlighted_row(popup)
                    .filter(|row| self.is_submenu(*row) && self.nodes[row].enabled)?;
                return Some(self.open_popup(row));
            }
            if self.popup_parents.is_empty() {
                return None;
            }
            return Some(self.close_submenu());
        }
        match key {
            Key::Escape => Some(self.close_submenu()),
            Key::Tab => {
                self.close_popup();
                None
            }
            Key::Enter | Key::Space => Some(self.choose(popup.owner, popup.highlighted)),
            Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End => {
                let enabled: Vec<_> = self
                    .popup_items(popup.owner)?
                    .iter()
                    .enumerate()
                    .filter_map(|(i, item)| item.enabled.then_some(i))
                    .collect();
                if enabled.is_empty() {
                    return Some(self.close_popup());
                }
                let current = enabled.iter().position(|index| *index == popup.highlighted);
                let index = match (key, current) {
                    (Key::Home, _) => 0,
                    (Key::End, _) => enabled.len() - 1,
                    // Nothing highlighted yet: Down enters at the top, Up at the bottom.
                    (Key::ArrowUp, None) => enabled.len() - 1,
                    (_, None) => 0,
                    (Key::ArrowUp, Some(current)) => (current + enabled.len() - 1) % enabled.len(),
                    (_, Some(current)) => (current + 1) % enabled.len(),
                };
                self.popup.as_mut().unwrap().highlighted = enabled[index];
                self.popup.as_mut().unwrap().reveal_highlight = true;
                self.invalidate_paint(popup.owner);
                Some(true)
            }
            _ => None,
        }
    }

    pub(super) fn control_event(&mut self, event: &Event) -> Option<bool> {
        match event {
            Event::FocusLost => {
                self.close_popup();
                self.cancel_compositions_except(None);
            }
            Event::Text(edit) => {
                return Some(self.focused.is_some_and(|id| self.edit(id, edit.clone())));
            }
            Event::ContextMenu(point) => {
                self.pointer = Some(*point);
                return Some(self.open_context_menu_at(*point));
            }
            Event::PointerMoved(position) => {
                if let Some(id) = self.captured {
                    if self.nodes[&id].kind == Kind::Slider {
                        self.pointer = Some(*position);
                        return Some(self.slider_pointer(id, *position));
                    }
                    if self.nodes[&id].kind == Kind::Divider {
                        self.pointer = Some(*position);
                        self.divider_pointer(id, *position);
                        return Some(true);
                    }
                    if self.nodes[&id].kind == Kind::TextInput {
                        self.pointer = Some(*position);
                        return Some(self.editor_pointer(id, *position, true));
                    }
                }
                if let Some(id) = self.hit_test(*position)
                    && let Some((owner, index)) = self.row_place(id)
                    && self.popup_of(owner).is_some()
                    // The row whose submenu is open keeps it; any other row
                    // of that menu closes it and takes the highlight.
                    && self.popup_of(id).is_none()
                {
                    let opens = self.is_submenu(id) && self.nodes[&id].enabled;
                    let mut changed = self.close_levels_below(owner);
                    if let Some(popup) = &mut self.popup {
                        popup.highlighted = index;
                        popup.reveal_highlight = false;
                    }
                    if opens {
                        self.pointer = Some(*position);
                        changed |= self.open_popup(id);
                    }
                    if changed {
                        self.pointer = Some(*position);
                        return Some(true);
                    }
                }
            }
            Event::PointerDown | Event::PointerDownWithShift => {
                let position = self.pointer?;
                let target = self.hit_test(position);
                let outermost = self.popup_levels().next();
                if let Some(popup) = outermost {
                    let inside = self
                        .popup_panels()
                        .iter()
                        .any(|panel| self.nodes[panel].clip.contains(position));
                    if target != Some(popup.owner) && !inside {
                        self.close_popup();
                    }
                }
                if let Some(id) = target.filter(|id| self.nodes[id].enabled) {
                    let kind = self.nodes[&id].kind;
                    if kind == Kind::Divider {
                        // A column sizer is dragged without taking focus.
                        if !matches!(self.nodes[&id].control, Control::Sizer { .. }) {
                            self.focus_control(id);
                        }
                        self.captured = Some(id);
                        return Some(true);
                    }
                    if matches!(kind, Kind::TextInput | Kind::Slider) {
                        self.focus_control(id);
                        self.captured = Some(id);
                        if kind == Kind::TextInput {
                            self.editor_pointer(
                                id,
                                position,
                                matches!(event, Event::PointerDownWithShift),
                            );
                        } else {
                            self.slider_pointer(id, position);
                        }
                        return Some(true);
                    }
                }
            }
            Event::KeyDown { key, shift, .. } => {
                if let Some(handled) = self.popup_key(*key) {
                    return Some(handled);
                }
                if *key == Key::ContextMenu {
                    return Some(self.open_context_menu_for_focus());
                }
                let id = self.focused?;
                let node = &self.nodes[&id];
                if !node.enabled {
                    return Some(false);
                }
                match node.kind {
                    Kind::Tab => {
                        if let Some(handled) = self.tab_key(id, *key) {
                            return Some(handled);
                        }
                    }
                    Kind::Divider => {
                        if let Some(handled) = self.divider_key(id, *key) {
                            return Some(handled);
                        }
                    }
                    Kind::DockHandle => {
                        if let Some(handled) = self.dock_handle_key(id, *key) {
                            return Some(handled);
                        }
                    }
                    _ => {}
                }
                let node = &self.nodes[&id];
                if node.editor.is_some() {
                    let steps = match key {
                        Key::ArrowUp => 1.0,
                        Key::ArrowDown => -1.0,
                        Key::PageUp => 10.0,
                        Key::PageDown => -10.0,
                        _ => 0.0,
                    };
                    if steps != 0.0 && self.step_number(id, steps) {
                        return Some(true);
                    }
                    // Down reopens the suggestions of an autocomplete field.
                    if *key == Key::ArrowDown && self.open_popup(id) {
                        return Some(true);
                    }
                    let edit = match key {
                        Key::ArrowLeft => TextEdit::Move {
                            direction: TextMove::Left,
                            select: *shift,
                            word: false,
                        },
                        Key::ArrowRight => TextEdit::Move {
                            direction: TextMove::Right,
                            select: *shift,
                            word: false,
                        },
                        Key::Home => TextEdit::Move {
                            direction: TextMove::Start,
                            select: *shift,
                            word: false,
                        },
                        Key::End => TextEdit::Move {
                            direction: TextMove::End,
                            select: *shift,
                            word: false,
                        },
                        Key::Backspace => TextEdit::Backspace { word: false },
                        Key::Delete => TextEdit::Delete { word: false },
                        Key::Escape => TextEdit::CancelComposition,
                        Key::Enter => {
                            self.queue_submit(id);
                            return Some(true);
                        }
                        _ => return None,
                    };
                    return Some(self.edit(id, edit));
                }
                if let Control::Slider {
                    value,
                    min,
                    max,
                    step,
                    ..
                } = node.control
                {
                    // Left and Right follow the slider's direction on screen.
                    let step = match key {
                        Key::ArrowLeft | Key::ArrowRight if node.resolved.rtl() => -step,
                        _ => step,
                    };
                    let next = match key {
                        Key::ArrowLeft | Key::ArrowDown => value - step,
                        Key::ArrowRight | Key::ArrowUp => value + step,
                        Key::PageDown => value - step * 10.0,
                        Key::PageUp => value + step * 10.0,
                        Key::Home => min,
                        Key::End => max,
                        _ => return None,
                    };
                    return Some(self.set_number(id, next));
                }
                if node.kind == Kind::Radio
                    && matches!(
                        key,
                        Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown
                    )
                {
                    let siblings: Vec<_> = self.nodes[&node.parent.unwrap()]
                        .children
                        .iter()
                        .copied()
                        .filter(|id| self.nodes[id].kind == Kind::Radio && self.nodes[id].enabled)
                        .collect();
                    let position = siblings
                        .iter()
                        .position(|candidate| *candidate == id)
                        .unwrap();
                    let next = if matches!(key, Key::ArrowLeft | Key::ArrowUp) {
                        (position + siblings.len() - 1) % siblings.len()
                    } else {
                        (position + 1) % siblings.len()
                    };
                    self.focus_control(siblings[next]);
                    self.activate(siblings[next]);
                    return Some(true);
                }
                if matches!(node.kind, Kind::ComboBox | Kind::MenuButton)
                    && matches!(key, Key::ArrowDown | Key::ArrowUp)
                {
                    return Some(self.open_popup(id));
                }
            }
            _ => {}
        }
        None
    }

    pub(super) fn control_action(&mut self, target: NodeId, action: &Action) -> Option<bool> {
        let node = self.nodes.get(&target)?;
        if !node.enabled {
            return Some(false);
        }
        match action {
            Action::SetText(text) if node.editor.is_some() => {
                let selected = self.edit(target, TextEdit::SelectAll);
                Some(self.edit(target, TextEdit::Insert(text.clone())) || selected)
            }
            Action::SetNumber(value) if matches!(node.control, Control::Sizer { .. }) => {
                Some(self.set_size(target, *value as f32))
            }
            Action::SetNumber(value) if node.kind == Kind::Divider => {
                Some(self.set_fraction(target, (*value / 100.0) as f32))
            }
            // A number field takes the value as text and commits it like Enter.
            Action::SetNumber(value)
                if matches!(
                    node.control,
                    Control::TextInput {
                        number: Some(_),
                        ..
                    }
                ) =>
            {
                self.edit(target, TextEdit::SelectAll);
                self.edit(target, TextEdit::Insert(value.to_string()));
                self.queue_submit(target);
                Some(true)
            }
            Action::SetNumber(value) => Some(self.set_number(target, *value)),
            Action::Increment | Action::Decrement => {
                let up = matches!(action, Action::Increment);
                if self.step_number(target, if up { 1.0 } else { -1.0 }) {
                    return Some(true);
                }
                let node = self.nodes.get(&target)?;
                let Control::Slider { value, step, .. } = node.control else {
                    return Some(false);
                };
                Some(self.set_number(target, value + if up { step } else { -step }))
            }
            Action::SetSelection { anchor, focus } => {
                Some(self.set_selection(target, *anchor, *focus))
            }
            Action::Expand => Some(self.open_popup(target)),
            Action::Collapse => Some(self.popup_of(target).is_some() && self.close_popup()),
            Action::Focus => {
                if node.focusable() {
                    self.focus_control(target);
                    Some(true)
                } else {
                    Some(false)
                }
            }
            _ => None,
        }
    }
}
