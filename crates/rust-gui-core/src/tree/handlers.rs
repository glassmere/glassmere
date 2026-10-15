//! What nodes send: the queue of what reached them, the typed handler slab
//! of the `App` driving the context, and the frame without a declaration.

use super::*;
use crate::app::{Handler, HandlerSlab, Requests};

impl Context {
    pub(super) fn queue_click(&mut self, id: NodeId) {
        self.clicks.insert(id);
        self.queued.push((id, Queued::Click));
        self.view_stale = true;
    }

    pub(super) fn queue_submit(&mut self, id: NodeId) {
        self.submitted.insert(id);
        self.queued.push((id, Queued::Submit));
        self.view_stale = true;
    }

    pub(super) fn queue_update(&mut self, id: NodeId, update: ControlUpdate) {
        self.updates.insert(id, update);
        self.note_update(id);
    }

    /// An update for `id` is waiting. Updates to one node merge into one
    /// entry, so it is queued once, where the first arrived.
    pub(super) fn note_update(&mut self, id: NodeId) {
        if !self.queued.contains(&(id, Queued::Update)) {
            self.queued.push((id, Queued::Update));
        }
        self.view_stale = true;
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
        self.submitted.retain(handled);
        self.handlers = Some(boxed);
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
    pub(crate) fn apply_requests(&mut self, requests: &mut Requests) -> bool {
        let mut changed = false;
        if let Some(theme) = requests.theme.take() {
            self.set_theme(theme);
            changed = true;
        }
        if let Some(messages) = requests.messages.take() {
            self.set_messages(messages);
            changed = true;
        }
        if let Some(format) = requests.format.take() {
            self.set_format(format);
            changed = true;
        }
        for (id, shortcut) in requests.bindings.drain(..) {
            self.set_binding(id, shortcut);
            changed = true;
        }
        if let Some(clip) = requests.clip.take() {
            self.clip_out = Some(clip);
        }
        if std::mem::take(&mut requests.paste) {
            self.clip_in = None;
            self.paste_requested = true;
        }
        if let Some(files) = requests.files.take() {
            self.chosen_files = None;
            self.file_request = Some(files);
        }
        self.notifications.append(&mut requests.notifications);
        self.window_commands.append(&mut requests.window_commands);
        for toast in requests.toasts.drain(..) {
            requests.toast_ids.push(self.push_toast(toast));
            changed = true;
        }
        // ponytail: multiple windows are not wired yet; the requests are
        // dropped until the desktop host takes them.
        requests.open_windows.clear();
        requests.close_windows.clear();
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
