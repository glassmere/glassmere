//! Images: files decoded in the background (PNG, JPEG, WebP, GIF, SVG), fit
//! modes, tinted vector icons, an image made in code, and background images
//! with nine-slice frames from a stylesheet and from code. Edit or replace a
//! file under `examples/assets` while this runs and it reloads. The file card
//! at the bottom can be dragged out of the window, onto the desktop or into
//! another application.
use glassmere::{
    Align, App, AppOptions, BackgroundImage, Button, Color, DragOutcome, DragPayload,
    DragSourceBuilder, Effects, Image, ImageFit, ImageOptions, ImageTint, Justify, Label, Length,
    Size, Style, Ui, run,
};

const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets");

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

/// A 24 by 24 button skin: a lit top edge, a dark rim and rounded corners that
/// must survive stretching.
fn skin() -> Image {
    let size = 24i32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            // Distance into the rounded rectangle, for the rim and the corner cut.
            let (dx, dy) = (
                (x.min(size - 1 - x) - 7).min(0),
                (y.min(size - 1 - y) - 7).min(0),
            );
            let corner = ((dx * dx + dy * dy) as f32).sqrt();
            let rim = corner > 5.5
                || x.min(size - 1 - x) < 2 && dy == 0
                || y.min(size - 1 - y) < 2 && dx == 0;
            let pixel = if corner > 7.5 {
                [0, 0, 0, 0]
            } else if rim {
                [14, 60, 56, 255]
            } else if y < 5 {
                [74, 190, 176, 255]
            } else {
                [30, 132, 122, 255]
            };
            rgba.extend(pixel);
        }
    }
    Image::from_rgba8(size as u32, size as u32, rgba).expect("24 by 24 pixels")
}

fn caption<M: 'static>(ui: &mut Ui<'_, M>, key: &str, text: &str) {
    let style = Style {
        font_size: Some(12.0),
        foreground: Some(Color::rgb(153, 171, 193)),
        ..Style::default()
    };
    ui.keyed(key, |ui| ui.label(Label::new(text).style(style)));
}

#[derive(Clone)]
enum Msg {
    Like,
    /// How the file card's drag ended.
    Exported(DragOutcome),
}

struct Gallery {
    smoke: bool,
    made: Image,
    skin: Image,
    liked: bool,
    exported: &'static str,
}

impl App for Gallery {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        match message {
            Msg::Like => self.liked = !self.liked,
            Msg::Exported(DragOutcome::Dropped) => {
                self.exported = "Dropped outside the window.";
            }
            Msg::Exported(DragOutcome::Cancelled) => {
                self.exported = "The drag was cancelled.";
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        if self.smoke {
            ui.request_redraw();
        }
        let liked = self.liked;
        let page = Style {
            padding: 28.0,
            gap: 18.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        let panel = Style {
            padding: 12.0,
            radius: 10.0,
            background: Some(Color::rgb(24, 35, 49)),
            ..Style::default()
        };
        ui.column(page, |ui| {
            ui.label(Label::new("Images and assets").style(Style {
                font_size: Some(26.0),
                ..Style::default()
            }));

            caption(
                ui,
                "fits-caption",
                "One 320 by 200 file in four 120 by 120 boxes: contain, cover, fill, none",
            );
            ui.row(Style::default().gap(14.0), |ui| {
                for (key, fit) in [
                    ("contain", ImageFit::Contain),
                    ("cover", ImageFit::Cover),
                    ("fill", ImageFit::Fill),
                    ("none", ImageFit::None),
                ] {
                    let frame = Style {
                        width: Length::Px(120.0),
                        height: Length::Px(120.0),
                        radius: 12.0,
                        background: panel.background,
                        ..Style::default()
                    };
                    let options = ImageOptions::new(format!("Hills at dusk, {key}"))
                        .fit(fit)
                        .style(frame);
                    ui.keyed(key, |ui| {
                        ui.image_file(format!("{ASSETS}/dusk.png"), options)
                    });
                }
            });

            caption(
                ui,
                "icons-caption",
                "SVG icons take the text colour; the badge is one SVG at three sizes",
            );
            let icons = Style {
                gap: 16.0,
                align_items: Align::Center,
                ..Style::default()
            };
            ui.row(icons, |ui| {
                for name in ["search", "sliders"] {
                    let options = ImageOptions::default().tint(ImageTint::Foreground);
                    ui.keyed(name, |ui| {
                        ui.image_file(format!("{ASSETS}/{name}.svg"), options)
                    });
                }
                let heart = if liked {
                    ImageTint::Color(Color::rgb(240, 82, 110))
                } else {
                    ImageTint::Foreground
                };
                let description = if liked { "Liked" } else { "Not liked" };
                ui.image_file(
                    format!("{ASSETS}/heart.svg"),
                    ImageOptions::new(description).tint(heart),
                );
                ui.button(Button::new(if liked { "Unlike" } else { "Like" }).on_click(Msg::Like));
                for (key, side) in [
                    ("badge-small", 24.0),
                    ("badge", 64.0),
                    ("badge-large", 96.0),
                ] {
                    let options = ImageOptions::default().style(Style {
                        width: Length::Px(side),
                        ..Style::default()
                    });
                    ui.keyed(key, |ui| {
                        ui.image_file(format!("{ASSETS}/badge.svg"), options)
                    });
                }
            });

            caption(
                ui,
                "made-caption",
                "Pixels made in code, translucent over a panel; a file that does not exist",
            );
            ui.row(Style::default().gap(16.0), |ui| {
                let backing = Style {
                    background: Some(Color::rgb(22, 105, 99)),
                    ..panel.clone()
                };
                ui.container(backing, |ui| {
                    ui.image(
                        &self.made,
                        ImageOptions::new("Checkerboard").style(Style {
                            width: Length::Px(96.0),
                            radius: 8.0,
                            ..Style::default()
                        }),
                    );
                });
                let missing = ImageOptions::new("not-there.png is missing").style(Style {
                    width: Length::Px(220.0),
                    height: Length::Px(120.0),
                    background: None,
                    ..panel.clone()
                });
                ui.image_file(format!("{ASSETS}/not-there.png"), missing);
            });

            caption(
                ui,
                "backgrounds-caption",
                "Backgrounds: a stylesheet photo, one SVG frame nine-sliced over three boxes, a \
                 24 px skin made in code",
            );
            let backgrounds = Style {
                gap: 16.0,
                align_items: Align::Start,
                ..Style::default()
            };
            ui.row(backgrounds, |ui| {
                let hero = ui.class("hero");
                ui.column(hero, |ui| ui.label("Hills at dusk"));
                for (key, width, height) in [
                    ("small", 90.0, 56.0),
                    ("wide", 170.0, 56.0),
                    ("tall", 120.0, 150.0),
                ] {
                    let frame = Style {
                        width: Length::Px(width),
                        height: Length::Px(height),
                        ..ui.class("frame")
                    };
                    ui.keyed(key, |ui| ui.column(frame, |ui| ui.label(key)));
                }
                let skinned = Style {
                    width: Length::Px(170.0),
                    height: Length::Px(64.0),
                    align_items: Align::Center,
                    justify_content: Justify::Center,
                    ..Style::default()
                }
                .background_image(BackgroundImage::new(&self.skin).slice(8.0, 8.0, 8.0, 8.0));
                ui.column(skinned, |ui| ui.label("Raster skin"));
            });

            caption(ui, "export-caption", self.exported);
            let file = std::path::PathBuf::from(format!("{ASSETS}/dusk.png"));
            let source =
                DragSourceBuilder::new("dusk.png, draggable file", DragPayload::Files(vec![file]))
                    .on_drag_end(Msg::Exported);
            ui.drag_source(panel, source, |ui| {
                ui.label("dusk.png");
            });
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    run(
        AppOptions {
            title: "Glassmere / Images".into(),
            size: Size::new(980.0, 860.0),
            stylesheet: Some(format!("{ASSETS}/../images.style").into()),
            // Long enough for the background decodes to land in a smoke run.
            frame_limit: smoke.then_some(30),
            ..AppOptions::default()
        },
        Gallery {
            smoke,
            made: checker(),
            skin: skin(),
            liked: false,
            exported: "Drag the card out of the window to copy the file somewhere.",
        },
    )
}
