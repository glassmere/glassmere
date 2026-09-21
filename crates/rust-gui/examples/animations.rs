use rust_gui::{
    Animation, AppOptions, Color, DialogOptions, Easing, Effect, Keyframe, Length, Paint, Repeat,
    Size, Style, TextWrap, Transform, Transition, run,
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
            ui.column("page", page, |ui| {
                ui.label_styled("h-states", "State transitions", heading.clone());
                ui.label_styled(
                    "p-states",
                    "Hover and press: colours, opacity and transforms ease between states. \
                     Transforms are visual only, so hit testing keeps the layout bounds.",
                    muted.clone(),
                );
                // Ancestors clip to their bounds, so rows leave room for transformed pixels.
                ui.row("states", Style::default().gap(12.0).padding(8.0), |ui| {
                    let ease = Transition::new(160);
                    ui.button_styled(
                        "colour",
                        "Colour",
                        true,
                        Style::button()
                            .background(theme.accent)
                            .foreground(Color(255, 255, 255, 255))
                            .transition(ease)
                            .on_hover(Paint::background(Color(232, 93, 117, 255))),
                    );
                    ui.button_styled(
                        "lift",
                        "Lift",
                        true,
                        Style::button()
                            .transition(ease)
                            .on_hover(Paint::transform(Transform {
                                translate: (0.0, -3.0),
                                scale: (1.06, 1.06),
                                rotate: 0.0,
                            }))
                            .on_pressed(Paint::transform(Transform::scale(0.94))),
                    );
                    ui.button_styled(
                        "fade",
                        "Fade",
                        true,
                        Style::button()
                            .transition(Transition::new(300).easing(Easing::EaseInOut))
                            .on_hover(Paint::opacity(0.35)),
                    );
                    ui.button_styled(
                        "tilt",
                        "Tilt",
                        true,
                        Style::button()
                            .transition(
                                Transition::new(260)
                                    .easing(Easing::CubicBezier(0.3, 1.8, 0.6, 1.0)),
                            )
                            .on_hover(Paint::transform(Transform::rotate(-6.0))),
                    );
                });

                ui.label_styled(
                    "h-keyframes",
                    "Keyframes and static layers",
                    heading.clone(),
                );
                ui.row(
                    "keyframes",
                    Style::default()
                        .gap(24.0)
                        .padding(16.0)
                        .align_items(rust_gui::Align::Center),
                    |ui| {
                        let tile = Style::default()
                            .width(Length::Px(44.0))
                            .height(Length::Px(44.0))
                            .radius(8.0);
                        ui.label_styled(
                            "spinner",
                            "",
                            tile.clone()
                                .background(theme.accent)
                                .animation(Animation::spin(1600)),
                        );
                        ui.label_styled(
                            "pulse",
                            "",
                            tile.clone()
                                .radius(22.0)
                                .background(Color(232, 93, 117, 255))
                                .animation(Animation::pulse(0.25, 900)),
                        );
                        ui.label_styled(
                            "bounce",
                            "",
                            tile.clone().background(Color(240, 180, 40, 255)).animation(
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
                            ),
                        );
                        // A resting layer: group opacity and rotation, text included.
                        ui.container(
                            "static",
                            Style::default()
                                .padding(12.0)
                                .radius(8.0)
                                .background(theme.surface)
                                .opacity(0.8)
                                .transform(Transform::rotate(-4.0)),
                            |ui| {
                                ui.label("Rotated group at 80% opacity");
                            },
                        );
                    },
                );

                ui.label_styled("h-lists", "Enter, exit and layout", heading.clone());
                ui.row("list-actions", Style::default().gap(8.0), |ui| {
                    if ui.button_keyed("add", "Add toast").clicked() {
                        toasts.insert(0, next_toast);
                        next_toast += 1;
                    }
                    if ui.button_keyed("remove", "Remove first").clicked() && !toasts.is_empty() {
                        toasts.remove(0);
                    }
                    if ui.button_keyed("rotate", "Rotate order").clicked() && toasts.len() > 1 {
                        toasts.rotate_left(1);
                    }
                    if ui
                        .button_keyed("expand", if expanded { "Collapse" } else { "Expand" })
                        .clicked()
                    {
                        expanded = !expanded;
                    }
                    if ui.button_keyed("dialog", "Open dialog").clicked() {
                        dialog = true;
                    }
                });
                if expanded {
                    ui.label_styled(
                        "details",
                        "This block enters with a slide and pushes the toasts down; they follow \
                         with a layout transition instead of jumping.",
                        muted
                            .clone()
                            .enter(Effect::slide(0.0, -8.0, 180))
                            .exit(Effect::fade(120)),
                    );
                }
                let mut dismissed = None;
                for toast in &toasts {
                    ui.row(
                        format!("toast-{toast}"),
                        Style::default()
                            .padding(10.0)
                            .gap(12.0)
                            .radius(8.0)
                            .width(Length::Px(360.0))
                            .background(theme.surface)
                            .align_items(rust_gui::Align::Center)
                            .layout_transition(Transition::new(220).easing(Easing::EaseInOut))
                            .enter(Effect::slide(-24.0, 0.0, 220))
                            .exit(Effect::slide(40.0, 0.0, 200)),
                        |ui| {
                            ui.label_styled(
                                "text",
                                format!("Toast #{toast}"),
                                Style::default().grow(1.0),
                            );
                            if ui.button_keyed("dismiss", "Dismiss").clicked() {
                                dismissed = Some(*toast);
                            }
                        },
                    );
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
                    if ui.button_keyed("close", "Close").clicked() {
                        *open = false;
                    }
                },
            );
        },
    )
}
