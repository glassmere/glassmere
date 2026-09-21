//! Images: files decoded in the background (PNG, JPEG, WebP, GIF, SVG), fit
//! modes, tinted vector icons and an image made in code. Edit or replace a
//! file under `examples/assets` while this runs and it reloads.
use rust_gui::{
    AppOptions, Color, Image, ImageFit, ImageOptions, ImageTint, Length, Style, Ui, run,
};

const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets");

/// A checkerboard with a transparent half, to show that alpha blends.
fn checker() -> Image {
    let size = 64u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let light = (x / 8 + y / 8) % 2 == 0;
            let alpha = if x + y < size { 255 } else { 90 };
            let shade = if light { 235 } else { 90 };
            rgba.extend([shade, shade, 255, alpha]);
        }
    }
    Image::from_rgba8(size, size, rgba).expect("64 by 64 pixels")
}

fn caption(ui: &mut Ui<'_>, key: &str, text: &str) {
    let style = Style {
        font_size: Some(12.0),
        foreground: Some(Color::rgb(153, 171, 193)),
        ..Style::default()
    };
    ui.label_styled(key, text, style);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let made = checker();
    let mut liked = false;
    run(
        AppOptions {
            title: "Rust GUI / Images".into(),
            // Long enough for the background decodes to land in a smoke run.
            frame_limit: smoke.then_some(30),
            ..AppOptions::default()
        },
        move |ui| {
            if smoke {
                ui.request_redraw();
            }
            let page = Style {
                padding: 28.0,
                gap: 18.0,
                width: Length::Percent(1.0),
                ..Style::default()
            };
            ui.column("page", page, |ui| {
                ui.label_styled(
                    "title",
                    "Images and assets",
                    Style {
                        font_size: Some(26.0),
                        ..Style::default()
                    },
                );

                caption(
                    ui,
                    "fits-caption",
                    "One 320 by 200 file in four 150 by 150 boxes: contain, cover, fill, none",
                );
                ui.row(
                    "fits",
                    Style {
                        gap: 14.0,
                        ..Style::default()
                    },
                    |ui| {
                        for (key, fit) in [
                            ("contain", ImageFit::Contain),
                            ("cover", ImageFit::Cover),
                            ("fill", ImageFit::Fill),
                            ("none", ImageFit::None),
                        ] {
                            let frame = Style {
                                width: Length::Px(150.0),
                                height: Length::Px(150.0),
                                radius: 12.0,
                                background: Some(Color::rgb(24, 35, 49)),
                                ..Style::default()
                            };
                            ui.image_file(
                                key,
                                format!("{ASSETS}/dusk.png"),
                                ImageOptions::new(format!("Hills at dusk, {key}"))
                                    .fit(fit)
                                    .style(frame),
                            );
                        }
                    },
                );

                caption(
                    ui,
                    "icons-caption",
                    "SVG icons take the text colour; the badge is one SVG at three sizes",
                );
                ui.row(
                    "icons",
                    Style {
                        gap: 16.0,
                        align_items: rust_gui::Align::Center,
                        ..Style::default()
                    },
                    |ui| {
                        for name in ["search", "sliders"] {
                            ui.image_file(
                                name,
                                format!("{ASSETS}/{name}.svg"),
                                ImageOptions::default().tint(ImageTint::Foreground),
                            );
                        }
                        let heart = if liked {
                            ImageTint::Color(Color::rgb(240, 82, 110))
                        } else {
                            ImageTint::Foreground
                        };
                        ui.image_file(
                            "heart",
                            format!("{ASSETS}/heart.svg"),
                            ImageOptions::new(if liked { "Liked" } else { "Not liked" })
                                .tint(heart),
                        );
                        if ui
                            .button_keyed("like", if liked { "Unlike" } else { "Like" })
                            .clicked()
                        {
                            liked = !liked;
                        }
                        for (key, side) in [
                            ("badge-small", 24.0),
                            ("badge", 64.0),
                            ("badge-large", 128.0),
                        ] {
                            ui.image_file(
                                key,
                                format!("{ASSETS}/badge.svg"),
                                ImageOptions::default().style(Style {
                                    width: Length::Px(side),
                                    ..Style::default()
                                }),
                            );
                        }
                    },
                );

                caption(
                    ui,
                    "made-caption",
                    "Pixels made in code, translucent over a panel; a file that does not exist",
                );
                ui.row(
                    "made",
                    Style {
                        gap: 16.0,
                        ..Style::default()
                    },
                    |ui| {
                        ui.container(
                            "panel",
                            Style {
                                padding: 12.0,
                                radius: 10.0,
                                background: Some(Color::rgb(22, 105, 99)),
                                ..Style::default()
                            },
                            |ui| {
                                ui.image(
                                    "checker",
                                    &made,
                                    ImageOptions::new("Checkerboard").style(Style {
                                        width: Length::Px(128.0),
                                        radius: 8.0,
                                        ..Style::default()
                                    }),
                                );
                            },
                        );
                        ui.image_file(
                            "missing",
                            format!("{ASSETS}/not-there.png"),
                            ImageOptions::new("not-there.png is missing").style(Style {
                                width: Length::Px(220.0),
                                height: Length::Px(152.0),
                                padding: 12.0,
                                radius: 10.0,
                                ..Style::default()
                            }),
                        );
                    },
                );
            });
        },
    )
}
