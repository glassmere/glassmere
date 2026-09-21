use crate::{Semantics, Ui};
use rust_gui_scene::{Primitive, Size};

/// Application-owned state, declared inside a stable parent/key scope.
/// Actions belong behind responses such as `clicked()`; declaration may repeat.
pub trait Component {
    fn ui(&mut self, ui: &mut Ui<'_>);
}

/// A leaf with explicit intrinsic size, local paint commands and semantics.
/// Button semantics opt into the same pointer, keyboard and accessibility
/// activation path as built-in buttons. Other roles are non-interactive.
/// Commands are clipped to the leaf; nested clip commands must be balanced.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomWidget {
    pub size: Size,
    pub semantics: Semantics,
    pub primitives: Vec<Primitive>,
}

impl CustomWidget {
    pub(crate) fn validate(&self) {
        assert!(
            self.size.width.is_finite()
                && self.size.height.is_finite()
                && self.size.width >= 0.0
                && self.size.height >= 0.0,
            "custom widget intrinsic size must be finite and nonnegative"
        );
        let mut depth = 0usize;
        let mut layers = 0usize;
        for primitive in &self.primitives {
            match primitive {
                Primitive::PushClip(_) => depth += 1,
                Primitive::PopClip => {
                    depth = depth.checked_sub(1).expect("custom widget clip underflow");
                }
                Primitive::PushLayer { .. } => layers += 1,
                Primitive::PopLayer => {
                    layers = layers
                        .checked_sub(1)
                        .expect("custom widget layer underflow");
                }
                _ => {}
            }
        }
        assert_eq!(depth, 0, "custom widget clips must be balanced");
        assert_eq!(layers, 0, "custom widget layers must be balanced");
    }
}
