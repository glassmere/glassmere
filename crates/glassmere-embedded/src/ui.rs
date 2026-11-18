//! An application on a display: input in, pixels out.
use crate::{EmbeddedMeasurer, EmbeddedRenderer, Framebuffer, Pixel};
use alloc::vec::Vec;
use core::time::Duration;
use glassmere_core::{App, AppDriver, Context, Event};
use glassmere_scene::{Point, Rect, Size};

/// Runs an [`App`] for one display, without an operating system: the
/// firmware loop hands it input and the time, and paints what changed.
///
/// ```
/// use glassmere_core::{App, Button, Effects, Ui};
/// use glassmere_embedded::{EmbeddedUi, Framebuffer, Rgb565};
/// use core::time::Duration;
///
/// struct Counter(u32);
/// impl App for Counter {
///     type Message = ();
///     fn update(&mut self, _: (), _: &mut Effects<'_, ()>) {
///         self.0 += 1;
///     }
///     fn view(&self, ui: &mut Ui<'_, ()>) {
///         ui.button(Button::new("Count").on_click(()));
///     }
/// }
///
/// let mut pixels = vec![Rgb565::default(); 320 * 240];
/// let mut ui = EmbeddedUi::new(Counter(0), 320, 240);
/// ui.update(Duration::ZERO);
/// // In the firmware loop:
/// ui.press(20.0, 20.0);
/// ui.release();
/// if ui.update(Duration::from_millis(16)) {
///     ui.render_damage(&mut Framebuffer::new(&mut pixels, 320, 240));
///     // ...send the `ui.damage()` rectangles of `pixels` to the panel...
///     ui.presented();
/// }
/// assert_eq!(ui.app().0, 1);
/// ```
pub struct EmbeddedUi<A: App> {
    app: A,
    driver: AppDriver<A::Message>,
    context: Context,
    renderer: EmbeddedRenderer,
    measurer: EmbeddedMeasurer,
    size: Size,
    /// What has changed on screen since `presented`.
    damage: Vec<Rect>,
}

impl<A: App> EmbeddedUi<A> {
    /// `app` on a display of `width` by `height` pixels, with the built-in
    /// fonts.
    pub fn new(app: A, width: u32, height: u32) -> Self {
        Self::with_renderer(app, width, height, EmbeddedRenderer::default())
    }

    /// The same with a renderer of your own fonts and options.
    pub fn with_renderer(app: A, width: u32, height: u32, renderer: EmbeddedRenderer) -> Self {
        let size = Size::new(width as f32, height as f32);
        Self {
            app,
            // Nothing sleeps here for a wake to end: the loop calls `update`.
            driver: AppDriver::new(|| true),
            context: Context::new(),
            measurer: renderer.measurer(),
            renderer,
            size,
            damage: alloc::vec![Rect {
                x: 0.0,
                y: 0.0,
                width: size.width,
                height: size.height,
            }],
        }
    }

    pub fn app(&self) -> &A {
        &self.app
    }

    /// The app, to change outside `update`. The next [`update`](Self::update)
    /// declares its view again.
    pub fn app_mut(&mut self) -> &mut A {
        self.driver.invalidate();
        &mut self.app
    }

    /// The tree, to set a theme or a stylesheet.
    pub fn context(&mut self) -> &mut Context {
        self.driver.invalidate();
        &mut self.context
    }

    /// A message for the app, handled by the next [`update`](Self::update):
    /// a sensor reading, a timer, a packet.
    pub fn send(&mut self, message: A::Message) {
        self.driver.send(message);
    }

    /// Input from the device: keys of a keypad or an encoder, a scroll, a
    /// touch.
    pub fn event(&mut self, event: Event) {
        self.context.event(event);
    }

    /// A press of a single-touch panel, in pixels.
    pub fn press(&mut self, x: f32, y: f32) {
        self.context.event(Event::PointerMoved(Point { x, y }));
        self.context.event(Event::PointerDown);
    }

    /// The pressed point moved.
    pub fn drag(&mut self, x: f32, y: f32) {
        self.context.event(Event::PointerMoved(Point { x, y }));
    }

    /// The panel is no longer pressed.
    pub fn release(&mut self) {
        self.context.event(Event::PointerUp);
        // Nothing hovers once the finger is gone.
        self.context.event(Event::PointerLeft);
    }

    /// One turn of the loop. `now` is the time since start, from a timer or
    /// a tick count; animations and tooltips run on it. True when the screen
    /// has to be painted: [`damage`](Self::damage) says where.
    pub fn update(&mut self, now: Duration) -> bool {
        self.context.set_now(now);
        let stats = self.driver.frame(
            &mut self.app,
            &mut self.context,
            self.size,
            &mut self.measurer,
        );
        if stats.scene_rebuilt {
            self.damage.extend_from_slice(self.context.damage_regions());
            if self.damage.len() > 8 {
                self.damage.clear();
                self.damage.push(Rect {
                    x: 0.0,
                    y: 0.0,
                    width: self.size.width,
                    height: self.size.height,
                });
            }
        }
        !self.damage.is_empty()
    }

    /// Whether to call [`update`](Self::update) again without input: an
    /// animation is running.
    pub fn animating(&self) -> bool {
        self.context.animating()
    }

    /// When `update` next has something to do without input, on its clock.
    /// Until then, and while nothing is [`animating`](Self::animating), the
    /// processor can sleep.
    pub fn next_wake(&self) -> Option<Duration> {
        self.context.next_wake()
    }

    /// The rectangles that changed since [`presented`](Self::presented), in
    /// pixels: what to send to the panel. The whole screen at first.
    pub fn damage(&self) -> &[Rect] {
        &self.damage
    }

    /// Paints the part of the screen `target` covers, all of it. A buffer
    /// smaller than the screen is painted band after band, moved with
    /// [`Framebuffer::set_origin`].
    pub fn render<P: Pixel>(&mut self, target: &mut Framebuffer<'_, P>) {
        let clear = self.context.theme().window;
        self.renderer
            .render(self.context.scene(), target, Some(clear));
    }

    /// Paints only what changed since [`presented`](Self::presented), for a
    /// buffer that still holds the last frame.
    pub fn render_damage<P: Pixel>(&mut self, target: &mut Framebuffer<'_, P>) {
        let clear = self.context.theme().window;
        for damage in &self.damage {
            self.renderer
                .render_damage(self.context.scene(), target, *damage, Some(clear));
        }
    }

    /// The panel shows the frame: forgets the damage.
    pub fn presented(&mut self) {
        self.damage.clear();
    }
}
