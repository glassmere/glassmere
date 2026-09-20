use crate::WakeHandle;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
};

struct Shared {
    pending: AtomicUsize,
    wake: Box<dyn Fn() -> bool + Send + Sync>,
}
/// Bounded nonblocking messages from workers or application-owned async runtimes.
pub struct MessageSender<T> {
    sender: SyncSender<T>,
    shared: Arc<Shared>,
}
impl<T> Clone for MessageSender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            shared: self.shared.clone(),
        }
    }
}
impl<T> MessageSender<T> {
    pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
        self.shared.pending.fetch_add(1, Ordering::AcqRel);
        match self.sender.try_send(message) {
            Ok(()) => {
                (self.shared.wake)();
                Ok(())
            }
            Err(error) => {
                self.shared.pending.fetch_sub(1, Ordering::AcqRel);
                Err(error)
            }
        }
    }
}
/// Drain before widget declaration. Partial drains schedule another UI turn.
pub struct Mailbox<T> {
    receiver: Receiver<T>,
    shared: Arc<Shared>,
}
impl<T> Mailbox<T> {
    pub fn drain(&self, limit: usize) -> Vec<T> {
        assert!(limit > 0, "mailbox drain limit must be positive");
        let messages: Vec<_> = self.receiver.try_iter().take(limit).collect();
        self.shared
            .pending
            .fetch_sub(messages.len(), Ordering::AcqRel);
        if self.shared.pending.load(Ordering::Acquire) > 0 {
            (self.shared.wake)();
        }
        messages
    }
}
fn channel<T>(
    capacity: usize,
    wake: impl Fn() -> bool + Send + Sync + 'static,
) -> (MessageSender<T>, Mailbox<T>) {
    assert!(capacity > 0, "mailbox capacity must be positive");
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let shared = Arc::new(Shared {
        pending: AtomicUsize::new(0),
        wake: Box::new(wake),
    });
    (
        MessageSender {
            sender,
            shared: shared.clone(),
        },
        Mailbox { receiver, shared },
    )
}
impl WakeHandle {
    pub fn channel<T>(&self, capacity: usize) -> (MessageSender<T>, Mailbox<T>) {
        let wake = self.clone();
        channel(capacity, move || wake.request_redraw())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_messages_preserve_order_report_backpressure_and_rewake_partial_drains() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let counted = wakes.clone();
        let (sender, receiver) = channel(2, move || {
            counted.fetch_add(1, Ordering::Relaxed);
            true
        });
        sender.try_send(1).unwrap();
        sender.clone().try_send(2).unwrap();
        assert!(matches!(sender.try_send(3), Err(TrySendError::Full(3))));
        assert_eq!(receiver.drain(1), [1]);
        assert_eq!(wakes.load(Ordering::Relaxed), 3);
        assert_eq!(receiver.drain(10), [2]);
        assert!(receiver.drain(1).is_empty());
        drop(receiver);
        assert!(matches!(
            sender.try_send(4),
            Err(TrySendError::Disconnected(4))
        ));
    }
}
