use super::*;
use crate::{TextMove, controls::slider_value};

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

    fn edit(&mut self, id: NodeId, edit: TextEdit) -> bool {
        let Some(node) = self.nodes.get_mut(&id).filter(|node| node.enabled) else {
            return false;
        };
        let Some(editor) = &mut node.editor else {
            return false;
        };
        let old_text = editor.text.clone();
        if !editor.apply(edit) {
            return false;
        }
        if old_text != editor.text {
            self.updates
                .insert(id, ControlUpdate::Text(editor.text.clone()));
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
        self.updates.insert(id, ControlUpdate::Number(next));
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
        self.set_number(id, min + f64::from(fraction) * (max - min))
    }

    fn editor_pointer(&mut self, id: NodeId, position: Point, extend: bool) -> bool {
        let node = &self.nodes[&id];
        let Some(editor) = &node.editor else {
            return false;
        };
        let local = Point {
            x: position.x - node.bounds.x - node.style.padding + editor.scroll_x,
            y: position.y - node.bounds.y - node.style.padding,
        };
        let cursor = editor.geometry.hit(local);
        let anchor = if extend {
            editor.selection.anchor
        } else {
            cursor
        };
        self.edit(id, TextEdit::Select { anchor, cursor })
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
            | Control::ContextMenu { items } => Some(items),
            _ => None,
        }
    }

    fn close_popup(&mut self) -> bool {
        let Some(popup) = self.popup.take() else {
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
        self.updates.insert(owner, ControlUpdate::Choice(index));
        self.close_popup();
        true
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
                if self.popup.is_some_and(|popup| popup.owner == id) {
                    self.close_popup()
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
            Control::Radio { .. } => {
                let siblings = self.nodes[&node.parent.unwrap()].children.clone();
                for sibling in siblings {
                    if self.nodes[&sibling].kind == Kind::Radio {
                        self.clicks.remove(&sibling);
                    }
                }
                self.clicks.insert(id);
                self.invalidate_paint(id);
                true
            }
            _ => {
                self.clicks.insert(id);
                self.invalidate_paint(id);
                true
            }
        }
    }

    fn popup_key(&mut self, key: Key) -> Option<bool> {
        let popup = self.popup?;
        match key {
            Key::Escape => Some(self.close_popup()),
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
                let current = enabled
                    .iter()
                    .position(|index| *index == popup.highlighted)
                    .unwrap_or(0);
                let index = match key {
                    Key::Home => 0,
                    Key::End => enabled.len() - 1,
                    Key::ArrowUp => (current + enabled.len() - 1) % enabled.len(),
                    _ => (current + 1) % enabled.len(),
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
                    && let Control::Choice { index, .. } = self.nodes[&id].control
                    && let Some(popup) = &mut self.popup
                {
                    popup.highlighted = index;
                    popup.reveal_highlight = false;
                }
            }
            Event::PointerDown | Event::PointerDownWithShift => {
                let position = self.pointer?;
                let target = self.hit_test(position);
                if let Some(popup) = self.popup {
                    let inside = self.nodes[&popup.owner]
                        .children
                        .first()
                        .is_some_and(|panel| self.nodes[panel].clip.contains(position));
                    if target != Some(popup.owner) && !inside {
                        self.close_popup();
                    }
                }
                if let Some(id) = target.filter(|id| self.nodes[id].enabled) {
                    let kind = self.nodes[&id].kind;
                    if kind == Kind::Divider {
                        self.focus_control(id);
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
                    _ => {}
                }
                let node = &self.nodes[&id];
                if node.editor.is_some() {
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
                            self.submitted.insert(id);
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
            Action::SetNumber(value) if node.kind == Kind::Divider => {
                Some(self.set_fraction(target, (*value / 100.0) as f32))
            }
            Action::SetNumber(value) => Some(self.set_number(target, *value)),
            Action::Increment | Action::Decrement => {
                let Control::Slider { value, step, .. } = node.control else {
                    return Some(false);
                };
                Some(self.set_number(
                    target,
                    value
                        + if matches!(action, Action::Increment) {
                            step
                        } else {
                            -step
                        },
                ))
            }
            Action::Expand => Some(self.open_popup(target)),
            Action::Collapse => {
                Some(self.popup.is_some_and(|popup| popup.owner == target) && self.close_popup())
            }
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
