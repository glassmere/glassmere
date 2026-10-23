//! The values that move between threads are `Send`, checked at compile time.
//! The generic ones are checked at a concrete message type.

use glassmere::{Cancel, Clip, Image, MessageSender, TaskSender, TextDocument};

fn assert_send<T: Send + 'static>() {}

#[test]
fn values_that_cross_threads_are_send() {
    assert_send::<Image>();
    assert_send::<Clip>();
    assert_send::<TextDocument>();
    assert_send::<Cancel>();
    assert_send::<TaskSender<u32, String>>();
    assert_send::<MessageSender<String>>();
}
