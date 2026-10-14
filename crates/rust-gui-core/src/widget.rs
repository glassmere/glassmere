use crate::{EventArea, Semantics, Ui, UiEvent};
use rust_gui_scene::{Point, Primitive, Rect, Size};

/// Application-owned state, declared inside a stable parent/key scope.
/// Actions belong behind responses such as `clicked()`; declaration may repeat.
pub trait Component {
    fn ui(&mut self, ui: &mut Ui<'_>);
}

/// A leaf with explicit intrinsic size, local paint commands and semantics.
/// Focusable semantics put it in the tab order and give it the keyboard,
/// whatever its role; button semantics also opt into the pointer and
/// accessibility activation path of built-in buttons. `input` is what it
/// hears of the events that reach it, which `Ui::widget` hands back in the
/// widget's own coordinates on the next declaration.
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
    /// In the order they arrived, with pointer positions relative to the
    /// widget's own top left corner.
    pub events: Vec<UiEvent>,
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
