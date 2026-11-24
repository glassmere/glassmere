use crate::{EventArea, Semantics, UiEvent};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use glassmere_scene::{Point, Primitive, Rect, Size};

/// A leaf with explicit intrinsic size, local paint commands and semantics.
/// Focusable semantics put it in the tab order and give it the keyboard,
/// whatever its role; the roles a user presses (`Button`, `Checkbox`,
/// `Radio`, `Tab`, `MenuButton`, `MenuItem` and `Option`) also opt into the
/// pointer and accessibility activation path of built-in buttons, and
/// `Semantics::checked`, `selected` and `expanded` are what a screen reader
/// is told of its state. `input` is what it
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
    pub(crate) on_click: Option<M>,
}

impl<F, M> core::fmt::Debug for WidgetBuilder<F, M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WidgetBuilder").finish_non_exhaustive()
    }
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
            on_click: None,
        }
    }

    /// The message sent when the widget is clicked, or activated with Enter
    /// or Space or an assistive technology's Activate. A click is a press
    /// that began on the widget and is released on it: drag away and let go
    /// and nothing is sent, as with a web page's `click` and a stock button.
    /// The widget needs a role that is pressed, `Button`, `Checkbox`,
    /// `Radio`, `Tab`, `MenuButton`, `MenuItem` or `Option`, and
    /// `Semantics::focusable` to be reached by the keyboard. The click is
    /// not repeated to `on_event` as `Activate`; the pointer events still
    /// are.
    pub fn on_click(mut self, message: M) -> Self {
        self.on_click = Some(message);
        self
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
