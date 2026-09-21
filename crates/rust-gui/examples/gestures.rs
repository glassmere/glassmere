//! Gestures: a photo that fingers move, scale and turn. One finger pans, two
//! pinch and rotate about the point between them, and a pinch on a trackpad
//! or Ctrl with the wheel zooms about the pointer. The two sliders above the
//! photo can be dragged at the same time, one finger each.
use rust_gui::{AppOptions, Color, ImageFit, ImageOptions, Length, Size, Style, Transform, run};

const PHOTO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/dusk.png");
const CANVAS: Size = Size::new(720.0, 440.0);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    // Where the photo's centre is, from the canvas centre; its size; its turn.
    // `--demo` starts moved, enlarged and turned, for a screenshot.
    let (mut x, mut y, mut zoom, mut turn) = if std::env::args().any(|arg| arg == "--demo") {
        (60.0f32, -20.0f32, 1.6f32, 0.35f32)
    } else {
        (0.0, 0.0, 1.0, 0.0)
    };
    let (mut corners, mut opacity) = (10.0f64, 100.0f64);
    let mut taps = 0;
    run(
        AppOptions {
            title: "Rust GUI / Gestures".into(),
            size: Size::new(780.0, 640.0),
            frame_limit: smoke.then_some(20),
            ..AppOptions::default()
        },
        move |ui| {
            if smoke {
                ui.request_redraw();
            }
            let page = Style {
                padding: 24.0,
                gap: 14.0,
                width: Length::Percent(1.0),
                ..Style::default()
            };
            ui.column("page", page, |ui| {
                ui.row("top", Style::default().gap(16.0), |ui| {
                    ui.column("corners", Style::default().width(Length::Px(220.0)), |ui| {
                        ui.slider("slider", "Corners", &mut corners, 0.0..=100.0, 1.0);
                    });
                    ui.column("opacity", Style::default().width(Length::Px(220.0)), |ui| {
                        ui.slider("slider", "Opacity", &mut opacity, 0.0..=100.0, 1.0);
                    });
                    if ui.button_keyed("reset", "Reset view").clicked() {
                        (x, y, zoom, turn) = (0.0, 0.0, 1.0, 0.0);
                    }
                });
                ui.label_keyed(
                    "state",
                    format!(
                        "Zoom {zoom:.2}  ·  turned {:.0}°  ·  moved {x:.0}, {y:.0}  ·  taps {taps}  ·  sliders {corners:.0} / {opacity:.0}",
                        turn.to_degrees()
                    ),
                );

                let canvas = Style {
                    width: Length::Px(CANVAS.width),
                    height: Length::Px(CANVAS.height),
                    radius: 12.0,
                    background: Some(Color::rgb(24, 35, 49)),
                    align_items: rust_gui::Align::Center,
                    justify_content: rust_gui::Justify::Center,
                    ..Style::default()
                };
                let gesture = ui.gesture_area("canvas", canvas, |ui| {
                    let photo = Style {
                        width: Length::Px(320.0),
                        height: Length::Px(200.0),
                        radius: corners as f32,
                        // Never quite gone, so that there is something to find again.
                        opacity: 0.15 + 0.85 * (opacity / 100.0) as f32,
                        transform: Transform {
                            translate: (x, y),
                            scale: (zoom, zoom),
                            rotate: turn.to_degrees(),
                        },
                        ..Style::default()
                    };
                    ui.image_file(
                        "photo",
                        PHOTO,
                        ImageOptions::new("Hills at dusk")
                            .fit(ImageFit::Cover)
                            .style(photo),
                    );
                });
                if gesture.changed() {
                    // What was under the fingers stays under them: the photo's
                    // centre is scaled and turned about the point the fingers
                    // came from, and lands relative to where they are now.
                    let about = gesture.center.unwrap_or_default();
                    let (cx, cy) = (
                        about.x - CANVAS.width / 2.0,
                        about.y - CANVAS.height / 2.0,
                    );
                    let factor = gesture.scale.clamp(0.2 / zoom, 8.0 / zoom);
                    let (sin, cos) = gesture.rotation.sin_cos();
                    let (dx, dy) = (
                        (x - (cx - gesture.pan_x)) * factor,
                        (y - (cy - gesture.pan_y)) * factor,
                    );
                    x = cx + dx * cos - dy * sin;
                    y = cy + dx * sin + dy * cos;
                    zoom *= factor;
                    turn += gesture.rotation;
                }
                if gesture.tapped.is_some() {
                    taps += 1;
                }
            });
        },
    )
}
