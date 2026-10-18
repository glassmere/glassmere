//! Gestures: a photo that fingers move, scale and turn. One finger pans, two
//! pinch and rotate about the point between them, and a pinch on a trackpad
//! or Ctrl with the wheel zooms about the pointer. The two sliders above the
//! photo can be dragged at the same time, one finger each.
use seaglass::{
    Align, App, AppOptions, Button, Color, Effects, GestureBuilder, GestureResponse, ImageFit,
    ImageOptions, Justify, Length, Size, Style, Transform, Ui, run,
};

const PHOTO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/dusk.png");
const CANVAS: Size = Size::new(720.0, 440.0);

#[derive(Clone)]
enum Msg {
    Corners(f64),
    Opacity(f64),
    Reset,
    Gesture(GestureResponse),
}

struct Gestures {
    smoke: bool,
    /// Where the photo's centre is, from the canvas centre; its size; its turn.
    x: f32,
    y: f32,
    zoom: f32,
    turn: f32,
    corners: f64,
    opacity: f64,
    taps: u32,
}

impl App for Gestures {
    type Message = Msg;

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Corners(corners) => self.corners = corners,
            Msg::Opacity(opacity) => self.opacity = opacity,
            Msg::Reset => (self.x, self.y, self.zoom, self.turn) = (0.0, 0.0, 1.0, 0.0),
            Msg::Gesture(gesture) => {
                if gesture.changed() {
                    // What was under the fingers stays under them: the photo's
                    // centre is scaled and turned about the point the fingers
                    // came from, and lands relative to where they are.
                    let about = gesture.center.unwrap_or_default();
                    let (cx, cy) = (about.x - CANVAS.width / 2.0, about.y - CANVAS.height / 2.0);
                    let factor = gesture.scale.clamp(0.2 / self.zoom, 8.0 / self.zoom);
                    let (sin, cos) = gesture.rotation.sin_cos();
                    let (dx, dy) = (
                        (self.x - (cx - gesture.pan_x)) * factor,
                        (self.y - (cy - gesture.pan_y)) * factor,
                    );
                    self.x = cx + dx * cos - dy * sin;
                    self.y = cy + dx * sin + dy * cos;
                    self.zoom *= factor;
                    self.turn += gesture.rotation;
                }
                if gesture.tapped.is_some() {
                    self.taps += 1;
                }
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        if self.smoke {
            ui.request_redraw();
        }
        let page = Style {
            padding: 24.0,
            gap: 14.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        ui.column(page, |ui| {
            ui.row(Style::default().gap(16.0), |ui| {
                ui.column(Style::default().width(Length::Px(220.0)), |ui| {
                    ui.slider("Corners", self.corners, 0.0..=100.0, 1.0, Msg::Corners);
                });
                ui.column(Style::default().width(Length::Px(220.0)), |ui| {
                    ui.slider("Opacity", self.opacity, 0.0..=100.0, 1.0, Msg::Opacity);
                });
                ui.button(Button::new("Reset view").on_click(Msg::Reset));
            });
            ui.label(format!(
                "Zoom {:.2}  ·  turned {:.0}°  ·  moved {:.0}, {:.0}  ·  taps {}  ·  \
                 sliders {:.0} / {:.0}",
                self.zoom,
                self.turn.to_degrees(),
                self.x,
                self.y,
                self.taps,
                self.corners,
                self.opacity,
            ));

            let canvas = Style {
                width: Length::Px(CANVAS.width),
                height: Length::Px(CANVAS.height),
                radius: 12.0,
                background: Some(Color::rgb(24, 35, 49)),
                align_items: Align::Center,
                justify_content: Justify::Center,
                ..Style::default()
            };
            ui.gesture_area(
                canvas,
                GestureBuilder::new().on_gesture(Msg::Gesture),
                |ui| {
                    let photo = Style {
                        width: Length::Px(320.0),
                        height: Length::Px(200.0),
                        radius: self.corners as f32,
                        // Never quite gone, so that there is something to find again.
                        opacity: 0.15 + 0.85 * (self.opacity / 100.0) as f32,
                        transform: Transform {
                            translate: (self.x, self.y),
                            scale: (self.zoom, self.zoom),
                            rotate: self.turn.to_degrees(),
                        },
                        ..Style::default()
                    };
                    ui.image_file(
                        PHOTO,
                        ImageOptions::new("Hills at dusk")
                            .fit(ImageFit::Cover)
                            .style(photo),
                    );
                },
            );
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    // `--demo` starts moved, enlarged and turned, for a screenshot.
    let (x, y, zoom, turn) = if std::env::args().any(|arg| arg == "--demo") {
        (60.0f32, -20.0f32, 1.6f32, 0.35f32)
    } else {
        (0.0, 0.0, 1.0, 0.0)
    };
    run(
        AppOptions {
            title: "Seaglass / Gestures".into(),
            size: Size::new(780.0, 640.0),
            frame_limit: smoke.then_some(20),
            ..AppOptions::default()
        },
        Gestures {
            smoke,
            x,
            y,
            zoom,
            turn,
            corners: 10.0,
            opacity: 100.0,
            taps: 0,
        },
    )?;
    Ok(())
}
