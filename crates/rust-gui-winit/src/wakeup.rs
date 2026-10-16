use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use winit::event_loop::EventLoopProxy;

pub(crate) enum UserEvent {
    Wake,
    Exit,
    Accessibility(accesskit_winit::Event),
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Accessibility(event)
    }
}

struct WakeState {
    proxy: EventLoopProxy<UserEvent>,
    pending: AtomicBool,
    closed: AtomicBool,
}

/// Cloneable, thread-safe wakeup for application-owned shared state or queues.
/// Write the new state first, then request a redraw. Repeated requests coalesce.
#[derive(Clone)]
pub struct WakeHandle(Arc<WakeState>);

impl WakeHandle {
    pub fn is_closed(&self) -> bool {
        self.0.closed.load(Ordering::Acquire)
    }
    pub(crate) fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        Self(Arc::new(WakeState {
            proxy,
            pending: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        }))
    }

    /// Returns false after shutdown. This wakes the event loop without polling.
    pub fn request_redraw(&self) -> bool {
        if self.0.closed.load(Ordering::Acquire) {
            return false;
        }
        if self.0.pending.swap(true, Ordering::AcqRel) {
            return true;
        }
        if self.0.proxy.send_event(UserEvent::Wake).is_err() {
            self.close();
            return false;
        }
        true
    }

    pub fn request_exit(&self) -> bool {
        !self.0.closed.load(Ordering::Acquire) && self.0.proxy.send_event(UserEvent::Exit).is_ok()
    }

    pub(crate) fn acknowledge(&self) {
        self.0.pending.store(false, Ordering::Release);
    }
    pub(crate) fn close(&self) {
        self.0.closed.store(true, Ordering::Release);
    }
}
