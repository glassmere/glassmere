//! The firmware of `examples/embedded-firmware` on the desk: its thermostat
//! runs through `glassmere-embedded` into a 320x240 RGB565 buffer, as it does
//! on a microcontroller. This window shows that buffer as large as it fits,
//! passes the mouse on as the touch panel, and plays the board's timer and
//! temperature sensor.
use embedded_firmware::{HEIGHT, Message, Thermostat, WIDTH};
use glassmere::embedded::{EmbeddedUi, Framebuffer, Pixel, Rgb565};
use glassmere::{
    Align, App, AppOptions, AreaBuilder, Effects, EventArea, Image, ImageFit, ImageOptions,
    Justify, Length, Point, Size, Style, Ui, UiEvent, run,
};
use std::time::{Duration, Instant};

/// How often the firmware's loop turns.
const TICK: Duration = Duration::from_millis(33);
/// How often the sensor reports.
const READING: Duration = Duration::from_secs(1);

/// Positions are in the panel's pixels.
#[derive(Clone)]
enum Touch {
    Press(Point),
    Move(Point),
    Release,
    Tick,
}

struct Simulator {
    device: EmbeddedUi<Thermostat>,
    /// The panel's memory.
    pixels: Vec<Rgb565>,
    /// What the window shows of it.
    shown: Image,
    started: Instant,
    /// When the sensor last reported.
    read: Duration,
}

impl Simulator {
    fn new() -> Self {
        let mut simulator = Self {
            device: EmbeddedUi::new(Thermostat::default(), WIDTH, HEIGHT),
            pixels: vec![Rgb565::default(); (WIDTH * HEIGHT) as usize],
            shown: Image::from_rgba8(1, 1, vec![0; 4]).expect("one pixel"),
            started: Instant::now(),
            read: Duration::ZERO,
        };
        simulator.turn();
        simulator
    }

    /// One turn of the firmware's loop.
    fn turn(&mut self) {
        let now = self.started.elapsed();
        if now - self.read >= READING {
            self.read = now;
            // The room warms while the heating is on and cools without it.
            let room = self.device.app();
            let next = if room.heating && room.measured < room.target {
                room.measured + 1
            } else if !room.heating && room.measured > 150 {
                room.measured - 1
            } else {
                room.measured
            };
            if next != room.measured {
                self.device.send(Message::Measured(next));
            }
        }
        if !self.device.update(now) {
            return;
        }
        let mut frame = Framebuffer::new(&mut self.pixels, WIDTH, HEIGHT);
        self.device.render_damage(&mut frame);
        self.device.presented();
        let rgba = self
            .pixels
            .iter()
            .flat_map(|pixel| {
                let [r, g, b] = pixel.to_rgb();
                [r, g, b, 255]
            })
            .collect();
        self.shown = Image::from_rgba8(WIDTH, HEIGHT, rgba).expect("a full frame");
    }
}

impl App for Simulator {
    type Message = Touch;

    fn init(&mut self, effects: &mut Effects<'_, Touch>) {
        // The board's timer: animations on the device run on these turns.
        let _ = effects.spawn(
            "timer",
            |out| {
                while !out.cancelled() {
                    let _ = out.send(());
                    std::thread::sleep(TICK);
                }
            },
            |()| Touch::Tick,
        );
    }

    fn update(&mut self, touch: Touch, _effects: &mut Effects<'_, Touch>) {
        match touch {
            Touch::Press(point) => self.device.press(point.x, point.y),
            // Also while nothing is pressed, which a touch panel cannot
            // tell: on the desk the pointer shows what it is over.
            Touch::Move(point) => self.device.drag(point.x, point.y),
            Touch::Release => self.device.release(),
            Touch::Tick => {}
        }
        self.turn();
    }

    fn view(&self, ui: &mut Ui<'_, Touch>) {
        // The panel keeps its 320x240 pixels; the window only magnifies them.
        let window = ui.window_size();
        let scale = (window.width / WIDTH as f32)
            .min(window.height / HEIGHT as f32)
            .max(0.1);
        let panel = Style::default()
            .width(Length::Px(WIDTH as f32 * scale))
            .height(Length::Px(HEIGHT as f32 * scale));
        let on_panel = move |point: Point| Point {
            x: point.x / scale,
            y: point.y / scale,
        };
        let touch = AreaBuilder::new(EventArea::pointer().capturing()).on_event(move |event| {
            Some(match event {
                UiEvent::PointerDown(point) => Touch::Press(on_panel(point)),
                UiEvent::PointerMoved(point) => Touch::Move(on_panel(point)),
                UiEvent::PointerUp(_) => Touch::Release,
                _ => return None,
            })
        });
        let centred = Style::default()
            .width(Length::Percent(1.0))
            .height(Length::Percent(1.0))
            .align_items(Align::Center)
            .justify_content(Justify::Center);
        ui.column(centred, |ui| {
            ui.event_area(panel.clone(), touch, |ui| {
                ui.image(
                    &self.shown,
                    ImageOptions::new("The device's display")
                        .fit(ImageFit::Fill)
                        .style(panel),
                );
            });
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    run(
        AppOptions {
            title: "Glassmere / Embedded".into(),
            size: Size::new(WIDTH as f32 * 2.0, HEIGHT as f32 * 2.0),
            frame_limit: smoke.then_some(2),
            ..AppOptions::default()
        },
        Simulator::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use glassmere::Event;
    use glassmere_testing::Harness;

    /// A click at a point of the 320x240 panel, in a window of `size`.
    fn click(harness: &mut Harness<Simulator>, size: Size, x: f32, y: f32) {
        let scale = (size.width / WIDTH as f32).min(size.height / HEIGHT as f32);
        let at = Point {
            x: (size.width - WIDTH as f32 * scale) / 2.0 + x * scale,
            y: (size.height - HEIGHT as f32 * scale) / 2.0 + y * scale,
        };
        harness.event(Event::PointerMoved(at));
        harness.run();
        harness.event(Event::PointerDown);
        harness.run();
        // The timer turns the loop while the button is held.
        for _ in 0..3 {
            std::thread::sleep(TICK);
            harness.app_mut().turn();
        }
        harness.event(Event::PointerUp);
        harness.run();
    }

    fn shown(harness: &Harness<Simulator>) -> Option<(Image, glassmere::Rect)> {
        let scene = harness.context().scene();
        scene
            .primitives
            .iter()
            .find_map(|primitive| match primitive {
                glassmere::Primitive::Image { image, rect, .. } => Some((image.clone(), *rect)),
                _ => None,
            })
    }

    #[test]
    fn a_click_reaches_the_device_and_the_window_shows_the_result() {
        let size = Size::new(640.0, 480.0);
        let mut harness = Harness::app(size, Simulator::new());
        let (first, _) = shown(&harness).expect("the display");
        click(&mut harness, size, 295.0, 84.0);
        assert_eq!(harness.state().device.app().target, 220);
        assert!(shown(&harness).expect("the display").0 != first);
    }

    #[test]
    fn the_display_follows_the_window_and_stays_clickable() {
        let mut harness = Harness::app(Size::new(640.0, 480.0), Simulator::new());
        // Wider than 4:3, so the panel is centred with bars at its sides.
        let size = Size::new(1000.0, 600.0);
        harness.resize(size);
        harness.run();
        let (_, rect) = shown(&harness).expect("the display");
        assert_eq!(
            (rect.x, rect.y, rect.width, rect.height),
            (100.0, 0.0, 800.0, 600.0)
        );
        click(&mut harness, size, 30.0, 84.0);
        assert_eq!(harness.state().device.app().target, 210);
    }
}
