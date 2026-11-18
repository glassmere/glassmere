use super::*;
#[cfg(feature = "std")]
use crate::ToastSender;
use crate::{Toast, ToastAction, ToastId};
#[cfg(feature = "std")]
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::cell::Cell;
#[cfg(feature = "std")]
use {alloc::sync::Arc, std::sync::Mutex};

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
    next_id: Cell<u64>,
    paused: bool,
    actions: VecDeque<ToastAction>,
    #[cfg(feature = "std")]
    inbox: Arc<Mutex<Vec<Toast>>>,
    #[cfg(feature = "std")]
    wake: Arc<Mutex<Box<dyn Fn() + Send>>>,
}

impl Default for ToastState {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            next_id: Cell::new(1),
            paused: false,
            actions: VecDeque::new(),
            #[cfg(feature = "std")]
            inbox: Arc::default(),
            #[cfg(feature = "std")]
            wake: Arc::new(Mutex::new(Box::new(|| {}))),
        }
    }
}

impl Context {
    /// Shows a toast, as `Effects::toast` does; for hosts and tests. A toast equal
    /// to one already waiting or showing restarts that one instead of stacking.
    pub fn push_toast(&mut self, toast: Toast) -> ToastId {
        let id = self.next_toast_id();
        self.queue_toast(id, toast)
    }

    pub(crate) fn next_toast_id(&self) -> ToastId {
        let id = self.toast.next_id.get();
        self.toast.next_id.set(id + 1);
        ToastId(id)
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
        self.view_stale = true;
    }

    /// How `ToastSender::send` wakes this context's window; hosts install it.
    #[cfg(feature = "std")]
    pub fn set_toast_wake(&mut self, wake: impl Fn() + Send + 'static) {
        *self
            .toast
            .wake
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(wake);
    }

    #[cfg(feature = "std")]
    pub(crate) fn toast_sender(&self) -> ToastSender {
        ToastSender {
            inbox: self.toast.inbox.clone(),
            wake: self.toast.wake.clone(),
        }
    }

    /// Takes the actions of the toasts `mine` names. The rest, of toasts
    /// whose action sends nothing, wait at most a few dozen deep.
    pub(crate) fn take_toast_actions(
        &mut self,
        mut mine: impl FnMut(ToastId) -> bool,
    ) -> Vec<ToastAction> {
        let mut taken = Vec::new();
        self.toast.actions.retain(|action| {
            if mine(action.id) {
                taken.push(action.clone());
                false
            } else {
                true
            }
        });
        taken
    }

    pub(crate) fn has_toast(&self, id: ToastId) -> bool {
        self.toast.queue.iter().any(|queued| queued.id == id)
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
        #[cfg(feature = "std")]
        {
            let arrived = core::mem::take(
                &mut *self
                    .toast
                    .inbox
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            );
            for toast in arrived {
                self.push_toast(toast);
            }
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
            let before = self.toast.queue.len();
            self.toast
                .queue
                .retain(|queued| queued.expires.is_none_or(|expires| expires > now));
            // The toasts are declared, so one leaving declares again.
            self.view_stale |= self.toast.queue.len() != before;
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

    /// After a declaration: what the toast buttons did.
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
                // Hosts redraw while this is true, so the action is delivered
                // as a message in the next batch even with no input.
                self.redraw_requested = true;
            }
            self.dismiss_toast(id);
        }
        // An app that never asks does not grow a backlog.
        while self.toast.actions.len() > 32 {
            self.toast.actions.pop_front();
        }
    }
}
