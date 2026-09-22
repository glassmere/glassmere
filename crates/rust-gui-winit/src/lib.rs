//! Event-driven desktop host with logical coordinates and a wgpu surface.
mod accessibility;
mod desktop;
mod diagnostics;
mod dock;
mod graphics;
pub mod hot;
mod images;
mod input;
mod mailbox;
mod os_drag;
mod wakeup;
pub use desktop::{DesktopClosed, DesktopHandle, WindowKey, run_windows};
pub use dock::{DockWindowView, DockWindows};
pub use graphics::{GpuMemoryPolicy, GraphicsBackend, GraphicsOptions};
pub use mailbox::{Mailbox, MessageSender};
use rust_gui_core::{Action, Context, DragPayload, Event, StyleSheet, TextEdit, Theme, Ui};
use rust_gui_devtools::Recorder;
use rust_gui_render_cpu::CpuRenderer;
use rust_gui_render_wgpu::Renderer;
use rust_gui_scene::{Color, Point, Primitive, Scene, Size};
use std::{
    cell::RefCell,
    error::Error,
    num::NonZeroU32,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use wakeup::UserEvent;
pub use wakeup::WakeHandle;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::ModifiersState,
    window::{CursorIcon, Window, WindowId},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Which theme a window starts with and follows.
#[derive(Clone, Debug, PartialEq)]
pub enum ThemeChoice {
    /// The OS light/dark preference, re-applied when the OS switches.
    System,
    Fixed(Theme),
}

/// Where the window's reading and layout direction comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DirectionChoice {
    /// Right-to-left when the OS locale's language is written that way.
    #[default]
    Auto,
    Ltr,
    Rtl,
}

impl DirectionChoice {
    pub fn resolve(self) -> rust_gui_core::Direction {
        match self {
            Self::Ltr => rust_gui_core::Direction::Ltr,
            Self::Rtl => rust_gui_core::Direction::Rtl,
            Self::Auto => sys_locale::get_locale()
                .map_or(rust_gui_core::Direction::Ltr, |locale| {
                    locale_direction(&locale)
                }),
        }
    }
}

/// Direction of a BCP 47 locale such as `he-IL` or `ar`, by its language.
pub fn locale_direction(locale: &str) -> rust_gui_core::Direction {
    let language = locale
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    // Languages whose usual script is Arabic, Hebrew, Thaana or Syriac.
    const RTL: [&str; 12] = [
        "ar", "he", "iw", "fa", "ur", "yi", "ps", "sd", "ug", "dv", "ckb", "syr",
    ];
    if RTL.contains(&language.as_str()) {
        rust_gui_core::Direction::Rtl
    } else {
        rust_gui_core::Direction::Ltr
    }
}

#[cfg(test)]
mod direction_tests {
    use super::{DirectionChoice, locale_direction};
    use rust_gui_core::Direction;

    #[test]
    fn locales_map_to_directions_by_language() {
        for locale in [
            "ar", "ar-EG", "he-IL", "iw_IL", "fa-IR", "ur-PK", "ckb-IQ", "AR-sa",
        ] {
            assert_eq!(locale_direction(locale), Direction::Rtl, "{locale}");
        }
        for locale in ["en-US", "sk-SK", "ja", "", "arn-CL", "hello"] {
            assert_eq!(locale_direction(locale), Direction::Ltr, "{locale}");
        }
        assert_eq!(DirectionChoice::Rtl.resolve(), Direction::Rtl);
        assert_eq!(DirectionChoice::Ltr.resolve(), Direction::Ltr);
    }
}

#[derive(Clone, Debug)]
pub struct AppOptions {
    pub title: String,
    pub size: Size,
    /// Clear color; `None` uses the active theme's window token.
    pub background: Option<Color>,
    /// Useful for unattended native-window smoke checks; None runs until closed.
    pub frame_limit: Option<u64>,
    pub graphics: GraphicsOptions,
    /// F12 opens an inspector window for this window; `RUST_GUI_INSPECT` opens it at start.
    pub devtools: bool,
    pub theme: ThemeChoice,
    /// A stylesheet file installed at start and reloaded when it changes on disk.
    pub stylesheet: Option<PathBuf>,
    /// Skips animations. ponytail: not read from the OS setting yet, winit has no
    /// cross-platform query; wire it per platform when one appears.
    pub reduced_motion: bool,
    /// Whether Left and Right move on screen or in reading order in
    /// bidirectional text.
    pub caret_movement: rust_gui_core::CaretMovement,
    /// Layout and text direction of the window.
    pub direction: DirectionChoice,
}

impl Default for AppOptions {
    fn default() -> Self {
        Self {
            title: "Rust GUI".into(),
            size: Size::new(900.0, 650.0),
            background: None,
            frame_limit: None,
            graphics: GraphicsOptions::default(),
            devtools: true,
            theme: ThemeChoice::System,
            stylesheet: None,
            reduced_motion: false,
            caret_movement: rust_gui_core::CaretMovement::Visual,
            direction: DirectionChoice::Auto,
        }
    }
}

/// How often a configured stylesheet is checked for changes.
// ponytail: mtime polling keeps the host dependency-free; a file watcher
// (notify) is the upgrade path if half-second latency or idle wakeups matter.
pub(crate) const STYLESHEET_POLL: Duration = Duration::from_millis(500);

pub fn run(options: AppOptions, view: impl FnMut(&mut Ui<'_>) + 'static) -> Result<()> {
    run_with_wakeup(options, |_| view)
}

/// The setup closure receives a handle for worker threads or async tasks, then
/// returns the view. Application state can be shared via a mutex or channel.
pub fn run_with_wakeup<F: FnMut(&mut Ui<'_>) + 'static>(
    options: AppOptions,
    setup: impl FnOnce(WakeHandle) -> F,
) -> Result<()> {
    desktop::run_single(options, setup)
}

struct WindowState {
    surfaces: Surfaces,
    renderer: Rc<RefCell<SharedRenderer>>,
    pending_scene: bool,
    accessibility: Option<accesskit_winit::Adapter>,
    window: Arc<Window>,
    ime_target: Option<rust_gui_core::NodeId>,
    ime_enabled: bool,
}

/// The display's software presentation, shared by its windows.
type CpuContext = Rc<softbuffer::Context<winit::event_loop::OwnedDisplayHandle>>;
type CpuSurface = softbuffer::Surface<winit::event_loop::OwnedDisplayHandle, Arc<Window>>;

/// How a window's pixels reach the screen.
enum Surfaces {
    Gpu {
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    },
    Cpu {
        context: CpuContext,
        surface: CpuSurface,
        /// What the surface was last sized to; zero asks for a resize.
        size: [u32; 2],
    },
}

struct SharedRenderer {
    renderer: Painter,
    window: Option<WindowId>,
    gpu: GpuRouting,
}

/// The renderer the windows of one display share.
/// Boxed: both are a few kilobytes, and this lives behind an `Rc` anyway.
enum Painter {
    Gpu(Box<Renderer>),
    Cpu(Box<CpuRenderer>),
}

impl Painter {
    /// Both renderers lay text out with the system they will draw it with.
    fn text_system(&mut self) -> &mut rust_gui_render_wgpu::TextSystem {
        match self {
            Self::Gpu(renderer) => renderer.text_system(),
            Self::Cpu(renderer) => renderer.text_system(),
        }
    }
}

/// What a presentation leaves for the caller to do.
enum Presented {
    Drawn(Duration),
    /// Nothing was shown; a redraw was asked for where one is needed.
    Skipped,
    /// The surface was busy: come back shortly.
    Retry,
}

/// What to paint, and what the renderer needs to know about it.
struct Frame<'a> {
    scene: &'a Scene,
    clear: Color,
    /// The scene differs from the one the renderer prepared.
    changed: bool,
    /// What changed since this renderer's last frame, in logical pixels.
    /// Empty means everything, which is what a renderer that has nothing to
    /// keep needs anyway.
    damage: &'a [rust_gui_scene::Rect],
    /// The window's first frame, for the startup diagnostics.
    first: bool,
}

/// GPU times come back a few frames late and the renderer is shared, so the
/// time of a frame may complete while another window is drawing. This
/// remembers who drew which frame and holds a result until its window asks.
#[derive(Default)]
struct GpuRouting {
    /// Renderer frame, the window that drew it and that window's frame number.
    drawn: std::collections::VecDeque<(u64, WindowId, u64)>,
    /// The renderer frame whose profile was routed last.
    routed: Option<u64>,
    arrived: std::collections::HashMap<WindowId, GpuTimes>,
}

/// A window's frame number, its GPU time and the passes it was made of.
type GpuTimes = (u64, Duration, Vec<(String, Duration)>);

impl WindowState {
    async fn new(
        window: Arc<Window>,
        event_loop: &ActiveEventLoop,
        options: &GraphicsOptions,
    ) -> Result<Self> {
        let renderer = std::env::var("RUST_GUI_RENDERER").ok();
        if options.wants_cpu(renderer.as_deref()) {
            return Self::cpu(window, Self::display(event_loop)?, None);
        }
        // Initializing every compiled backend loads unused driver stacks and
        // enumerates their adapters. Try one backend, dropping failed attempts
        // before opening the next. WGPU_BACKEND restricts the candidate set.
        let environment = std::env::var("WGPU_BACKEND").ok();
        let candidates = options.candidates(
            wgpu::Instance::enabled_backend_features(),
            environment.as_deref(),
        );
        match graphics::first_available(candidates, |backend| {
            Self::with_backend(window.clone(), event_loop, backend, options)
        })
        .await
        {
            Ok(state) => Ok(state),
            // Painting without an adapter is slower, and it is how the
            // application runs at all on a machine whose drivers cannot.
            Err(error) => {
                eprintln!("rust-gui: {error}. Falling back to the CPU renderer.");
                Self::cpu(window, Self::display(event_loop)?, None)
            }
        }
    }

    fn display(event_loop: &ActiveEventLoop) -> Result<CpuContext> {
        softbuffer::Context::new(event_loop.owned_display_handle())
            .map(Rc::new)
            .map_err(|error| format!("no software presentation: {error}").into())
    }

    /// A window painted by `CpuRenderer`, sharing the renderer of the windows
    /// that are already open when there are any.
    fn cpu(
        window: Arc<Window>,
        context: CpuContext,
        renderer: Option<Rc<RefCell<SharedRenderer>>>,
    ) -> Result<Self> {
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|error| format!("no software presentation: {error}"))?;
        Ok(Self {
            surfaces: Surfaces::Cpu {
                context,
                surface,
                size: [0; 2],
            },
            renderer: renderer.unwrap_or_else(|| {
                Rc::new(RefCell::new(SharedRenderer {
                    renderer: Painter::Cpu(Box::new(CpuRenderer::new())),
                    window: None,
                    gpu: GpuRouting::default(),
                }))
            }),
            pending_scene: true,
            accessibility: None,
            window,
            ime_target: None,
            ime_enabled: false,
        })
    }

    async fn with_backend(
        window: Arc<Window>,
        event_loop: &ActiveEventLoop,
        backend: wgpu::Backends,
        options: &GraphicsOptions,
    ) -> Result<Self> {
        let mut descriptor = wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
            event_loop.owned_display_handle(),
        ));
        descriptor.backends = backend;
        let instance = wgpu::Instance::new(descriptor);
        diagnostics::checkpoint("gpu_instance", None);
        let surface = instance.create_surface(window.clone())?;
        diagnostics::checkpoint("surface_created", None);
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::LowPower,
                ..Default::default()
            })
            .await?;
        #[cfg(feature = "diagnostics")]
        eprintln!("GPU_ADAPTER {:?}", adapter.get_info());
        diagnostics::checkpoint("adapter_detected", None);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("rust-gui device"),
                // Lets the inspector time render passes on the GPU's own
                // clock where the adapter can; nothing is measured until an
                // inspector is open.
                required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
                memory_hints: diagnostics::memory_hints(options.memory_policy.hints()),
                required_limits: diagnostics::device_limits(options.limits()),
                ..Default::default()
            })
            .await?;
        diagnostics::checkpoint("gpu_device", Some(&device));
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("No compatible surface configuration")?;
        config.format = surface
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(wgpu::TextureFormat::is_srgb)
            .ok_or("No sRGB surface format")?;
        config.present_mode = wgpu::PresentMode::Fifo;
        config.desired_maximum_frame_latency =
            diagnostics::frame_latency(options.maximum_frame_latency.max(1));
        surface.configure(&device, &config);
        diagnostics::checkpoint("presentation_buffers", Some(&device));
        let renderer = Renderer::new(&device, &queue, config.format);
        diagnostics::checkpoint("renderer_and_fonts", Some(&device));
        Ok(Self {
            surfaces: Surfaces::Gpu {
                instance,
                adapter,
                device,
                queue,
                surface,
                config,
            },
            renderer: Rc::new(RefCell::new(SharedRenderer {
                renderer: Painter::Gpu(Box::new(renderer)),
                window: None,
                gpu: GpuRouting::default(),
            })),
            pending_scene: true,
            accessibility: None,
            window,
            ime_target: None,
            ime_enabled: false,
        })
    }

    fn resize(&mut self) {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        match &mut self.surfaces {
            Surfaces::Gpu {
                device,
                surface,
                config,
                ..
            } => {
                config.width = size.width;
                config.height = size.height;
                surface.configure(device, config);
            }
            // A software surface is sized by the frame that is about to use it.
            Surfaces::Cpu { size, .. } => *size = [0; 2],
        }
    }

    /// The graphics device, for the startup and memory diagnostics.
    fn device(&self) -> Option<&wgpu::Device> {
        match &self.surfaces {
            Surfaces::Gpu { device, .. } => Some(device),
            Surfaces::Cpu { .. } => None,
        }
    }

    fn adapter_name(&self) -> String {
        match &self.surfaces {
            Surfaces::Gpu { adapter, .. } => {
                let info = adapter.get_info();
                format!("{} ({:?})", info.name, info.backend)
            }
            Surfaces::Cpu { .. } => "CPU renderer, no graphics adapter".to_owned(),
        }
    }

    fn present_gpu(&mut self, renderer: &mut Renderer, frame: Frame<'_>) -> Result<Presented> {
        let Surfaces::Gpu {
            device,
            queue,
            surface,
            ..
        } = &self.surfaces
        else {
            return Ok(Presented::Skipped);
        };
        // Cheap handle clones, so that a lost surface can be replaced below.
        let (device, queue) = (device.clone(), queue.clone());
        let (texture, suboptimal) = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
            wgpu::CurrentSurfaceTexture::Occluded => return Ok(Presented::Skipped),
            wgpu::CurrentSurfaceTexture::Timeout => return Ok(Presented::Retry),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.resize();
                self.window.request_redraw();
                return Ok(Presented::Skipped);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                if let Surfaces::Gpu {
                    instance, surface, ..
                } = &mut self.surfaces
                {
                    *surface = instance.create_surface(self.window.clone())?;
                }
                self.resize();
                self.window.request_redraw();
                return Ok(Presented::Skipped);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Surface validation failed".into());
            }
        };
        let target = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let size = self.window.inner_size();
        let scale = self.window.scale_factor() as f32;
        let started = Instant::now();
        {
            let _render_span = tracing::trace_span!("rust_gui.render").entered();
            renderer.render_cached(
                &device,
                &queue,
                &target,
                [size.width, size.height],
                scale,
                frame.scene,
                frame.clear,
                frame.changed,
                frame.damage,
            )?;
        }
        if frame.first {
            diagnostics::checkpoint("first_render", Some(&device));
        }
        self.window.pre_present_notify();
        texture.present();
        let render = started.elapsed();
        if frame.first {
            diagnostics::checkpoint("first_present", Some(&device));
        }
        if suboptimal {
            self.resize();
        }
        Ok(Presented::Drawn(render))
    }

    fn present_cpu(&mut self, renderer: &mut CpuRenderer, frame: Frame<'_>) -> Result<Presented> {
        let physical = self.window.inner_size();
        let scale = self.window.scale_factor() as f32;
        let (Some(width), Some(height)) = (
            NonZeroU32::new(physical.width),
            NonZeroU32::new(physical.height),
        ) else {
            return Ok(Presented::Skipped);
        };
        let Surfaces::Cpu { surface, size, .. } = &mut self.surfaces else {
            return Ok(Presented::Skipped);
        };
        let started = Instant::now();
        if *size != [physical.width, physical.height] {
            surface
                .resize(width, height)
                .map_err(|error| format!("software surface not resized: {error}"))?;
            *size = [physical.width, physical.height];
        }
        let pixels = renderer.render_damaged(
            [physical.width, physical.height],
            scale,
            frame.scene,
            frame.clear,
            frame.damage,
        );
        let mut buffer = surface
            .buffer_mut()
            .map_err(|error| format!("no software buffer: {error}"))?;
        for (out, pixel) in buffer.iter_mut().zip(pixels.as_chunks::<4>().0) {
            *out = u32::from(pixel[0]) << 16 | u32::from(pixel[1]) << 8 | u32::from(pixel[2]);
        }
        self.window.pre_present_notify();
        buffer
            .present()
            .map_err(|error| format!("software frame not presented: {error}"))?;
        Ok(Presented::Drawn(started.elapsed()))
    }
}

struct Application<F> {
    options: AppOptions,
    view: F,
    context: Context,
    state: Option<WindowState>,
    modifiers: ModifiersState,
    error: Option<Box<dyn Error>>,
    frames: u64,
    accessibility_requested: bool,
    clipboard: Rc<RefCell<Option<arboard::Clipboard>>>,
    restore_focus: Option<rust_gui_core::NodeId>,
    close_requested: bool,
    pointer: Point,
    logical_size: Size,
    recorder: Recorder,
    inspector_requested: bool,
    /// Inspector decorations painted after the scene, in logical coordinates.
    overlay: Vec<Primitive>,
    overlay_dirty: bool,
    poller: hot::Poller,
    images: images::ImageFiles,
    last_poll: Instant,
    /// The development runner asked this process to stop.
    hot_stopping: bool,
    retry_pending: bool,
    /// Framework diagnostics already printed.
    diagnostics_seen: usize,
    /// A drag that was handed to the operating system and has not ended.
    os_drag: Option<std::sync::mpsc::Receiver<rust_gui_core::DragOutcome>>,
    /// Some inspector is open, so the shared renderer profiles its frames.
    profile_gpu: bool,
    /// An inspector is looking at this window.
    inspected: bool,
    started: Instant,
}

impl<F: FnMut(&mut Ui<'_>)> Application<F> {
    fn new(
        options: AppOptions,
        view: F,
        state: WindowState,
        clipboard: Rc<RefCell<Option<arboard::Clipboard>>>,
        wake: WakeHandle,
    ) -> Self {
        let mut context = Context::new();
        context.set_reduced_motion(options.reduced_motion);
        context.set_caret_movement(options.caret_movement);
        context.set_direction(options.direction.resolve());
        // `ToastSender::send` from any thread wakes the loop; the next frame shows the toast.
        let toast_wake = wake.clone();
        context.set_toast_wake(move || {
            toast_wake.request_redraw();
        });
        Self {
            options,
            view,
            context,
            state: Some(state),
            modifiers: ModifiersState::empty(),
            error: None,
            frames: 0,
            accessibility_requested: false,
            clipboard,
            restore_focus: None,
            close_requested: false,
            pointer: Point::default(),
            logical_size: Size::default(),
            recorder: Recorder::new(256),
            inspector_requested: false,
            overlay: Vec::new(),
            overlay_dirty: false,
            poller: hot::Poller::default(),
            images: images::ImageFiles::new(wake),
            last_poll: Instant::now(),
            hot_stopping: false,
            retry_pending: false,
            diagnostics_seen: 0,
            os_drag: None,
            profile_gpu: false,
            inspected: false,
            started: Instant::now(),
        }
    }

    fn sync_clock(&mut self) {
        self.context.set_now(self.started.elapsed());
    }

    /// The earliest moment this window needs the loop to wake without input.
    fn deadline(&self) -> Option<Instant> {
        let wake = self.context.next_wake().map(|at| self.started + at);
        let polls = self.options.stylesheet.is_some()
            || !self.context.watched_paths().is_empty()
            || !self.context.image_paths().is_empty()
            || hot::active();
        let poll = polls.then(|| Instant::now() + STYLESHEET_POLL);
        let retry = self
            .retry_pending
            .then(|| Instant::now() + Duration::from_millis(100));
        [wake, poll, retry].into_iter().flatten().min()
    }

    /// Applies the OS preference when the window follows the system theme.
    fn system_theme(&mut self, theme: Option<winit::window::Theme>) {
        if self.options.theme == ThemeChoice::System {
            self.context.set_theme(match theme {
                Some(winit::window::Theme::Light) => Theme::light(),
                _ => Theme::dark(),
            });
        }
    }

    /// Installs the configured stylesheet when it is new or changed on disk.
    /// Returns whether a sheet was installed; parse errors keep the previous sheet.
    fn load_stylesheet(&mut self) -> std::result::Result<bool, String> {
        let Some(path) = &self.options.stylesheet else {
            return Ok(false);
        };
        let Some(content) = self.poller.changed(path) else {
            return Ok(false);
        };
        let text =
            String::from_utf8(content?).map_err(|error| format!("{}: {error}", path.display()))?;
        let mut sheet = StyleSheet::parse(&text)
            .map_err(|error| format!("{}:{}: {}", path.display(), error.line, error.message))?;
        // `url(...)` in a sheet is relative to the sheet, not to where the app started.
        if let Some(directory) = path.parent() {
            sheet.rebase(directory);
        }
        self.context.set_stylesheet(sheet);
        Ok(true)
    }

    /// Loads files the view asked for and reloads the ones that changed.
    fn load_watched(&mut self) -> bool {
        let mut changed = false;
        for path in self.context.watched_paths().to_vec() {
            if let Some(content) = self.poller.changed(&path) {
                if let Err(error) = &content {
                    eprintln!("watched file not loaded: {error}");
                }
                changed |= self.context.set_watched_file(&path, content);
            }
        }
        changed
    }

    /// The periodic tick: stylesheet, watched files and the development
    /// runner's stop request.
    fn poll_files(&mut self) {
        let sheet = match self.load_stylesheet() {
            Ok(changed) => changed,
            Err(error) => {
                eprintln!("stylesheet not reloaded: {error}");
                false
            }
        };
        self.images.request(&mut self.context, true);
        if self.load_watched() || sheet {
            self.request_redraw();
        }
        if !self.hot_stopping && hot::stop_requested() {
            self.hot_stopping = true;
            self.hot_save();
            self.close_requested = true;
        }
    }

    /// Declares once more so the view can hand over its state, then writes it
    /// with the window's geometry for the next start.
    fn hot_save(&mut self) {
        let Some(state) = &self.state else {
            return;
        };
        self.context.set_now(self.started.elapsed());
        self.context.request_hot_save();
        {
            let shared = state.renderer.clone();
            let mut shared = shared.borrow_mut();
            self.context.run(
                self.logical_size,
                shared.renderer.text_system(),
                &mut self.view,
            );
        }
        let scale = state.window.scale_factor() as f32;
        let size = state.window.inner_size();
        let window = state
            .window
            .outer_position()
            .ok()
            .filter(|_| size.width > 0 && size.height > 0)
            .map(|position| hot::SavedWindow {
                x: position.x,
                y: position.y,
                width: size.width as f32 / scale,
                height: size.height as f32 / scale,
            });
        let text = self.context.take_hot_state().unwrap_or_default();
        if let Err(error) = hot::save(window, &text) {
            eprintln!("hot state not saved: {error}");
        }
    }

    fn request_redraw(&self) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }

    fn set_overlay(&mut self, overlay: Vec<Primitive>) {
        if self.overlay != overlay {
            self.overlay = overlay;
            self.overlay_dirty = true;
            self.request_redraw();
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let Some(state) = self.state.as_mut() else {
            return Ok(());
        };
        let size = state.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let frame_started = Instant::now();
        let _frame_span = tracing::trace_span!("rust_gui.frame").entered();
        let scale = state.window.scale_factor() as f32;
        let shared_renderer = state.renderer.clone();
        let mut shared = shared_renderer.borrow_mut();
        let window_changed = shared.window != Some(state.window.id());
        // The same for every window of the device: switching it per window
        // would drop the readbacks that are still on their way.
        if let Painter::Gpu(renderer) = &mut shared.renderer {
            renderer.set_profiling(self.profile_gpu);
        }
        let logical_size = Size::new(size.width as f32 / scale, size.height as f32 / scale);
        self.logical_size = logical_size;
        self.context.set_now(self.started.elapsed());
        // Images decoded in the background since the last frame.
        self.images.deliver(&mut self.context);
        let mut stats =
            self.context
                .run(logical_size, shared.renderer.text_system(), &mut self.view);
        // A consumed click may make the view ask for something that itself
        // needs another declaration: a button that switches the language or
        // the theme. Declare until it settles, within reason, so the frame
        // that is presented is never a stale one.
        let mut pending = stats.activations;
        for _ in 0..3 {
            if pending == 0 {
                break;
            }
            let again =
                self.context
                    .run(logical_size, shared.renderer.text_system(), &mut self.view);
            pending = again.activations;
            stats = merge(stats, again);
        }
        if let Some(text) = self.context.take_clipboard() {
            input::write_clipboard(&mut self.clipboard.borrow_mut(), &text);
        }
        // Each new framework diagnostic is printed once; the inspector (F12)
        // lists them with their nodes. They are collected in debug builds.
        let diagnostics = self.context.diagnostics();
        for entry in diagnostics.iter().skip(self.diagnostics_seen) {
            eprintln!(
                "rust-gui diagnostic: {:?} at `{}`: {}",
                entry.kind, entry.path, entry.message
            );
        }
        self.diagnostics_seen = diagnostics.len();
        self.images.request(&mut self.context, false);
        // Files asked for the first time load right away; the next frame shows them.
        let fresh: Vec<_> = self
            .context
            .watched_paths()
            .iter()
            .filter(|path| !self.context.watched_known(path))
            .cloned()
            .collect();
        for path in fresh {
            if let Some(content) = self.poller.changed(&path) {
                if let Err(error) = &content {
                    eprintln!("watched file not loaded: {error}");
                }
                self.context.set_watched_file(&path, content);
                state.window.request_redraw();
            }
        }
        let scene_changed = stats.scene_rebuilt;
        let ime_target = self
            .context
            .focused()
            .filter(|_| self.context.accepts_text());
        if state.ime_target != ime_target {
            if state.ime_target.is_some() {
                state.window.set_ime_allowed(false);
            }
            if ime_target.is_some() {
                state.window.set_ime_allowed(true);
            }
            state.ime_target = ime_target;
        }
        if state.ime_enabled
            && let Some(caret) = self.context.ime_cursor_rect()
        {
            state.window.set_ime_cursor_area(
                LogicalPosition::new(caret.x, caret.y),
                LogicalSize::new(caret.width.max(1.0), caret.height.max(1.0)),
            );
        }
        // Keep invalidation until rendering succeeds. A timeout or lost surface
        // must not consume a scene update before its GPU data has been prepared.
        state.pending_scene |= scene_changed;
        // Publish semantics before surface acquisition: a timeout or occlusion
        // must not prevent assistive technologies receiving current state.
        if (scene_changed || self.accessibility_requested)
            && let Some(adapter) = &mut state.accessibility
        {
            adapter.update_if_active(|| {
                accessibility::tree(&self.context, &self.options.title, f64::from(scale))
            });
            self.accessibility_requested = false;
        }
        if self.frames == 0 {
            diagnostics::checkpoint("first_layout", state.device());
        }
        state.window.set_cursor(
            match self.context.hovered().and_then(|id| self.context.node(id)) {
                Some(node) if node.semantics.role == rust_gui_core::Role::TextInput => {
                    CursorIcon::Text
                }
                Some(node) if node.semantics.role == rust_gui_core::Role::Splitter => {
                    match node.orientation {
                        Some(rust_gui_core::Axis::Vertical) => CursorIcon::RowResize,
                        _ => CursorIcon::ColResize,
                    }
                }
                Some(node) if node.semantics.focusable => CursorIcon::Pointer,
                _ => CursorIcon::Default,
            },
        );
        // The overlay is appended to a copy only while an inspector decorates
        // this window; ordinary frames render the retained scene directly.
        let decorated;
        let scene = if self.overlay.is_empty() {
            self.context.scene()
        } else {
            let base = self.context.scene();
            decorated = Scene {
                size: base.size,
                primitives: base
                    .primitives
                    .iter()
                    .chain(self.overlay.iter())
                    .cloned()
                    .collect(),
            };
            &decorated
        };
        let frame = Frame {
            scene,
            clear: self
                .options
                .background
                .unwrap_or(self.context.theme().window),
            changed: state.pending_scene || window_changed || self.overlay_dirty,
            // Only a frame that was declared knows what changed in it, and
            // only this window's own last frame is there to keep.
            damage: if scene_changed && !window_changed && !self.overlay_dirty {
                self.context.damage_regions()
            } else {
                &[]
            },
            first: self.frames == 0,
        };
        let render = match &mut shared.renderer {
            Painter::Gpu(renderer) => state.present_gpu(renderer, frame)?,
            Painter::Cpu(renderer) => state.present_cpu(renderer, frame)?,
        };
        let render = match render {
            Presented::Drawn(render) => render,
            Presented::Skipped => return Ok(()),
            Presented::Retry => {
                self.retry_pending = true;
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(100),
                ));
                return Ok(());
            }
        };
        state.pending_scene = false;
        self.overlay_dirty = false;
        // Fifo presentation paces this loop to the display while anything animates.
        if self.context.animating() {
            state.window.request_redraw();
        }
        self.frames += 1;
        self.recorder.frame(stats, render, frame_started.elapsed());
        if matches!(self.frames, 60 | 600 | 6000) {
            let stage = match self.frames {
                60 => "frame_60",
                600 => "frame_600",
                _ => "frame_6000",
            };
            if let Painter::Gpu(renderer) = &shared.renderer {
                diagnostics::render_statistics(stage, renderer.stats());
            }
            diagnostics::checkpoint(stage, state.device());
        }
        if let Some(limit) = self.options.frame_limit {
            if self.frames >= limit {
                self.close_requested = true;
            } else {
                state.window.request_redraw();
            }
        }
        if self.profile_gpu {
            let core_frame = self
                .recorder
                .frames()
                .back()
                .map_or(0, |record| record.stats.frame);
            let shared = &mut *shared;
            let window = state.window.id();
            match &shared.renderer {
                // Nothing to measure, and the inspector says why rather than
                // showing counters that would all be zero.
                Painter::Cpu(_) => {
                    if self.inspected {
                        self.recorder.gpu(
                            rust_gui_devtools::GpuSnapshot {
                                adapter: state.adapter_name(),
                                software: true,
                                ..Default::default()
                            },
                            None,
                        );
                    }
                }
                Painter::Gpu(renderer) => {
                    let stats = renderer.stats();
                    let routing = &mut shared.gpu;
                    routing.drawn.push_back((stats.frames, window, core_frame));
                    while routing.drawn.len() > 32 {
                        routing.drawn.pop_front();
                    }
                    // Whatever completed goes to the window that drew it, which may
                    // not be this one.
                    if let Some(profile) = renderer.gpu_profile()
                        && routing.routed != Some(profile.frame)
                    {
                        routing.routed = Some(profile.frame);
                        if let Some((_, owner, frame)) = routing
                            .drawn
                            .iter()
                            .find(|(drawn, ..)| *drawn == profile.frame)
                        {
                            let passes = profile
                                .passes
                                .iter()
                                .map(|pass| (pass.label.clone(), pass.duration))
                                .collect();
                            // ponytail: one entry per window that ever drew while an
                            // inspector was open, overwritten and never pruned; prune
                            // on window close if apps start opening thousands.
                            routing
                                .arrived
                                .insert(*owner, (*frame, profile.total, passes));
                        }
                    }
                    if self.inspected {
                        let arrived = routing.arrived.remove(&window);
                        let memory = stats.memory;
                        let snapshot = rust_gui_devtools::GpuSnapshot {
                            adapter: state.adapter_name(),
                            software: false,
                            timestamps: renderer.timestamps_supported(),
                            passes: match &arrived {
                                Some((_, _, passes)) => passes.clone(),
                                None => self
                                    .recorder
                                    .gpu_snapshot()
                                    .map(|last| last.passes.clone())
                                    .unwrap_or_default(),
                            },
                            draw_calls: stats.draw_calls as u32,
                            quads: stats.quads as u32,
                            text_batches: stats.text_batches as u32,
                            render_passes: stats.passes as u32,
                            memory: vec![
                                ("Vertex and uniform buffers".into(), memory.buffers),
                                ("Image atlas".into(), memory.image_atlas),
                                ("Image textures".into(), memory.image_textures),
                                ("Offscreen layers".into(), memory.layers),
                                ("Profiling".into(), memory.profiling),
                            ],
                            memory_peak: memory.peak,
                            driver: state
                                .device()
                                .and_then(Renderer::driver_memory)
                                .map(|driver| (driver.allocated, driver.reserved)),
                        };
                        self.recorder
                            .gpu(snapshot, arrived.map(|(frame, total, _)| (frame, total)));
                    }
                }
            }
        }
        shared.window = Some(state.window.id());
        Ok(())
    }

    fn input(&mut self, event: Event) {
        self.offer(event);
    }

    /// Like `input`, and says whether the event was taken.
    fn offer(&mut self, event: Event) -> bool {
        self.sync_clock();
        let description = self.options.devtools.then(|| format!("{event:?}"));
        let handled = self.context.event(event);
        if let Some(description) = description {
            self.recorder
                .event(self.context.stats().frame, description, handled);
        }
        if handled {
            self.request_redraw();
        }
        handled
    }

    /// Tells the source of a system drag how it ended, once that is known.
    fn poll_os_drag(&mut self) {
        use std::sync::mpsc::TryRecvError;
        let outcome = match self.os_drag.as_ref().map(|done| done.try_recv()) {
            None | Some(Err(TryRecvError::Empty)) => return,
            Some(Ok(outcome)) => outcome,
            Some(Err(TryRecvError::Disconnected)) => rust_gui_core::DragOutcome::Cancelled,
        };
        self.os_drag = None;
        self.context.finish_exported_drag(outcome);
        self.request_redraw();
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: Box<dyn Error>) {
        self.error = Some(error);
        event_loop.exit();
    }

    fn accessibility(&mut self, event_loop: &ActiveEventLoop, event: accesskit_winit::Event) {
        let Some(state) = &self.state else {
            return;
        };
        if state.window.id() != event.window_id {
            return;
        }
        match event.window_event {
            accesskit_winit::WindowEvent::InitialTreeRequested => {
                self.accessibility_requested = true;
                if let Err(error) = self.redraw(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            accesskit_winit::WindowEvent::ActionRequested(request) => {
                let focus_window = request.action == accesskit::Action::Focus;
                if accessibility::action(&mut self.context, request, state.window.scale_factor()) {
                    if focus_window {
                        state.window.focus_window();
                    }
                    state.window.request_redraw();
                }
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {}
        }
    }

    fn new_events(&mut self, cause: StartCause) {
        self.poll_os_drag();
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            if std::mem::take(&mut self.retry_pending) {
                self.request_redraw();
            }
            if self
                .context
                .next_wake()
                .is_some_and(|at| self.started.elapsed() >= at)
            {
                self.request_redraw();
            }
        }
        // By the clock, not by timer wakeups: a window that redraws every frame
        // (an animation) never sleeps, and its files must still be polled.
        if self.last_poll.elapsed() >= STYLESHEET_POLL {
            self.last_poll = Instant::now();
            self.poll_files();
        }
    }

    fn suspended(&mut self) {
        self.context.event(Event::FocusLost);
        self.state = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if let Some(adapter) = &mut state.accessibility {
            adapter.process_event(&state.window, &event);
        }
        match event {
            WindowEvent::CloseRequested => self.close_requested = true,
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.accessibility_requested = true;
                state.resize();
                state.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::Occluded(false) => state.window.request_redraw(),
            WindowEvent::ThemeChanged(theme) => {
                state.window.request_redraw();
                self.system_theme(Some(theme));
            }
            WindowEvent::Focused(focused) => {
                if !focused {
                    self.modifiers = ModifiersState::empty();
                }
                if input::window_focus(&mut self.context, &mut self.restore_focus, focused) {
                    state.window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let position = position.to_logical::<f32>(state.window.scale_factor());
                self.pointer = Point {
                    x: position.x,
                    y: position.y,
                };
                let size = state
                    .window
                    .inner_size()
                    .to_logical::<f32>(state.window.scale_factor());
                let outside = position.x < 0.0
                    || position.y < 0.0
                    || position.x >= size.width
                    || position.y >= size.height;
                self.input(Event::PointerMoved(self.pointer));
                // A drag that leaves the window becomes the system's, if the
                // platform can carry its payload; otherwise it stays ours and
                // may still come back.
                if outside
                    && self.os_drag.is_none()
                    && let Some(payload) = self.context.dragging()
                    && let Some(state) = &self.state
                    && let Some(done) = os_drag::start(&state.window, payload)
                {
                    self.context.export_drag();
                    self.os_drag = Some(done);
                    self.poll_os_drag();
                }
            }
            // ponytail: winit 0.30 carries no drop position and Windows sends no
            // CursorMoved during an OLE drag, so this uses the last known pointer;
            // upgrade path is a platform cursor query at drop time.
            WindowEvent::DroppedFile(path) => self.input(Event::DropAt {
                position: self.pointer,
                payload: DragPayload::Files(vec![path]),
            }),
            WindowEvent::CursorLeft { .. } => self.input(Event::PointerLeft),
            // Trackpad gestures, which winit reports on macOS only.
            WindowEvent::PinchGesture { delta, .. } => {
                self.input(Event::Zoom(1.0 + delta as f32));
            }
            // winit counts counterclockwise degrees; the core clockwise radians.
            WindowEvent::RotationGesture { delta, .. } => {
                self.input(Event::Rotate(-delta.to_radians()));
            }
            // Fingers and pens. Windows does not repeat them as mouse events
            // once winit has taken the pointer messages.
            WindowEvent::Touch(touch) => {
                let position = touch
                    .location
                    .to_logical::<f32>(state.window.scale_factor());
                self.input(Event::Touch {
                    id: touch.id,
                    phase: match touch.phase {
                        winit::event::TouchPhase::Started => rust_gui_core::TouchPhase::Started,
                        winit::event::TouchPhase::Moved => rust_gui_core::TouchPhase::Moved,
                        winit::event::TouchPhase::Ended => rust_gui_core::TouchPhase::Ended,
                        winit::event::TouchPhase::Cancelled => rust_gui_core::TouchPhase::Cancelled,
                    },
                    position: Point {
                        x: position.x,
                        y: position.y,
                    },
                    force: touch.force.map(|force| force.normalized() as f32),
                    // winit reports no tilt or barrel button in 0.30.
                    pen: None,
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (mut delta_x, mut delta_y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (-x * 40.0, -y * 40.0),
                    MouseScrollDelta::PixelDelta(position) => {
                        let scale = state.window.scale_factor();
                        (-(position.x / scale) as f32, -(position.y / scale) as f32)
                    }
                };
                // Ctrl with the wheel zooms a gesture area, and so does a
                // pinch on a Windows touchpad, which arrives as exactly that.
                // One notch is a tenth. Elsewhere the wheel scrolls as usual.
                if self.modifiers.control_key()
                    && delta_y != 0.0
                    && self.offer(Event::Zoom((-delta_y / 400.0).exp()))
                {
                    return;
                }
                // Shift turns a plain wheel into a sideways one.
                if self.modifiers.shift_key() && delta_x == 0.0 {
                    (delta_x, delta_y) = (delta_y, 0.0);
                }
                self.input(Event::Scroll { delta_x, delta_y });
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Right,
                ..
            } => self.input(Event::ContextMenu(self.pointer)),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.input(if state == ElementState::Pressed {
                    if self.modifiers.alt_key() {
                        // Adds a caret in a code editor; elsewhere it does nothing.
                        Event::Editor(rust_gui_core::EditorCommand::AddCursorAt(self.pointer))
                    } else if self.modifiers.shift_key() {
                        Event::PointerDownWithShift
                    } else {
                        Event::PointerDown
                    }
                } else {
                    Event::PointerUp
                });
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            // Focus synchronization reports already-held keys as synthetic
            // presses. They must not type characters or activate controls.
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                self.keyboard(event);
            }
            WindowEvent::Ime(ime) => match ime {
                Ime::Enabled => {
                    state.ime_enabled = true;
                    state.window.request_redraw();
                }
                Ime::Disabled => {
                    state.ime_enabled = false;
                    self.input(Event::Text(TextEdit::CancelComposition));
                }
                Ime::Preedit(text, selection) => {
                    self.input(Event::Text(TextEdit::Preedit { text, selection }))
                }
                Ime::Commit(text) => self.input(Event::Text(TextEdit::Commit(text))),
            },
            _ => {}
        }
    }
}

/// Combines the two declarations of one presented frame into one record.
fn merge(
    first: rust_gui_core::FrameStats,
    second: rust_gui_core::FrameStats,
) -> rust_gui_core::FrameStats {
    rust_gui_core::FrameStats {
        activations: first.activations + second.activations,
        created: first.created + second.created,
        reused: first.reused + second.reused,
        removed: first.removed + second.removed,
        skipped: first.skipped + second.skipped,
        text_measures: first.text_measures + second.text_measures,
        layout_passes: first.layout_passes + second.layout_passes,
        scene_rebuilt: first.scene_rebuilt || second.scene_rebuilt,
        dirty: first.dirty | second.dirty,
        paint_nodes: first.paint_nodes + second.paint_nodes,
        paint_cache_hits: first.paint_cache_hits + second.paint_cache_hits,
        frame: second.frame,
        declare: first.declare + second.declare,
        layout: first.layout + second.layout,
        paint: first.paint + second.paint,
    }
}
