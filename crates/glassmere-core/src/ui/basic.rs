//! Controls that show a state or a single choice and little else: progress,
//! busy indicators, switches, disclosure sections, links and range sliders.

use super::*;
use crate::Length;

impl<'a, M: 'static> Ui<'a, M> {
    /// A progress bar: `Some(fraction)` fills that share of it, clamped to
    /// `0.0..=1.0`, and `None` shows a segment that keeps moving, for work
    /// that cannot say how far it is. It is read as a progress bar with a
    /// percentage in the first case and without a value in the second.
    #[track_caller]
    pub fn progress(&mut self, progress: impl Into<Progress>, value: Option<f64>) {
        let progress = progress.into();
        let key = self.next_key();
        let value = value.map(|fraction| {
            if fraction.is_nan() {
                0.0
            } else {
                fraction.clamp(0.0, 1.0)
            }
        });
        self.push(Element {
            style: progress.style.unwrap_or(Style {
                width: Length::Px(280.0),
                height: Length::Px(8.0),
                radius: 4.0,
                ..Style::default()
            }),
            enabled: self.enabled,
            control: Control::Progress {
                label: progress.label,
                value,
            },
            ..Element::new(key, Kind::Progress)
        });
    }
}
