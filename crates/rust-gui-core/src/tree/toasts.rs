use super::*;
use crate::{Toast, ToastAction, ToastId, ToastSender};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Toasts on screen at once; later ones wait and their time starts when shown.
pub(crate) const VISIBLE_TOASTS: usize = 4;
/// What every toast is given again after the pointer or the keyboard leaves them.
const GRACE: Duration = Duration::from_millis(1500);

struct Queued {
    id: ToastId,
    toast: Toast,
    /// Set when the toast becomes visible.
    expires: Option<Duration>,
}

pub(super) struct ToastState {
    queue: Vec<Queued>,
    /// Shared with `Ui::toast`, which only has `&Context`.
    next_id: AtomicU64,
    paused: bool,
    actions: VecDeque<ToastAction>,
    inbox: Arc<Mutex<Vec<Toast>>>,
    wake: Arc<Mutex<Box<dyn Fn() + Send>>>,
}

impl Default for ToastState {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            next_id: AtomicU64::new(1),
            paused: false,
            actions: VecDeque::new(),
            inbox: Arc::default(),
            wake: Arc::new(Mutex::new(Box::new(|| {}))),
        }
    }
}

impl Context {
    /// Shows a toast, as `Ui::toast` does; for hosts and tests. A toast equal
    /// to one already waiting or showing restarts that one instead of stacking.
    pub fn push_toast(&mut self, toast: Toast) -> ToastId {
        let id = self.next_toast_id();
        self.queue_toast(id, toast)
    }

    pub(crate) fn next_toast_id(&self) -> ToastId {
        ToastId(self.toast.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn queue_toast(&mut self, id: ToastId, toast: Toast) -> ToastId {
        self.redraw_requested = true;
        if let Some(same) = self.toast.queue.iter_mut().find(|same| same.toast == toast) {
            // The newest caller's id is the one that stays valid.
            same.id = id;
            same.expires = None;
            return id;
        }
        self.toast.queue.push(Queued {
            id,
            toast,
            expires: None,
        });
        id
    }

    /// The toasts on screen, oldest first.
    pub fn toasts(&self) -> impl Iterator<Item = (ToastId, &Toast)> {
        self.toast
            .queue
            .iter()
            .take(VISIBLE_TOASTS)
            .map(|queued| (queued.id, &queued.toast))
    }

    pub fn dismiss_toast(&mut self, id: ToastId) {
        self.toast.queue.retain(|queued| queued.id != id);
        self.pending |= Dirty::PAINT;
    }

    /// How `ToastSender::send` wakes this context's window; hosts install it.
    pub fn set_toast_wake(&mut self, wake: impl Fn() + Send + 'static) {
        *self
            .toast
            .wake
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(wake);
    }

    pub(crate) fn toast_sender(&self) -> ToastSender {
        ToastSender {
            inbox: self.toast.inbox.clone(),
            wake: self.toast.wake.clone(),
        }
    }

    pub(crate) fn toast_action_at(&self, index: usize) -> Option<ToastAction> {
        self.toast.actions.get(index).cloned()
    }

    fn within_toast(&self, id: Option<NodeId>) -> bool {
        let mut next = id;
        while let Some(node) = next.and_then(|id| self.nodes.get(&id)) {
            if node.kind == Kind::Toast {
                return true;
            }
            next = node.parent;
        }
        false
    }

    /// Before a declaration: toasts from other threads arrive, time starts for
    /// the ones that became visible, and the ones whose time is up leave.
    pub(super) fn update_toasts(&mut self) {
        let arrived = std::mem::take(
            &mut *self
                .toast
                .inbox
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for toast in arrived {
            self.push_toast(toast);
        }
        // Reading or reaching for a toast keeps all of them; afterwards each
        // gets a moment more, however long the pointer rested.
        let paused = self.within_toast(self.hovered) || self.within_toast(self.focused);
        let resumed = self.toast.paused && !paused;
        self.toast.paused = paused;
        let now = self.now;
        if resumed {
            for expires in self
                .toast
                .queue
                .iter_mut()
                .filter_map(|queued| queued.expires.as_mut())
            {
                *expires = (*expires).max(now + GRACE);
            }
        }
        if !paused {
            self.toast
                .queue
                .retain(|queued| queued.expires.is_none_or(|expires| expires > now));
        }
        // After the leavers are gone, so that toasts moving up from the wait
        // start their time now and the next wake already knows about them.
        for queued in self.toast.queue.iter_mut().take(VISIBLE_TOASTS) {
            if let Some(duration) = queued.toast.duration {
                queued.expires.get_or_insert(now + duration);
            }
        }
    }

    /// When the next toast leaves, unless they are being held.
    pub(super) fn toast_wake(&self) -> Option<Duration> {
        if self.toast.paused {
            return None;
        }
        self.toast
            .queue
            .iter()
            .take(VISIBLE_TOASTS)
            .filter_map(|queued| queued.expires)
            .min()
    }

    /// After a declaration: what `Ui::toast`, the toast buttons and
    /// `Ui::toast_action` did.
    pub(super) fn apply_toasts(&mut self, outbox: crate::ui::ToastOutbox) {
        for (id, toast) in outbox.raised {
            self.queue_toast(id, toast);
        }
        for (id, action) in outbox.closed {
            if action && let Some(queued) = self.toast.queue.iter().find(|queued| queued.id == id) {
                self.toast.actions.push_back(ToastAction {
                    id,
                    tag: queued.toast.tag.clone(),
                });
            }
            self.dismiss_toast(id);
        }
        self.toast
            .actions
            .drain(..outbox.taken.min(self.toast.actions.len()));
        // An app that never asks does not grow a backlog.
        while self.toast.actions.len() > 32 {
            self.toast.actions.pop_front();
        }
    }
}
