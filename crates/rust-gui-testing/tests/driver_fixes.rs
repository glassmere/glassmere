use rust_gui_core::{App, Effects, Ui};
use rust_gui_scene::Size;
use rust_gui_testing::Harness;

#[derive(Clone)]
struct Noop;

#[derive(Default)]
struct Counter {
    value: u32,
}

impl App for Counter {
    type Message = Noop;

    fn update(&mut self, _: Noop, _: &mut Effects<'_, Noop>) {}

    fn view(&self, ui: &mut Ui<'_, Noop>) {
        ui.label(format!("Value {}", self.value));
    }
}

#[test]
fn state_mut_then_run_shows_the_change() {
    let mut harness = Harness::app(Size::new(300.0, 200.0), Counter::default());
    harness.state_mut().value = 7;
    harness.run();
    harness.by_label("Value 7");
}
