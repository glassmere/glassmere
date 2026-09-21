use rust_gui_core::{
    Context, ImageFit, ImageOptions, ImageTint, Length, MonospaceMeasurer, Role, Style,
};
use rust_gui_scene::{Color, Image, Primitive, Rect, Size};
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
        ui.row("row", Style::default(), |ui| {
            ui.image("natural", &image, ImageOptions::new("A landscape"));
            ui.image(
                "wide",
                &image,
                ImageOptions::new("").style(Style::default().width(Length::Px(100.0))),
            );
            ui.image(
                "cover",
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
            );
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
        ui.column("tinted", Style::default().foreground(red), |ui| {
            ui.image(
                "icon",
                &icon,
                ImageOptions::new("").tint(ImageTint::Foreground),
            );
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
            ui.image_file(
                "photo",
                path,
                ImageOptions::new("Photo").style(
                    Style::default()
                        .width(Length::Px(120.0))
                        .height(Length::Px(80.0)),
                ),
            );
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

    // A failure before any success: still a placeholder, now with the name.
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
                ui.image_file("big", big, ImageOptions::default());
            }
            ui.image_file("small", "small.png", ImageOptions::default());
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
