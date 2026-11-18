//! Controls that show a state or a single choice and little else: progress,
//! busy indicators, switches, disclosure sections, links and range sliders.

use super::*;
use crate::app::Handler;
use crate::controls::slider_value;
use crate::{Length, TextWrap, Transition};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use core::ops::RangeInclusive;

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

    /// A busy indicator: dots that turn while the work goes on, still when
    /// motion is reduced. It is read as a progress bar without a value.
    #[track_caller]
    pub fn spinner(&mut self, spinner: impl Into<Spinner>) {
        let spinner = spinner.into();
        let key = self.next_key();
        self.push(Element {
            style: spinner.style.unwrap_or(Style {
                width: Length::Px(24.0),
                height: Length::Px(24.0),
                ..Style::default()
            }),
            enabled: self.enabled,
            control: Control::Spinner {
                label: spinner.label,
            },
            ..Element::new(key, Kind::Spinner)
        });
    }

    /// An on/off switch showing `on`: a click, Space or Enter sends
    /// `on_toggle(!on)`. The label is written beside it and names it.
    #[track_caller]
    pub fn switch(
        &mut self,
        switch: impl Into<Switch>,
        on: bool,
        on_toggle: impl Fn(bool) -> M,
    ) -> Response<'a> {
        let switch = switch.into();
        let key = self.next_key();
        let response = self.push(Element {
            text: switch.label.clone(),
            style: Style {
                wrap: TextWrap::None,
                ..switch.style.unwrap_or(Style {
                    transition: Some(Transition::new(120)),
                    ..Style::button()
                })
            },
            enabled: self.enabled,
            control: Control::Switch {
                label: switch.label,
                on,
            },
            ..Element::new(key, Kind::Switch)
        });
        self.set_handler(Handler::Message(on_toggle(!on)));
        response
    }

    /// A section whose header, a chevron and the label, opens and closes it:
    /// a click, Enter, Space, or Right to open and Left to close, send
    /// `on_toggle` of the state they ask for. `content` is declared only
    /// while `open`, so the application owns the state as it does for
    /// [`Ui::tabs`]. The header is a button that reports whether it is
    /// expanded.
    #[track_caller]
    pub fn collapsible(
        &mut self,
        section: impl Into<Collapsible>,
        open: bool,
        on_toggle: impl Fn(bool) -> M + 'static,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        let section = section.into();
        let key = self.next_key();
        let message = on_toggle(!open);
        self.group(key, Kind::Column, Style::default().gap(4.0), |ui| {
            ui.keyed("$header", |ui| {
                let key = ui.next_key();
                ui.push(Element {
                    text: section.label.clone(),
                    style: Style {
                        wrap: TextWrap::None,
                        ..section.style.unwrap_or(Style {
                            padding: 8.0,
                            radius: 6.0,
                            ..Style::default()
                        })
                    },
                    enabled: ui.enabled,
                    control: Control::Disclosure {
                        label: section.label,
                        open,
                    },
                    ..Element::new(key, Kind::Disclosure)
                });
                ui.set_handler(Handler::Message(message));
            });
            if open {
                ui.keyed("$content", |ui| {
                    ui.column(Style::default().padding(8.0).gap(8.0), content);
                });
            }
        });
    }

    /// A link: accent-coloured, underlined text with the pointing hand over
    /// it, which Tab reaches and Enter activates. `ui.link("Documentation")`
    /// is the whole of it, and [`Link::new`] carries a style, a disabled
    /// state, and with [`Link::on_click`] the message a click sends.
    #[track_caller]
    pub fn link(&mut self, link: impl Into<Link<M>>) -> Response<'a> {
        let link = link.into();
        let key = self.next_key();
        let style = link.style.unwrap_or(Style {
            padding: 3.0,
            radius: 3.0,
            ..Style::default()
        });
        let response = self.leaf(
            key,
            Kind::Link,
            link.text,
            Style {
                wrap: TextWrap::None,
                ..style
            },
            link.enabled,
        );
        if let Some(message) = link.on_click {
            self.set_handler(Handler::Message(message));
        }
        response
    }

    /// A slider with two thumbs for the ends of `value` over `range`: a drag
    /// or a key on either sends `on_change` of the whole range it reaches,
    /// snapped to `step`. The thumbs never cross, and each is read as a
    /// slider of its own, named "minimum" or "maximum" after the label. A
    /// `value` outside `range` or in the wrong order is shown clamped and
    /// ordered.
    #[track_caller]
    pub fn range_slider(
        &mut self,
        slider: impl Into<RangeSlider>,
        value: RangeInclusive<f64>,
        range: RangeInclusive<f64>,
        step: f64,
        on_change: impl Fn(RangeInclusive<f64>) -> M + 'static,
    ) -> Response<'a> {
        let (min, max) = (*range.start(), *range.end());
        if !(min.is_finite() && max.is_finite() && min < max && step.is_finite() && step > 0.0) {
            self.misuse(
                "range_slider requires finite ordered bounds and a positive step".to_owned(),
            );
            return Response::default();
        }
        let slider = slider.into();
        let key = self.next_key();
        let mut low = slider_value(*value.start(), min, max, step);
        let mut high = slider_value(*value.end(), min, max, step);
        if low > high {
            core::mem::swap(&mut low, &mut high);
        }
        let on_change = alloc::rc::Rc::new(on_change);
        let parent = self.context.child(self.parent, &key, Kind::RangeSlider);
        let mut inner = self.scope(parent);
        // Each thumb can reach as far as the other, and covers the whole box
        // so that the track is one place to press.
        for (name, message, thumb, reach, is_low) in [
            (
                "$minimum",
                "glassmere-range-minimum",
                low,
                (min, high),
                true,
            ),
            (
                "$maximum",
                "glassmere-range-maximum",
                high,
                (low, max),
                false,
            ),
        ] {
            inner.keyed(name, |ui| {
                let key = ui.next_key();
                let label = ui.tr_args(message, &[("name", slider.label.as_str().into())]);
                ui.push(Element {
                    style: Style {
                        width: Length::Percent(1.0),
                        height: Length::Percent(1.0),
                        ..Style::default().offset(0.0, 0.0)
                    },
                    enabled: ui.enabled,
                    control: Control::Slider {
                        label,
                        value: thumb,
                        min: reach.0,
                        max: reach.1,
                        step,
                        span: Some((min, max)),
                    },
                    ..Element::new(key, Kind::Slider)
                });
                let on_change = alloc::rc::Rc::clone(&on_change);
                ui.set_handler(Handler::Map(Box::new(move |update| match update {
                    ControlUpdate::Number(number) => {
                        let number = slider_value(number, reach.0, reach.1, step);
                        Some(on_change(if is_low {
                            number..=reach.1
                        } else {
                            reach.0..=number
                        }))
                    }
                    _ => None,
                })));
            });
        }
        let children = inner.elements;
        self.push(Element {
            text: format!("{}: {low:.2} \u{2013} {high:.2}", slider.label),
            style: slider.style.unwrap_or(Style {
                width: Length::Px(280.0),
                height: Length::Px(64.0),
                padding: 10.0,
                ..Style::button()
            }),
            enabled: self.enabled,
            control: Control::RangeSlider {
                label: slider.label,
            },
            children,
            ..Element::new(key, Kind::RangeSlider)
        })
    }
}
