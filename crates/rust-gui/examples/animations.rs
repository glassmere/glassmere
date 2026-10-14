use rust_gui::{
    Animation, AppOptions, Button, Color, DialogOptions, Easing, Effect, Keyframe, Label, Length,
    Paint, Repeat, Size, Style, TextWrap, Transform, Transition, run,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let open = std::env::args().any(|arg| arg == "--open");
    let reduced_motion = std::env::args().any(|arg| arg == "--reduced-motion");
    let mut toasts: Vec<u32> = if open { vec![1, 2, 3] } else { vec![1] };
    let mut next_toast = 4u32;
    let mut dialog = false;
    let mut expanded = open;
    run(
        AppOptions {
            title: "Rust GUI / Animations".into(),
            size: Size::new(900.0, 760.0),
            frame_limit: smoke.then_some(3),
            reduced_motion,
            ..AppOptions::default()
        },
        move |ui| {
            let theme = ui.theme().clone();
            let heading = Style::default().font_weight(700).font_size(18.0);
            let muted = Style::default()
                .font_size(13.0)
                .foreground(theme.text_muted)
                .wrap(TextWrap::Word)
                .width(Length::Percent(1.0));
            let page = Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Percent(1.0))
                .padding(24.0)
                .gap(14.0);
            ui.column(page, |ui| {
                ui.label(Label::new("State transitions").style(heading.clone()));
                ui.label(Label::new("Hover and press: colours, opacity and transforms ease between states. \
                     Transforms are visual only, so hit testing keeps the layout bounds.").style(muted.clone()));
                // Ancestors clip to their bounds, so rows leave room for transformed pixels.
                ui.row(Style::default().gap(12.0).padding(8.0), |ui| {
                    let ease = Transition::new(160);
                    ui.button(Button::new("Colour").style(Style::button()
                            .background(theme.accent)
                            .foreground(Color(255, 255, 255, 255))
                            .transition(ease)
                            .on_hover(Paint::background(Color(232, 93, 117, 255)))));
                    ui.button(Button::new("Lift").style(Style::button()
                            .transition(ease)
                            .on_hover(Paint::transform(Transform {
                                translate: (0.0, -3.0),
                                scale: (1.06, 1.06),
                                rotate: 0.0,
                            }))
                            .on_pressed(Paint::transform(Transform::scale(0.94)))));
                    ui.button(Button::new("Fade").style(Style::button()
                            .transition(Transition::new(300).easing(Easing::EaseInOut))
                            .on_hover(Paint::opacity(0.35))));
                    ui.button(Button::new("Tilt").style(Style::button()
                            .transition(
                                Transition::new(260)
                                    .easing(Easing::CubicBezier(0.3, 1.8, 0.6, 1.0)),
                            )
                            .on_hover(Paint::transform(Transform::rotate(-6.0)))));
                });

                ui.label(Label::new("Keyframes and static layers").style(heading.clone()));
                ui.row(Style::default()
                        .gap(24.0)
                        .padding(16.0)
                        .align_items(rust_gui::Align::Center), |ui| {
                        let tile = Style::default()
                            .width(Length::Px(44.0))
                            .height(Length::Px(44.0))
                            .radius(8.0);
                        ui.label(Label::new("").style(tile.clone()
                                .background(theme.accent)
                                .animation(Animation::spin(1600))));
                        ui.label(Label::new("").style(tile.clone()
                                .radius(22.0)
                                .background(Color(232, 93, 117, 255))
                                .animation(Animation::pulse(0.25, 900))));
                        ui.label(Label::new("").style(tile.clone().background(Color(240, 180, 40, 255)).animation(
                                Animation::new(
                                    vec![
                                        Keyframe::at(0.0).transform(Transform::translate(0.0, 0.0)),
                                        Keyframe::at(0.5)
                                            .transform(Transform::translate(0.0, -14.0))
                                            .background(Color(250, 120, 40, 255)),
                                        Keyframe::at(1.0).transform(Transform::translate(0.0, 0.0)),
                                    ],
                                    700,
                                )
                                .easing(Easing::EaseInOut)
                                .repeat(Repeat::Forever),
                            )));
                        // A resting layer: group opacity and rotation, text included.
                        ui.container(Style::default()
                                .padding(12.0)
                                .radius(8.0)
                                .background(theme.surface)
                                .opacity(0.8)
                                .transform(Transform::rotate(-4.0)), |ui| {
                                ui.label("Rotated group at 80% opacity");
                            });
                    });

                ui.label(Label::new("Enter, exit and layout").style(heading.clone()));
                ui.row(Style::default().gap(8.0), |ui| {
                    if ui.button("Add toast").clicked() {
                        toasts.insert(0, next_toast);
                        next_toast += 1;
                    }
                    if ui.button("Remove first").clicked() && !toasts.is_empty() {
                        toasts.remove(0);
                    }
                    if ui.button("Rotate order").clicked() && toasts.len() > 1 {
                        toasts.rotate_left(1);
                    }
                    if ui.button(if expanded { "Collapse" } else { "Expand" })
                        .clicked()
                    {
                        expanded = !expanded;
                    }
                    if ui.button("Open dialog").clicked() {
                        dialog = true;
                    }
                });
                if expanded {
                    ui.label(Label::new("This block enters with a slide and pushes the toasts down; they follow \
                         with a layout transition instead of jumping.").style(muted
                            .clone()
                            .enter(Effect::slide(0.0, -8.0, 180))
                            .exit(Effect::fade(120))));
                }
                let mut dismissed = None;
                for toast in &toasts {
                    ui.keyed(format!("toast-{toast}"), |ui| ui.row(Style::default()
                            .padding(10.0)
                            .gap(12.0)
                            .radius(8.0)
                            .width(Length::Px(360.0))
                            .background(theme.surface)
                            .align_items(rust_gui::Align::Center)
                            .layout_transition(Transition::new(220).easing(Easing::EaseInOut))
                            .enter(Effect::slide(-24.0, 0.0, 220))
                            .exit(Effect::slide(40.0, 0.0, 200)), |ui| {
                            ui.label(Label::new(format!("Toast #{toast}")).style(Style::default().grow(1.0)));
                            if ui.button("Dismiss").clicked() {
                                dismissed = Some(*toast);
                            }
                        }));
                }
                if let Some(toast) = dismissed {
                    toasts.retain(|other| *other != toast);
                }
            });
            ui.dialog(
                "animated",
                &mut dialog,
                DialogOptions::new("Animated dialog").style(
                    Style::default()
                        .enter(Effect::zoom(0.92, 160))
                        .exit(Effect::zoom(0.96, 120)),
                ),
                |ui, open| {
                    ui.label("The panel zooms in, and fades out after it is removed.");
                    if ui.button("Close").clicked() {
                        *open = false;
                    }
                },
            );
        },
    )
}
