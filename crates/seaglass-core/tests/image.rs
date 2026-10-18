use seaglass_core::{
    BackgroundImage, Context, ImageFit, ImageOptions, ImageSource, ImageTint, Length,
    MonospaceMeasurer, Role, Style, StyleSheet,
};
use seaglass_scene::{Color, Image, Primitive, Rect, Size};
use std::path::Path;

const ROOT: Size = Size::new(600.0, 400.0);

fn picture(width: u32, height: u32) -> Image {
    Image::from_rgba8(width, height, vec![255; (width * height * 4) as usize]).unwrap()
}

fn drawn(context: &Context) -> Vec<(Rect, Rect, Option<Color>, f32)> {
    context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Image {
                rect,
                source,
                tint,
                radius,
                ..
            } => Some((*rect, *source, *tint, *radius)),
            _ => None,
        })
        .collect()
}

fn bounds(context: &Context, key: &str) -> Rect {
    let path: Vec<&str> = key.split('/').collect();
    context.node(context.find(&path).unwrap()).unwrap().bounds
}

#[test]
fn images_take_their_natural_size_follow_one_given_side_and_fit_a_given_box() {
    let mut context = Context::new();
    let image = picture(200, 100);
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        ui.keyed("row", |ui| {
            ui.row(Style::default(), |ui| {
                ui.keyed("natural", |ui| {
                    ui.image(&image, ImageOptions::new("A landscape"))
                });
                ui.keyed("wide", |ui| {
                    ui.image(
                        &image,
                        ImageOptions::new("").style(Style::default().width(Length::Px(100.0))),
                    )
                });
                ui.keyed("cover", |ui| {
                    ui.image(
                        &image,
                        ImageOptions::new("")
                            .fit(ImageFit::Cover)
                            .tint(ImageTint::Color(Color(1, 2, 3, 255)))
                            .style(
                                Style::default()
                                    .width(Length::Px(50.0))
                                    .height(Length::Px(50.0))
                                    .radius(8.0),
                            ),
                    )
                });
            })
        });
    });
    let natural = bounds(&context, "row/natural");
    assert_eq!((natural.width, natural.height), (200.0, 100.0));
    let wide = bounds(&context, "row/wide");
    assert_eq!((wide.width, wide.height), (100.0, 50.0));

    let images = drawn(&context);
    assert_eq!(images.len(), 3);
    let full = Rect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 100.0,
    };
    assert_eq!((images[0].0, images[0].1), (natural, full));
    assert_eq!((images[1].0, images[1].1, images[1].2), (wide, full, None));
    // Cover crops the sides of the wide image and keeps the box.
    let cover = bounds(&context, "row/cover");
    assert_eq!(images[2].0, cover);
    assert_eq!(
        images[2].1,
        Rect {
            x: 50.0,
            y: 0.0,
            width: 100.0,
            height: 100.0
        }
    );
    assert_eq!(images[2].2, Some(Color(1, 2, 3, 255)));
    assert_eq!(images[2].3, 8.0);

    // A named image is an image to assistive technology; an unnamed one is decoration.
    let named = context
        .node(context.find(&["row", "natural"]).unwrap())
        .unwrap();
    assert_eq!(named.semantics.role, Role::Image);
    assert_eq!(named.semantics.label, "A landscape");
    assert!(!named.semantics.focusable);
    let plain = context
        .node(context.find(&["row", "wide"]).unwrap())
        .unwrap();
    assert_eq!(plain.semantics.role, Role::Group);
}

#[test]
fn a_foreground_tint_follows_the_inherited_text_colour() {
    let mut context = Context::new();
    let icon = picture(16, 16);
    let red = Color(200, 0, 0, 255);
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        ui.keyed("tinted", |ui| {
            ui.column(Style::default().foreground(red), |ui| {
                ui.keyed("icon", |ui| {
                    ui.image(&icon, ImageOptions::new("").tint(ImageTint::Foreground))
                });
            })
        });
    });
    assert_eq!(drawn(&context)[0].2, Some(red));
}

#[test]
fn image_files_show_a_placeholder_until_the_host_delivers_and_keep_the_last_good_image() {
    let mut context = Context::new();
    let path = Path::new("assets/photo.png");
    let frame = |context: &mut Context| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            ui.keyed("photo", |ui| {
                ui.image_file(
                    path,
                    ImageOptions::new("Photo").style(
                        Style::default()
                            .width(Length::Px(120.0))
                            .height(Length::Px(80.0)),
                    ),
                )
            });
        })
    };
    frame(&mut context);
    assert_eq!(context.image_paths(), [path.to_owned()]);
    assert!(!context.image_known(path));
    assert!(drawn(&context).is_empty());
    let placeholder = bounds(&context, "photo");
    assert!(context.scene().primitives.iter().any(
        |primitive| matches!(primitive, Primitive::Quad { rect, .. } if *rect == placeholder)
    ));

    // A failure before any success: still a placeholder, labelled with the name.
    assert!(context.set_image_file(path, Err("photo.png: not found".into())));
    assert!(!context.set_image_file(path, Err("photo.png: not found".into())));
    frame(&mut context);
    assert_eq!(context.image_error(path), Some("photo.png: not found"));
    assert!(context.scene().primitives.iter().any(|primitive| matches!(
        primitive,
        Primitive::StyledText { text, .. } | Primitive::Text { text, .. } if text == "Photo"
    )));

    assert!(context.set_image_file(path, Ok(picture(60, 40))));
    frame(&mut context);
    assert!(context.image_known(path) && context.image_error(path).is_none());
    assert_eq!(drawn(&context)[0].0, placeholder);

    // A reload that fails (a half-written file) keeps what was on screen.
    assert!(context.set_image_file(path, Err("photo.png: truncated".into())));
    frame(&mut context);
    assert_eq!(drawn(&context).len(), 1);
    assert_eq!(context.image_error(path), Some("photo.png: truncated"));
}

#[test]
fn a_file_image_sizes_itself_once_loaded_and_unused_images_are_dropped_past_a_budget() {
    let mut context = Context::new();
    let shown = std::cell::Cell::new(true);
    let big = Path::new("big.png");
    let frame = |context: &mut Context| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            if shown.get() {
                ui.keyed("big", |ui| ui.image_file(big, ImageOptions::default()));
            }
            ui.keyed("small", |ui| {
                ui.image_file("small.png", ImageOptions::default())
            });
        })
    };
    frame(&mut context);
    assert_eq!(bounds(&context, "big").height, 0.0);
    // 4100 x 4100 x 4 bytes is just over the 64 MiB kept for unused images.
    context.set_image_file(big, Ok(picture(4100, 4100)));
    context.set_image_file(Path::new("small.png"), Ok(picture(8, 8)));
    frame(&mut context);
    assert_eq!(bounds(&context, "small").width, 8.0);
    assert!(context.image_known(big));
    shown.set(false);
    frame(&mut context);
    assert!(!context.image_known(big));
    assert!(context.image_known(Path::new("small.png")));
}

#[test]
fn background_images_paint_between_the_colour_and_the_content_and_nine_slice_into_pieces() {
    let mut context = Context::new();
    let photo = picture(200, 100);
    let frame = picture(30, 30);
    let navy = Color(10, 20, 60, 255);
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        let sized = Style::default()
            .width(Length::Px(100.0))
            .height(Length::Px(100.0));
        ui.keyed("card", |ui| {
            ui.column(
                sized
                    .clone()
                    .radius(9.0)
                    .background(navy)
                    .background_image(BackgroundImage::new(&photo)),
                |ui| ui.label("Title"),
            )
        });
        ui.keyed("framed", |ui| {
            ui.column(
                sized.foreground(navy).background_image(
                    BackgroundImage::new(&frame)
                        .slice(10.0, 10.0, 10.0, 10.0)
                        .tint(ImageTint::Foreground),
                ),
                |_| {},
            )
        });
    });
    let card = bounds(&context, "card");
    let primitives = &context.scene().primitives;
    let position =
        |wanted: &dyn Fn(&Primitive) -> bool| primitives.iter().position(wanted).unwrap();
    let colour = position(
        &|p| matches!(p, Primitive::Quad { rect, color, .. } if *rect == card && *color == navy),
    );
    let image = position(&|p| matches!(p, Primitive::Image { rect, .. } if *rect == card));
    let title = position(
        &|p| matches!(p, Primitive::Text { text, .. } | Primitive::StyledText { text, .. } if text == "Title"),
    );
    assert!(colour < image && image < title);
    let images = drawn(&context);
    // Cover by default, rounded like the node.
    assert_eq!(images[0].1.width, 100.0);
    assert_eq!(images[0].3, 9.0);
    // Nine pieces, square corners kept, tinted with the inherited colour, never rounded.
    assert_eq!(images.len(), 10);
    let framed = bounds(&context, "framed");
    assert_eq!((images[1].0.x, images[1].0.width), (framed.x, 10.0));
    assert_eq!(images[5].0.width, 80.0);
    assert!(
        images[1..]
            .iter()
            .all(|piece| piece.2 == Some(navy) && piece.3 == 0.0)
    );
}

#[test]
fn background_files_load_through_the_host_and_repaint_when_they_arrive() {
    let mut context = Context::new();
    let path = Path::new("skin/panel.png");
    let frame = |context: &mut Context| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            let style = Style::default()
                .width(Length::Px(80.0))
                .height(Length::Px(40.0))
                .background_image(BackgroundImage::file(path).fit(ImageFit::Fill));
            ui.keyed("panel", |ui| ui.column(style, |_| {}));
        })
    };
    frame(&mut context);
    assert_eq!(context.image_paths(), [path.to_owned()]);
    assert!(drawn(&context).is_empty());
    context.set_image_file(path, Ok(picture(8, 8)));
    frame(&mut context);
    let images = drawn(&context);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].0, bounds(&context, "panel"));
    assert_eq!(images[0].1.width, 8.0);
}

#[test]
fn stylesheets_describe_background_images_relative_to_their_own_file() {
    let mut sheet = StyleSheet::parse(
        "class panel {\n\
             background-image: url(\"skin/panel.png\")\n\
             background-slice: 8 12\n\
             background-tint: foreground\n\
         }\n\
         class hero extends panel { background-image: url(/abs/hero.jpg); background-slice: none; background-fit: contain }\n\
         class plain extends panel { background-image: none }",
    )
    .unwrap();
    sheet.rebase(Path::new("app/styles"));
    let panel = sheet
        .class("panel")
        .unwrap()
        .background_image
        .clone()
        .unwrap();
    assert_eq!(
        panel.source,
        ImageSource::File(Path::new("app/styles").join("skin/panel.png"))
    );
    assert_eq!(panel.slice, Some([8.0, 12.0, 8.0, 12.0]));
    assert_eq!(panel.tint, ImageTint::Foreground);
    assert_eq!(panel.fit, ImageFit::Cover);
    // A new image keeps what the base class said about it; absolute paths stay.
    let hero = sheet
        .class("hero")
        .unwrap()
        .background_image
        .clone()
        .unwrap();
    assert_eq!(hero.source, ImageSource::File("/abs/hero.jpg".into()));
    assert_eq!(
        (hero.slice, hero.fit, hero.tint),
        (None, ImageFit::Contain, ImageTint::Foreground)
    );
    assert!(sheet.class("plain").unwrap().background_image.is_none());

    for (text, message) in [
        (
            "class a { background-fit: cover }",
            "needs a background-image",
        ),
        ("class a { background-image: panel.png }", "url(file)"),
        (
            "class a { background-image: url(p.png); background-slice: 1 2 3 }",
            "1, 2 or 4",
        ),
        (
            "class a { background-image: url(p.png); background-fit: tile }",
            "unknown background-fit",
        ),
    ] {
        let error = StyleSheet::parse(text).unwrap_err();
        assert!(error.message.contains(message), "{text}: {}", error.message);
    }
}
