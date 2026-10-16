//! What nodes send: the queue of what reached them, the typed handler slab
//! of the `App` driving the context, and the frame without a declaration.

use super::*;
use crate::NumberOptions;
use crate::WindowId;
use crate::app::Shared;
use crate::app::{Handler, HandlerSlab, Requests};
use crate::collection::CollectionInput;

impl Context {
    /// Input for `id` that no handler will answer waits for a declaration
    /// that reads it; input that a handler answers declares only through the
    /// messages it sends.
    pub(super) fn stale_unless_handled(&mut self, id: NodeId) {
        if self
            .nodes
            .get(&id)
            .is_none_or(|node| node.handler.is_none())
        {
            self.view_stale = true;
        }
    }

    pub(super) fn queue_click(&mut self, id: NodeId) {
        self.clicks.insert(id);
        self.queued.push((id, Queued::Click));
        self.stale_unless_handled(id);
    }

    pub(super) fn queue_submit(&mut self, id: NodeId) {
        self.submitted.insert(id);
        self.queued.push((id, Queued::Submit));
        self.stale_unless_handled(id);
    }

    pub(super) fn queue_update(&mut self, id: NodeId, update: ControlUpdate) {
        // Typed text in a number field that sends messages waits for Enter or
        // for leaving the field; it is not a change the view reads.
        let typing = matches!(update, ControlUpdate::Text(_)) && self.sends_numbers(id);
        self.updates.insert(id, update);
        if typing {
            if !self.queued.contains(&(id, Queued::Update)) {
                self.queued.push((id, Queued::Update));
            }
        } else {
            self.note_update(id);
        }
    }

    /// Whether `id` is a number field that sends messages, whose typed text
    /// is a draft until it is committed.
    pub(super) fn sends_numbers(&self, id: NodeId) -> bool {
        self.nodes.get(&id).is_some_and(|node| {
            node.handler.is_some()
                && matches!(
                    node.control,
                    Control::TextInput {
                        number: Some(_),
                        ..
                    }
                )
        })
    }

    /// Leaving a number field commits its draft, as Enter does. It does not
    /// make the view stale: a draft that does not parse reverts silently, and
    /// one that does sends its message, which runs the view anyway.
    pub(crate) fn queue_blur(&mut self, id: NodeId) {
        let drafting = self.nodes.get(&id).is_some_and(|node| {
            matches!(
                node.control,
                Control::TextInput {
                    number: Some(_),
                    ..
                }
            )
        });
        if drafting {
            self.queued.push((id, Queued::Submit));
            self.submitted.insert(id);
        }
    }

    /// An update for `id` is waiting. Updates to one node merge into one
    /// entry, so it is queued once, where the first arrived.
    pub(super) fn note_update(&mut self, id: NodeId) {
        if !self.queued.contains(&(id, Queued::Update)) {
            self.queued.push((id, Queued::Update));
        }
        self.stale_unless_handled(id);
    }

    /// The slab of the `App` driving this context, taken out so that the
    /// nodes can be changed beside it. Put it back with `self.handlers`.
    fn take_slab<M: 'static>(&mut self) -> Box<dyn std::any::Any> {
        let slab = self
            .handlers
            .take()
            .unwrap_or_else(|| Box::new(HandlerSlab::<M>::default()));
        assert!(
            slab.is::<HandlerSlab<M>>(),
            "a context is driven by one message type"
        );
        slab
    }

    /// Turns what reached the nodes since the last frame into messages, in
    /// the order it arrived, through each node's handler. What reached a
    /// node without one stays where the declaration reads it.
    pub(crate) fn resolve<M: Clone + 'static>(&mut self, out: &mut Vec<M>) {
        self.text_typed.clear();
        self.flush_grid_pending();
        // Grid filter delays that have passed: queued before anything else, so
        // the declaration that follows the batch does not drop them.
        for rows in self.due_grid_filters() {
            self.collection_queue(rows, CollectionInput::FiltersDue);
        }
        if self.queued.is_empty() {
            return;
        }
        let mut boxed = self.take_slab::<M>();
        let slab = boxed.downcast_mut::<HandlerSlab<M>>().unwrap();
        let mut queued = std::mem::take(&mut self.queued);
        for (id, kind) in queued.drain(..) {
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            let Some(handler) = node
                .handler
                .and_then(|slot| slab.slots[slot as usize].as_ref())
            else {
                continue;
            };
            let enabled = node.enabled;
            match (kind, handler) {
                (Queued::Click, Handler::Message(message)) => {
                    if enabled && self.clicks.contains(&id) {
                        out.push(message.clone());
                    }
                }
                (Queued::Click, Handler::Refocus(message)) => {
                    if enabled && self.clicks.contains(&id) {
                        out.push(message.clone());
                        self.refocus_editor(id);
                    }
                }
                (Queued::Click, Handler::Step(steps)) => {
                    let row = node.parent.and_then(|row| self.nodes.get(&row));
                    let field = row.and_then(|row| {
                        row.children_by_key
                            .get(&WidgetKey::named("$field"))
                            .copied()
                    });
                    if let (true, Some(field)) = (enabled && self.clicks.contains(&id), field)
                        && let Some(Handler::Number { on, options }) = self
                            .nodes
                            .get(&field)
                            .and_then(|node| node.handler)
                            .and_then(|slot| slab.slots[slot as usize].as_ref())
                    {
                        self.commit_number(field, *steps, on.as_ref(), options, out);
                    }
                }
                (Queued::Update, Handler::Number { on, options }) => {
                    // Only steps commit; typed text waits for Enter or leaving.
                    if let Some(ControlUpdate::Number(steps)) = self.updates.remove(&id)
                        && enabled
                    {
                        self.commit_number(id, steps, on.as_ref(), options, out);
                    }
                }
                (Queued::Submit, Handler::Number { on, options }) => {
                    if enabled && self.submitted.contains(&id) {
                        self.commit_number(id, 0.0, on.as_ref(), options, out);
                    }
                }
                (
                    Queued::Submit,
                    Handler::Field {
                        submit: Some(message),
                        ..
                    },
                ) => {
                    if enabled && self.submitted.contains(&id) {
                        out.push(message.clone());
                    }
                }
                (
                    Queued::Submit,
                    Handler::Autocomplete {
                        submit: Some(message),
                        ..
                    },
                ) => {
                    if enabled && self.submitted.contains(&id) {
                        out.push(message.clone());
                    }
                }
                (Queued::Update, Handler::Autocomplete { change, .. }) => {
                    if let Some(update) = self.updates.remove(&id)
                        && enabled
                    {
                        if matches!(update, ControlUpdate::Text(_)) {
                            self.text_typed.insert(id);
                        }
                        change(update, out);
                    }
                }
                (Queued::Update, Handler::Map(map))
                | (
                    Queued::Update,
                    Handler::Field {
                        change: Some(map), ..
                    },
                ) => {
                    if let Some(mut update) = self.updates.remove(&id)
                        && enabled
                    {
                        if matches!(update, ControlUpdate::Text(_)) {
                            self.text_typed.insert(id);
                        }
                        // Fingers down and pressure are read when the message is
                        // made, as a declaration would read them.
                        if let ControlUpdate::Gesture(gesture) = &mut update {
                            (gesture.active, gesture.pressure) = self.gesture_contacts(id);
                        }
                        out.extend(map(update));
                    }
                }
                (Queued::Update, Handler::Each(each)) => {
                    if let Some(update) = self.updates.remove(&id)
                        && enabled
                    {
                        each(update, out);
                    }
                }
                // A pointer click already reached the widget as pointer events.
                (Queued::Click, Handler::Event(map)) => {
                    if enabled && self.clicks.contains(&id) && !self.pointer_clicks.contains(&id) {
                        out.extend(map(crate::UiEvent::Activate));
                    }
                }
                (Queued::Area, Handler::Event(map)) => {
                    let bounds = node.bounds;
                    // Taken even when disabled, so that nothing waits for a
                    // declaration that will not come.
                    for event in self.take_area_events(id) {
                        if enabled {
                            out.extend(map(crate::WidgetInput::local(event, bounds)));
                        }
                    }
                }
                _ => {}
            }
        }
        self.queued = queued;
        // A node with a handler has been answered; the rest wait for the
        // declaration that reads them.
        let nodes = &self.nodes;
        let handled = |id: &NodeId| nodes.get(id).is_none_or(|node| node.handler.is_none());
        self.clicks.retain(handled);
        self.pointer_clicks.retain(|id| self.clicks.contains(id));
        self.submitted.retain(handled);
        self.handlers = Some(boxed);
    }

    /// Gives the focus to the text of the editor that the button `id` is part
    /// of, as a toolbar press does: the text is ready for typing again.
    fn refocus_editor(&mut self, id: NodeId) {
        let area = WidgetKey::named("$area");
        let mut next = self.nodes.get(&id).and_then(|node| node.parent);
        while let Some(group) = next {
            let Some(node) = self.nodes.get(&group) else {
                return;
            };
            if let Some(target) = node.children_by_key.get(&area).copied() {
                self.focus_control(target);
                self.note_focus();
                return;
            }
            next = node.parent;
        }
    }

    /// Commits the number field `id`: `steps` steps from its draft, or from
    /// its declared number when it has none. `on` of the result is sent when
    /// it differs from the declared number. Either way the field then shows
    /// the committed number and forgets the draft, so text that does not
    /// parse reverts without a message.
    fn commit_number<M>(
        &mut self,
        id: NodeId,
        steps: f64,
        on: &dyn Fn(f64) -> M,
        options: &NumberOptions,
        out: &mut Vec<M>,
    ) {
        let Some(node) = self.nodes.get(&id).filter(|node| node.enabled) else {
            return;
        };
        let Control::TextInput {
            value,
            number: Some(declared),
            draft,
            ..
        } = &node.control
        else {
            return;
        };
        let declared = declared.value;
        let text = node
            .editor
            .as_ref()
            .map_or_else(String::new, |editor| editor.text.clone());
        let drafting = *draft || text != *value;
        let base = if drafting {
            options.parse_in(self, &text).unwrap_or(declared)
        } else {
            declared
        };
        let next = if steps != 0.0 {
            options.constrain(base + steps * options.step)
        } else {
            base
        };
        if next != declared {
            out.push(on(next));
        }
        let shown = options.format_in(self, next);
        let Some(node) = self.nodes.get_mut(&id) else {
            return;
        };
        if let Control::TextInput {
            value,
            number: Some(number),
            draft,
            ..
        } = &mut node.control
        {
            *value = shown.clone();
            number.value = next;
            *draft = false;
        }
        if let Some(editor) = &mut node.editor {
            editor.synchronize(&shown);
        }
        self.invalidate_paint(id);
        self.pending |= Dirty::ACCESSIBILITY;
    }

    /// Declares with the handlers kept in this context's slab: what the
    /// `App` driver runs.
    pub(crate) fn run_app<M: 'static>(
        &mut self,
        size: Size,
        measurer: &mut impl TextMeasurer,
        view: impl FnOnce(&mut Ui<'_, M>),
    ) -> FrameStats {
        let mut boxed = self.take_slab::<M>();
        let mut frame = std::mem::take(&mut boxed.downcast_mut::<HandlerSlab<M>>().unwrap().frame);
        self.handlers = Some(boxed);
        self.declare(size, measurer, &mut frame, view);
        let mut boxed = self.take_slab::<M>();
        let slab = boxed.downcast_mut::<HandlerSlab<M>>().unwrap();
        self.store_handlers(slab, &mut frame);
        slab.frame = frame;
        self.handlers = Some(boxed);
        self.stats
    }

    /// Moves what the declaration declared into the slots of the nodes it
    /// went to, and frees the slots of nodes that no longer send anything.
    fn store_handlers<M>(&mut self, slab: &mut HandlerSlab<M>, frame: &mut Vec<Handler<M>>) {
        for slot in self.freed_handlers.drain(..) {
            slab.slots[slot as usize] = None;
            slab.free.push(slot);
        }
        slab.targets.clear();
        slab.targets.resize(frame.len(), None);
        for (id, index) in self.handler_moves.drain(..) {
            match index {
                Some(index) => slab.targets[index as usize] = Some(id),
                None => {
                    if let Some(slot) = self.nodes.get_mut(&id).and_then(|node| node.handler.take())
                    {
                        slab.slots[slot as usize] = None;
                        slab.free.push(slot);
                    }
                }
            }
        }
        for (index, handler) in frame.drain(..).enumerate() {
            let Some(node) = slab.targets[index].and_then(|id| self.nodes.get_mut(&id)) else {
                continue;
            };
            let slot = *node.handler.get_or_insert_with(|| {
                slab.free.pop().unwrap_or_else(|| {
                    slab.slots.push(None);
                    (slab.slots.len() - 1) as u32
                })
            });
            slab.slots[slot as usize] = Some(handler);
        }
    }

    /// A frame without a declaration: time moves on, and layout and paint
    /// run over the tree as it is, where something is dirty.
    pub(crate) fn refresh(&mut self, size: Size, measurer: &mut impl TextMeasurer) -> FrameStats {
        let measurer = &mut Timed {
            inner: measurer,
            spent: Duration::ZERO,
        };
        self.begin_frame(size);
        self.resize_root(size);
        self.update_tooltip();
        self.layout_pass(size, measurer);
        self.prepare_controls(measurer);
        self.update_hover();
        self.paint_and_finish(size, measurer);
        self.stats
    }

    /// Before a batch decides whether to declare: what time changed.
    pub(crate) fn prepare_batch(&mut self) {
        self.update_toasts();
        self.start_grid_timers();
        self.update_long_presses();
    }

    /// Whether something the view reads changed since it last declared,
    /// apart from messages: kernel state it reads, host-set state, its own
    /// request for another frame, the window's size, or a hover, focus or
    /// press it observed.
    pub(crate) fn view_wanted(&self, size: Size, before: Interaction) -> bool {
        if self.view_stale || self.redraw_requested || size != self.size {
            return true;
        }
        let now = self.interaction();
        let observed = |a: Option<NodeId>, b: Option<NodeId>| {
            a != b
                && [a, b]
                    .into_iter()
                    .flatten()
                    .any(|id| self.nodes.get(&id).is_some_and(|node| node.observed.get()))
        };
        observed(before.hovered, now.hovered)
            || observed(before.focused, now.focused)
            || observed(before.pressed, now.pressed)
            || observed(before.keyboard, now.keyboard)
    }

    pub(crate) fn interaction(&self) -> Interaction {
        Interaction {
            hovered: self.hovered,
            focused: self.focused,
            pressed: self.captured.filter(|id| self.hovered == Some(*id)),
            keyboard: self.keyboard_pressed.map(|(id, _)| id),
        }
    }

    /// Records what the batch did in the frame's statistics.
    pub(crate) fn note_batch(&mut self, declared: bool, messages: usize) -> FrameStats {
        self.stats.declared = declared;
        self.stats.messages = messages;
        self.stats
    }

    /// Applies what `update` asked for. Returns whether the view reads any
    /// of it, and so has to declare.
    pub(crate) fn apply_requests(&mut self, window: WindowId, requests: &mut Requests) -> bool {
        let mut changed = false;
        if self.settings.0 != requests.theme.0 {
            self.settings.0 = requests.theme.0;
            if let Some(theme) = requests.theme.1.clone() {
                self.set_theme(theme);
            }
            changed = true;
        }
        if self.settings.1 != requests.messages.0 {
            self.settings.1 = requests.messages.0;
            self.set_messages(
                requests
                    .messages
                    .1
                    .clone()
                    .map(|m| Box::new(Shared(m)) as _),
            );
            changed = true;
        }
        if self.settings.2 != requests.format.0 {
            self.settings.2 = requests.format.0;
            self.set_format(requests.format.1.clone().map(|f| Box::new(Shared(f)) as _));
            changed = true;
        }
        for (id, shortcut) in requests.bindings.drain(..) {
            self.set_binding(id, shortcut);
            changed = true;
        }
        if let Some(clip) = requests.clip.take() {
            self.clip_out = Some(clip);
        }
        if requests.paste.contains(&window) {
            requests.paste.retain(|target| *target != window);
            self.clip_in = None;
            self.paste_requested = true;
        }
        // One dialog at a time: the next waits for the answer to this one.
        if !requests.file_busy.contains(&window)
            && let Some(first) = requests
                .files
                .iter()
                .position(|(target, _)| *target == window)
        {
            let (_, files) = requests.files.remove(first);
            requests.file_busy.push(window);
            self.chosen_files = None;
            self.file_request = Some(files);
        }
        self.notifications.append(&mut requests.notifications);
        self.window_commands.extend(
            requests
                .window_commands
                .extract_if(.., |(target, _)| *target == window)
                .map(|(_, command)| command),
        );
        for (_, number, toast) in requests
            .toasts
            .extract_if(.., |(target, ..)| *target == window)
        {
            requests
                .toast_ids
                .push((number, window, self.push_toast(toast)));
            changed = true;
        }
        changed
    }

    pub(crate) fn take_pasted(&mut self) -> Option<crate::Clip> {
        self.clip_in.take()
    }

    pub(crate) fn take_chosen_files(&mut self) -> Option<Vec<std::path::PathBuf>> {
        self.chosen_files.take()
    }

    /// Focuses the focusable widget declared with this key, wherever it is.
    pub(crate) fn focus_named(&mut self, key: &str) {
        let target = self.nodes.iter().find_map(|(id, node)| {
            (matches!(&node.key, WidgetKey::Named(name) if name == key) && node.focusable())
                .then_some(*id)
        });
        if let Some(target) = target {
            self.focus_control(target);
            self.note_focus();
        }
    }
}
