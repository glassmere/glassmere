//! A rotary dial, built the way a third party would build one: a normal
//! crate, depending on `seaglass` and on nothing private to it.
//!
//! It is here to hold the framework to its promise. Everything a built-in
//! control has, this has through the public API: its own state, the pointer
//! and the keyboard and the wheel, the theme's colours, a place in the tab
//! order, what assistive technology reads out, and tests that drive it
//! headlessly. If any of that needed an internal type, the promise would be
//! broken and this crate would not compile.

use seaglass::{
    Color, CustomWidget, EventArea, Key, NumericValue, Primitive, Rect, Role, Semantics, Size,
    Style, Ui, UiEvent, WidgetBuilder, WidgetInput,
};

/// How far the dial turns, in radians, and where it starts. Measured
/// clockwise from the positive x axis, which in screen coordinates points
/// right and down, so the sweep runs from the bottom left to the bottom
/// right through the top.
const START: f64 = 0.75 * std::f64::consts::PI;
const SWEEP: f64 = 1.5 * std::f64::consts::PI;
/// Pixels of vertical drag for the whole range.
const TRAVEL: f64 = 180.0;
const STEP: f64 = 0.05;
const TICKS: usize = 25;

/// What the dial's messages carry: an event it heard, in its own coordinates.
/// The application keeps it in its message type, wrapped as it likes.
#[derive(Clone, Debug)]
pub enum DialMsg {
    Heard(UiEvent),
}

/// A knob holding a number from zero to one. The application owns it, like
/// every other piece of state here; `update` changes it and `view` only
/// reads it.
#[derive(Clone, Debug, Default)]
pub struct Dial {
    value: f64,
    /// Where a drag started and what the value was then, so that a drag is
    /// relative to its own beginning rather than to the pointer's position.
    drag: Option<(f32, f64)>,
}

impl Dial {
    pub fn new(value: f64) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
            drag: None,
        }
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn set_value(&mut self, value: f64) {
        self.value = value.clamp(0.0, 1.0);
    }

    /// Applies what the dial heard. Returns whether the value turned.
    pub fn update(&mut self, message: DialMsg) -> bool {
        let before = self.value;
        match message {
            DialMsg::Heard(event) => self.apply(&event),
        }
        self.value != before
    }

    /// Declares the dial. `size` is its diameter in logical pixels. What it
    /// hears is sent as `wrap(DialMsg::Heard(..))`, and the view itself only
    /// reads the dial.
    pub fn view<M: Clone + 'static>(
        &self,
        ui: &mut Ui<'_, M>,
        key: &str,
        label: &str,
        size: f32,
        wrap: impl Fn(DialMsg) -> M + 'static,
    ) {
        let theme = ui.theme();
        let (face, ring, mark, dim, focus) = (
            theme.control,
            theme.accent,
            theme.text,
            theme.text_muted,
            theme.focus,
        );
        let label = label.to_owned();
        let widget = WidgetBuilder::new(|input: &WidgetInput| {
            self.paint(input, size, face, ring, mark, dim, focus, label)
        })
        .on_event(move |event| Some(wrap(DialMsg::Heard(event))));
        ui.keyed(key, |ui| ui.widget(Style::default(), widget));
    }

    /// One event. Dragging is vertical, because a knob followed around a
    /// circle jumps when the pointer crosses its centre.
    fn apply(&mut self, event: &UiEvent) {
        match event {
            UiEvent::PointerDown(at) => self.drag = Some((at.y, self.value)),
            UiEvent::PointerMoved(at) => {
                if let Some((from, value)) = self.drag {
                    self.set_value(value + f64::from(from - at.y) / TRAVEL);
                }
            }
            UiEvent::PointerUp(_) => self.drag = None,
            UiEvent::Scroll { delta_y, .. } => {
                self.set_value(self.value + f64::from(*delta_y) / TRAVEL);
            }
            UiEvent::Key { key, shift } => {
                let step = if *shift { STEP / 5.0 } else { STEP };
                match key {
                    Key::ArrowUp | Key::ArrowRight => self.set_value(self.value + step),
                    Key::ArrowDown | Key::ArrowLeft => self.set_value(self.value - step),
                    Key::Home => self.set_value(0.0),
                    Key::End => self.set_value(1.0),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// What it looks like and what it is. Paint is in the widget's own
    /// coordinates, so nothing here knows where on the screen it sits.
    #[allow(clippy::too_many_arguments)]
    fn paint(
        &self,
        input: &WidgetInput,
        size: f32,
        face: Color,
        ring: Color,
        mark: Color,
        dim: Color,
        focus: Color,
        label: String,
    ) -> CustomWidget {
        let centre = f64::from(size) / 2.0;
        let mut primitives = Vec::new();
        let dot = |at: (f64, f64), radius: f64, color: Color| Primitive::Quad {
            rect: Rect {
                x: (at.0 - radius) as f32,
                y: (at.1 - radius) as f32,
                width: (radius * 2.0) as f32,
                height: (radius * 2.0) as f32,
            },
            color,
            radius: radius as f32,
        };
        let around = |angle: f64, distance: f64| {
            (
                centre + distance * angle.cos(),
                centre + distance * angle.sin(),
            )
        };
        if input.focused {
            primitives.push(dot((centre, centre), centre, focus));
        }
        primitives.push(dot((centre, centre), centre - 3.0, face));
        // The ticks around it, lit as far as the value.
        for tick in 0..TICKS {
            let along = tick as f64 / (TICKS - 1) as f64;
            let lit = along <= self.value + f64::EPSILON;
            let color = match (input.enabled, lit) {
                (false, _) => dim,
                (true, true) => ring,
                (true, false) => face,
            };
            primitives.push(dot(
                around(START + SWEEP * along, centre * 0.78),
                2.5,
                color,
            ));
        }
        // The needle, as dots from the middle outward.
        let angle = START + SWEEP * self.value;
        let needle = if input.enabled { mark } else { dim };
        for step in 1..=5 {
            let distance = centre * 0.12 * f64::from(step);
            primitives.push(dot(around(angle, distance), 2.5, needle));
        }
        if input.hovered || input.pressed {
            primitives.push(dot((centre, centre), centre * 0.1, ring));
        }
        CustomWidget {
            size: Size::new(size, size),
            semantics: Semantics {
                role: Role::Slider,
                label,
                disabled: !input.enabled,
                focusable: input.enabled,
                value: Some(format!("{:.0}%", self.value * 100.0)),
                numeric: Some(NumericValue {
                    value: self.value * 100.0,
                    min: 0.0,
                    max: 100.0,
                    step: STEP * 100.0,
                }),
            },
            primitives,
            // Capturing keeps the keys and the wheel from the containers
            // above: without it, an arrow key would turn the dial and scroll
            // the page it sits on. Pointer events are heard, never kept, so
            // the framework still gives the dial focus, hover and press.
            input: EventArea {
                pointer: true,
                keyboard: true,
                scroll: true,
                capture: true,
                consume: true,
            },
        }
    }
}
