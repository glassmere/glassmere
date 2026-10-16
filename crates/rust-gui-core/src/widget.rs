use crate::{EventArea, Semantics, UiEvent};
use rust_gui_scene::{Point, Primitive, Rect, Size};

/// A leaf with explicit intrinsic size, local paint commands and semantics.
/// Focusable semantics put it in the tab order and give it the keyboard,
/// whatever its role; button semantics also opt into the pointer and
/// accessibility activation path of built-in buttons. `input` is what it
/// hears of the events that reach it, which `Ui::widget` hands to its
/// `on_event` in the widget's own coordinates.
/// Commands are clipped to the leaf; nested clip commands must be balanced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomWidget {
    pub size: Size,
    pub semantics: Semantics,
    pub primitives: Vec<Primitive>,
    pub input: EventArea,
}

/// What a widget of the application's own is told before it paints: the room
/// it was given, how it is being touched, and what has reached it since it
/// last painted. Everything here is of the last frame, because that is what
/// there is to know when this one is being declared.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WidgetInput {
    /// What the node was laid out at, and zero on the frame it appears.
    pub size: Size,
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    /// False inside a disabled scope, so the widget can paint itself so.
    pub enabled: bool,
}

/// A custom widget declared with `Ui::widget`: how it is built, and the
/// message its events send. Without [`WidgetBuilder::on_event`] it sends
/// nothing.
pub struct WidgetBuilder<F, M = ()> {
    pub(crate) build: F,
    pub(crate) on_event: Option<Box<dyn Fn(crate::UiEvent) -> Option<M>>>,
}

impl<F, M> WidgetBuilder<F, M> {
    /// `build` is called with what the widget was laid out at and how it is
    /// being touched.
    pub fn new(build: F) -> Self
    where
        F: FnOnce(&WidgetInput) -> CustomWidget,
    {
        Self {
            build,
            on_event: None,
        }
    }

    /// The message an event that reaches the widget sends, or none for an
    /// event it does not take. Pointer positions are in the widget's own
    /// coordinates, from its top left corner. A disabled widget hears nothing.
    pub fn on_event(mut self, on_event: impl Fn(crate::UiEvent) -> Option<M> + 'static) -> Self {
        self.on_event = Some(Box::new(on_event));
        self
    }
}

impl WidgetInput {
    /// Moves an event into the widget's own coordinates.
    pub(crate) fn local(event: UiEvent, bounds: Rect) -> UiEvent {
        let shift = |at: Point| Point {
            x: at.x - bounds.x,
            y: at.y - bounds.y,
        };
        match event {
            UiEvent::PointerDown(at) => UiEvent::PointerDown(shift(at)),
            UiEvent::PointerUp(at) => UiEvent::PointerUp(shift(at)),
            UiEvent::PointerMoved(at) => UiEvent::PointerMoved(shift(at)),
            other => other,
        }
    }
}

impl CustomWidget {
    /// Why the widget cannot be drawn, if it cannot.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !(self.size.width.is_finite()
            && self.size.height.is_finite()
            && self.size.width >= 0.0
            && self.size.height >= 0.0)
        {
            return Err("custom widget intrinsic size must be finite and nonnegative".to_owned());
        }
        let mut depth = 0usize;
        let mut layers = 0usize;
        for primitive in &self.primitives {
            match primitive {
                Primitive::PushClip(_) => depth += 1,
                Primitive::PopClip => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or("custom widget pops a clip it never pushed")?;
                }
                Primitive::PushLayer { .. } => layers += 1,
                Primitive::PopLayer => {
                    layers = layers
                        .checked_sub(1)
                        .ok_or("custom widget pops a layer it never pushed")?;
                }
                _ => {}
            }
        }
        if depth != 0 {
            return Err("custom widget clips must be balanced".to_owned());
        }
        if layers != 0 {
            return Err("custom widget layers must be balanced".to_owned());
        }
        Ok(())
    }
}
