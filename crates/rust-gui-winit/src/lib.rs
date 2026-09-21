//! Event-driven desktop host with logical coordinates and a wgpu surface.
mod accessibility;
mod desktop;
mod diagnostics;
mod dock;
mod graphics;
mod input;
mod mailbox;
mod wakeup;
pub use desktop::{DesktopClosed, DesktopHandle, WindowKey, run_windows};
pub use dock::{DockWindowView, DockWindows};
pub use graphics::{GpuMemoryPolicy, GraphicsBackend, GraphicsOptions};
pub use mailbox::{Mailbox, MessageSender};
use rust_gui_core::{Action, Context, DragPayload, Event, StyleSheet, TextEdit, Theme, Ui};
use rust_gui_devtools::Recorder;
use rust_gui_render_wgpu::Renderer;
use rust_gui_scene::{Color, Point, Primitive, Scene, Size};
use std::{
    cell::RefCell,
    error::Error,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
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
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Rc<RefCell<SharedRenderer>>,
    pending_scene: bool,
    accessibility: Option<accesskit_winit::Adapter>,
    window: Arc<Window>,
    ime_target: Option<rust_gui_core::NodeId>,
    ime_enabled: bool,
}

struct SharedRenderer {
    renderer: Renderer,
    window: Option<WindowId>,
}

impl WindowState {
    async fn new(
        window: Arc<Window>,
        event_loop: &ActiveEventLoop,
        options: &GraphicsOptions,
    ) -> Result<Self> {
        // Initializing every compiled backend loads unused driver stacks and
        // enumerates their adapters. Try one backend, dropping failed attempts
        // before opening the next. WGPU_BACKEND restricts the candidate set.
        let environment = std::env::var("WGPU_BACKEND").ok();
        let candidates = options.candidates(
            wgpu::Instance::enabled_backend_features(),
            environment.as_deref(),
        );
        graphics::first_available(candidates, |backend| {
            Self::with_backend(window.clone(), event_loop, backend, options)
        })
        .await
        .map_err(Into::into)
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
            instance,
            adapter,
            device,
            queue,
            surface,
            config,
            renderer: Rc::new(RefCell::new(SharedRenderer {
                renderer,
                window: None,
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
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
        }
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
    stylesheet_modified: Option<SystemTime>,
    retry_pending: bool,
    started: Instant,
}

impl<F: FnMut(&mut Ui<'_>)> Application<F> {
    fn new(
        options: AppOptions,
        view: F,
        state: WindowState,
        clipboard: Rc<RefCell<Option<arboard::Clipboard>>>,
    ) -> Self {
        let mut context = Context::new();
        context.set_reduced_motion(options.reduced_motion);
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
            stylesheet_modified: None,
            retry_pending: false,
            started: Instant::now(),
        }
    }

    fn sync_clock(&mut self) {
        self.context.set_now(self.started.elapsed());
    }

    /// The earliest moment this window needs the loop to wake without input.
    fn deadline(&self) -> Option<Instant> {
        let wake = self.context.next_wake().map(|at| self.started + at);
        let poll = self
            .options
            .stylesheet
            .is_some()
            .then(|| Instant::now() + STYLESHEET_POLL);
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
        let modified = std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if self.stylesheet_modified == Some(modified) {
            return Ok(false);
        }
        self.stylesheet_modified = Some(modified);
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let sheet = StyleSheet::parse(&text)
            .map_err(|error| format!("{}:{}: {}", path.display(), error.line, error.message))?;
        self.context.set_stylesheet(sheet);
        Ok(true)
    }

    fn poll_stylesheet(&mut self) {
        match self.load_stylesheet() {
            Ok(true) => self.request_redraw(),
            Ok(false) => {}
            Err(error) => eprintln!("stylesheet not reloaded: {error}"),
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
        let renderer = &mut shared.renderer;
        let logical_size = Size::new(size.width as f32 / scale, size.height as f32 / scale);
        self.logical_size = logical_size;
        self.context.set_now(self.started.elapsed());
        let mut stats = self
            .context
            .run(logical_size, renderer.text_system(), &mut self.view);
        if stats.activations > 0 {
            let second = self
                .context
                .run(logical_size, renderer.text_system(), &mut self.view);
            stats = merge(stats, second);
        }
        if let Some(text) = self.context.take_clipboard() {
            input::write_clipboard(&mut self.clipboard.borrow_mut(), &text);
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
            diagnostics::checkpoint("first_layout", Some(&state.device));
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
        let (frame, suboptimal) = match state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            wgpu::CurrentSurfaceTexture::Timeout => {
                self.retry_pending = true;
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(100),
                ));
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                state.resize();
                state.window.request_redraw();
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                state.surface = state.instance.create_surface(state.window.clone())?;
                state.resize();
                state.window.request_redraw();
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Surface validation failed".into());
            }
        };
        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let render_started = Instant::now();
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
        {
            let _render_span = tracing::trace_span!("rust_gui.render").entered();
            renderer.render_cached(
                &state.device,
                &state.queue,
                &target,
                [size.width, size.height],
                scale,
                scene,
                self.options
                    .background
                    .unwrap_or(self.context.theme().window),
                state.pending_scene || window_changed || self.overlay_dirty,
            )?;
        }
        state.pending_scene = false;
        self.overlay_dirty = false;
        if self.frames == 0 {
            diagnostics::checkpoint("first_render", Some(&state.device));
        }
        state.window.pre_present_notify();
        frame.present();
        // Fifo presentation paces this loop to the display while anything animates.
        if self.context.animating() {
            state.window.request_redraw();
        }
        let render = render_started.elapsed();
        if self.frames == 0 {
            diagnostics::checkpoint("first_present", Some(&state.device));
        }
        if suboptimal {
            state.resize();
        }
        self.frames += 1;
        self.recorder.frame(stats, render, frame_started.elapsed());
        if matches!(self.frames, 60 | 600 | 6000) {
            let stage = match self.frames {
                60 => "frame_60",
                600 => "frame_600",
                _ => "frame_6000",
            };
            diagnostics::render_statistics(stage, renderer.stats());
            diagnostics::checkpoint(stage, Some(&state.device));
        }
        if let Some(limit) = self.options.frame_limit {
            if self.frames >= limit {
                self.close_requested = true;
            } else {
                state.window.request_redraw();
            }
        }
        shared.window = Some(state.window.id());
        Ok(())
    }

    fn input(&mut self, event: Event) {
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
            self.poll_stylesheet();
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
                self.input(Event::PointerMoved(self.pointer));
            }
            // ponytail: winit 0.30 carries no drop position and Windows sends no
            // CursorMoved during an OLE drag, so this uses the last known pointer;
            // upgrade path is a platform cursor query at drop time.
            WindowEvent::DroppedFile(path) => self.input(Event::DropAt {
                position: self.pointer,
                payload: DragPayload::Files(vec![path]),
            }),
            WindowEvent::CursorLeft { .. } => self.input(Event::PointerLeft),
            WindowEvent::MouseWheel { delta, .. } => {
                let delta_y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -y * 40.0,
                    MouseScrollDelta::PixelDelta(position) => {
                        -(position.y / state.window.scale_factor()) as f32
                    }
                };
                self.input(Event::Scroll { delta_y });
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
