use super::*;
use rust_gui_devtools::{Inspector, InspectorModel, InspectorResponse};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) type View = Box<dyn FnMut(&mut Ui<'_>)>;
type SendView = Box<dyn FnMut(&mut Ui<'_>) + Send>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowKey(pub u64);

#[derive(Clone, Copy, Debug)]
pub struct DesktopClosed;
impl std::fmt::Display for DesktopClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("desktop event loop is closed")
    }
}
impl Error for DesktopClosed {}

pub(crate) enum WindowRequest {
    Open {
        key: WindowKey,
        options: Box<AppOptions>,
        view: SendView,
        /// A window that stays blocked while this one is open.
        owner: Option<WindowKey>,
    },
    Close(WindowKey),
    Redraw(WindowKey),
}

/// Thread-safe desktop requests. View closures are transferred to the UI thread;
/// share application state between them with Arc/Mutex or bounded mailboxes.
#[derive(Clone)]
pub struct DesktopHandle {
    proxy: EventLoopProxy<UserEvent>,
    next: Arc<AtomicU64>,
    wake: WakeHandle,
}
impl DesktopHandle {
    pub fn open_window(
        &self,
        options: AppOptions,
        view: impl FnMut(&mut Ui<'_>) + Send + 'static,
    ) -> std::result::Result<WindowKey, DesktopClosed> {
        self.open(options, view, None)
    }

    /// A native modal window. The owner ignores input and hands focus to the
    /// dialog until the dialog closes; on Windows it is also an owned window.
    pub fn open_modal(
        &self,
        owner: WindowKey,
        options: AppOptions,
        view: impl FnMut(&mut Ui<'_>) + Send + 'static,
    ) -> std::result::Result<WindowKey, DesktopClosed> {
        self.open(options, view, Some(owner))
    }

    fn open(
        &self,
        options: AppOptions,
        view: impl FnMut(&mut Ui<'_>) + Send + 'static,
        owner: Option<WindowKey>,
    ) -> std::result::Result<WindowKey, DesktopClosed> {
        if self.wake.is_closed() {
            return Err(DesktopClosed);
        }
        let key = WindowKey(
            self.next
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| DesktopClosed)?,
        );
        self.proxy
            .send_event(UserEvent::Window(WindowRequest::Open {
                key,
                options: Box::new(options),
                view: Box::new(view),
                owner,
            }))
            .map_err(|_| DesktopClosed)?;
        Ok(key)
    }
    pub fn close_window(&self, key: WindowKey) -> bool {
        !self.wake.is_closed()
            && self
                .proxy
                .send_event(UserEvent::Window(WindowRequest::Close(key)))
                .is_ok()
    }
    pub fn redraw(&self, key: WindowKey) -> bool {
        !self.wake.is_closed()
            && self
                .proxy
                .send_event(UserEvent::Window(WindowRequest::Redraw(key)))
                .is_ok()
    }
    pub fn wakeup(&self) -> WakeHandle {
        self.wake.clone()
    }
    pub fn request_exit(&self) -> bool {
        self.wake.request_exit()
    }
}

/// One event loop, graphics device, font system, glyph atlas and clipboard owner
/// for multiple independently focused windows. All windows use `graphics`;
/// their AppOptions graphics field is superseded by this desktop-wide policy.
/// Setup must enqueue at least one window. Closing the last window exits.
pub fn run_windows(graphics: GraphicsOptions, setup: impl FnOnce(DesktopHandle)) -> Result<()> {
    let _ = crate::STARTED.set(std::time::Instant::now());
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let wake = WakeHandle::new(proxy.clone());
    let next = Arc::new(AtomicU64::new(1));
    setup(DesktopHandle {
        proxy: proxy.clone(),
        next: next.clone(),
        wake: wake.clone(),
    });
    if next.load(Ordering::Acquire) == 1 {
        wake.close();
        return Err("run_windows setup must open at least one window".into());
    }
    run(event_loop, Desktop::new(proxy, wake, next, graphics))
}

/// One window whose view need not be `Send`. Backs `run` and `run_with_wakeup`.
pub(crate) fn run_single<F: FnMut(&mut Ui<'_>) + 'static>(
    options: AppOptions,
    setup: impl FnOnce(WakeHandle) -> F,
) -> Result<()> {
    diagnostics::checkpoint("process", None);
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let wake = WakeHandle::new(proxy.clone());
    let view = setup(wake.clone());
    diagnostics::checkpoint("event_loop", None);
    let graphics = options.graphics.clone();
    let mut desktop = Desktop::new(proxy, wake, Arc::new(AtomicU64::new(2)), graphics);
    desktop.pending.push(Pending {
        key: WindowKey(1),
        options,
        view: Box::new(view),
        owner: None,
    });
    run(event_loop, desktop)
}

fn run(event_loop: EventLoop<UserEvent>, mut desktop: Desktop) -> Result<()> {
    event_loop.set_control_flow(ControlFlow::Wait);
    let result = event_loop.run_app(&mut desktop);
    desktop.wake.close();
    result?;
    match desktop.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct Pending {
    key: WindowKey,
    options: AppOptions,
    view: View,
    owner: Option<WindowKey>,
}

struct InspectorShared {
    inspector: Inspector,
    model: InspectorModel,
    response: InspectorResponse,
}

struct InspectorLink {
    target: WindowKey,
    shared: Rc<RefCell<InspectorShared>>,
}

struct Desktop {
    windows: HashMap<WindowKey, Application<View>>,
    pending: Vec<Pending>,
    /// Inspector window → inspected window.
    inspectors: HashMap<WindowKey, InspectorLink>,
    /// Native modal dialog → owner window blocked while it is open.
    modals: HashMap<WindowKey, WindowKey>,
    resumed: bool,
    opened: bool,
    proxy: EventLoopProxy<UserEvent>,
    wake: WakeHandle,
    next: Arc<AtomicU64>,
    graphics: GraphicsOptions,
    clipboard: Rc<RefCell<Option<arboard::Clipboard>>>,
    error: Option<Box<dyn Error>>,
}

impl WindowState {
    fn sharing(window: Arc<Window>, source: &Self) -> Result<Self> {
        let Surfaces::Gpu {
            instance,
            adapter,
            device,
            queue,
            config,
            ..
        } = &source.surfaces
        else {
            let Surfaces::Cpu { context, .. } = &source.surfaces else {
                unreachable!()
            };
            return Self::cpu(window, context.clone(), Some(source.renderer.clone()));
        };
        let surface = instance.create_surface(window.clone())?;
        let capabilities = surface.get_capabilities(adapter);
        if !capabilities.formats.contains(&config.format) {
            return Err(
                "new window cannot present using the shared graphics device and format".into(),
            );
        }
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            width: size.width.max(1),
            height: size.height.max(1),
            ..config.clone()
        };
        surface.configure(device, &config);
        #[cfg(feature = "diagnostics")]
        eprintln!("GPU_SHARED_DEVICE window={:?}", window.id());
        Ok(Self {
            surfaces: Surfaces::Gpu {
                instance: instance.clone(),
                adapter: adapter.clone(),
                device: device.clone(),
                queue: queue.clone(),
                surface,
                config,
            },
            renderer: source.renderer.clone(),
            pending_scene: true,
            accessibility: None,
            window,
            ime_target: None,
            ime_enabled: false,
        })
    }
}

impl Desktop {
    /// The screen a window should open on, or `None` to leave it to the
    /// system. A named screen that is not there falls back to the primary
    /// one, so unplugging a monitor does not hide a window.
    fn screen_for(
        &self,
        event_loop: &ActiveEventLoop,
        owner: Option<WindowKey>,
        placement: &Placement,
    ) -> Option<winit::monitor::MonitorHandle> {
        let primary = || {
            event_loop
                .primary_monitor()
                .or_else(|| event_loop.available_monitors().next())
        };
        match placement {
            Placement::System => None,
            Placement::Centered => primary(),
            Placement::WithOwner => owner
                .and_then(|key| self.windows.get(&key)?.state.as_ref())
                .and_then(|state| state.window.current_monitor())
                .or_else(primary),
            Placement::Named(name) => event_loop
                .available_monitors()
                .find(|screen| {
                    screen
                        .name()
                        .is_some_and(|screen| screen.to_lowercase().contains(&name.to_lowercase()))
                })
                .or_else(primary),
        }
    }

    fn new(
        proxy: EventLoopProxy<UserEvent>,
        wake: WakeHandle,
        next: Arc<AtomicU64>,
        graphics: GraphicsOptions,
    ) -> Self {
        Self {
            windows: HashMap::new(),
            pending: Vec::new(),
            inspectors: HashMap::new(),
            modals: HashMap::new(),
            resumed: false,
            opened: false,
            proxy,
            wake,
            next,
            graphics,
            clipboard: Rc::new(RefCell::new(None)),
            error: None,
        }
    }

    fn allocate_key(&self) -> WindowKey {
        WindowKey(self.next.fetch_add(1, Ordering::AcqRel))
    }

    fn attach(
        &self,
        event_loop: &ActiveEventLoop,
        options: &AppOptions,
        owner: Option<WindowKey>,
    ) -> Result<WindowState> {
        let mut attributes = Window::default_attributes()
            .with_visible(false)
            .with_title(&options.title)
            .with_inner_size(LogicalSize::new(options.size.width, options.size.height));
        // Where the window opens, when the application asked for something
        // other than what the system would do.
        let screen = self.screen_for(event_loop, owner, &options.placement);
        // A development restart puts the first window back where it was.
        if owner.is_none()
            && self.windows.is_empty()
            && let Some(saved) = hot::saved_window()
        {
            attributes = attributes
                .with_inner_size(LogicalSize::new(saved.width, saved.height))
                .with_position(winit::dpi::PhysicalPosition::new(saved.x, saved.y));
        }
        #[cfg(target_os = "windows")]
        if let Some(owner) = owner.and_then(|key| self.windows.get(&key)?.state.as_ref()) {
            use winit::platform::windows::WindowAttributesExtWindows;
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = owner.window.window_handle()
                && let RawWindowHandle::Win32(win32) = handle.as_raw()
            {
                attributes = attributes.with_owner_window(win32.hwnd.get());
            }
        }
        #[cfg(not(target_os = "windows"))]
        let _ = owner;
        let window = Arc::new(event_loop.create_window(attributes)?);
        // Centred by the frame the user sees, which is only known once the
        // window exists, and in the screen's own pixels.
        if let Some(screen) = screen {
            let (size, screen_size) = (window.outer_size(), screen.size());
            let corner = screen.position();
            window.set_outer_position(winit::dpi::PhysicalPosition::new(
                corner.x + (screen_size.width as i32 - size.width as i32) / 2,
                corner.y + (screen_size.height as i32 - size.height as i32) / 2,
            ));
        }
        diagnostics::checkpoint("native_window", None);
        let mut state =
            if let Some(source) = self.windows.values().find_map(|app| app.state.as_ref()) {
                WindowState::sharing(window, source)?
            } else {
                pollster::block_on(WindowState::new(window, event_loop, &self.graphics))?
            };
        // What the application draws itself, now that there is a renderer to
        // register it with. A window without an adapter has none.
        if let Some(setup) = &options.gpu
            && let Painter::Gpu(renderer) = &mut state.renderer.borrow_mut().renderer
            && let Ok(mut setup) = setup.0.lock()
        {
            setup(renderer);
        }
        state.accessibility = Some(accesskit_winit::Adapter::with_event_loop_proxy(
            event_loop,
            &state.window,
            self.proxy.clone(),
        ));
        state.window.set_visible(true);
        state.window.request_redraw();
        Ok(state)
    }

    fn open(
        &mut self,
        event_loop: &ActiveEventLoop,
        key: WindowKey,
        mut options: AppOptions,
        view: View,
        owner: Option<WindowKey>,
    ) -> bool {
        options.graphics = self.graphics.clone();
        let owner = owner.filter(|owner| self.windows.contains_key(owner));
        match self.attach(event_loop, &options, owner) {
            Ok(state) => {
                let mut app = Application::new(
                    options,
                    view,
                    state,
                    self.clipboard.clone(),
                    self.wake.clone(),
                );
                if let ThemeChoice::Fixed(theme) = &app.options.theme {
                    app.context.set_theme(theme.clone());
                } else {
                    let theme = app.state.as_ref().and_then(|state| state.window.theme());
                    app.system_theme(theme);
                }
                if let Err(error) = app.load_stylesheet() {
                    self.error = Some(error.into());
                    event_loop.exit();
                    return false;
                }
                // RUST_GUI_INSPECT opens the inspector immediately, for smoke
                // runs and for tools without keyboard focus.
                app.inspector_requested =
                    app.options.devtools && std::env::var_os("RUST_GUI_INSPECT").is_some();
                self.windows.insert(key, app);
                if let Some(owner) = owner {
                    self.modals.insert(key, owner);
                }
                self.opened = true;
                true
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
                false
            }
        }
    }

    fn request(&mut self, event_loop: &ActiveEventLoop, request: WindowRequest) {
        match request {
            WindowRequest::Open {
                key,
                options,
                view,
                owner,
            } => {
                self.open(event_loop, key, *options, view, owner);
            }
            WindowRequest::Close(key) => {
                self.windows.remove(&key);
            }
            WindowRequest::Redraw(key) => {
                if let Some(state) = self.windows.get(&key).and_then(|app| app.state.as_ref()) {
                    state.window.request_redraw();
                }
            }
        }
        self.finish(event_loop);
    }

    /// Requests received before the event loop resumed.
    fn queue(&mut self, request: WindowRequest) {
        match request {
            WindowRequest::Open {
                key,
                options,
                view,
                owner,
            } => self.pending.push(Pending {
                key,
                options: *options,
                view,
                owner,
            }),
            WindowRequest::Close(key) => self.pending.retain(|pending| pending.key != key),
            WindowRequest::Redraw(_) => {}
        }
    }

    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        for app in self.windows.values_mut() {
            if let Some(error) = app.error.take() {
                self.error = Some(error);
                event_loop.exit();
            }
        }
        self.windows.retain(|_, app| !app.close_requested);
        // A closed owner takes its native dialogs with it; a closed dialog
        // unblocks its owner.
        let orphaned: Vec<_> = self
            .modals
            .iter()
            .filter(|(_, owner)| !self.windows.contains_key(owner))
            .map(|(dialog, _)| *dialog)
            .collect();
        for dialog in orphaned {
            self.windows.remove(&dialog);
        }
        self.modals
            .retain(|dialog, _| self.windows.contains_key(dialog));
        self.sync_inspectors(event_loop);
        if self.opened && self.windows.is_empty() {
            event_loop.exit();
        }
        self.schedule(event_loop);
    }

    /// Sleeps until the earliest window deadline (tooltips, stylesheet polling,
    /// surface retries), or indefinitely when nothing is pending.
    fn schedule(&self, event_loop: &ActiveEventLoop) {
        let next = self
            .windows
            .values()
            .filter_map(Application::deadline)
            .min();
        event_loop.set_control_flow(match next {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        });
    }

    fn inspector_of(&self, target: WindowKey) -> Option<WindowKey> {
        self.inspectors
            .iter()
            .find(|(_, link)| link.target == target)
            .map(|(key, _)| *key)
    }

    fn sync_inspectors(&mut self, event_loop: &ActiveEventLoop) {
        // A closed inspector window releases its target's overlay.
        let closed: Vec<_> = self
            .inspectors
            .keys()
            .filter(|key| !self.windows.contains_key(key))
            .copied()
            .collect();
        for key in closed {
            let link = self.inspectors.remove(&key).unwrap();
            if let Some(target) = self.windows.get_mut(&link.target) {
                target.set_overlay(Vec::new());
            }
        }
        // An inspector does not outlive its target.
        let orphaned: Vec<_> = self
            .inspectors
            .iter()
            .filter(|(_, link)| !self.windows.contains_key(&link.target))
            .map(|(key, _)| *key)
            .collect();
        for key in orphaned {
            self.inspectors.remove(&key);
            self.windows.remove(&key);
        }
        let requested: Vec<_> = self
            .windows
            .iter_mut()
            .filter_map(|(key, app)| std::mem::take(&mut app.inspector_requested).then_some(*key))
            .collect();
        for target in requested {
            if self.inspectors.contains_key(&target) {
                continue;
            }
            match self.inspector_of(target) {
                Some(existing) => {
                    if let Some(state) = self
                        .windows
                        .get(&existing)
                        .and_then(|app| app.state.as_ref())
                    {
                        state.window.focus_window();
                    }
                }
                None => self.open_inspector(event_loop, target),
            }
        }
    }

    fn open_inspector(&mut self, event_loop: &ActiveEventLoop, target: WindowKey) {
        let Some(app) = self.windows.get(&target) else {
            return;
        };
        let options = AppOptions {
            title: format!("Inspector · {}", app.options.title),
            size: Size::new(760.0, 820.0),
            frame_limit: app.options.frame_limit,
            devtools: false,
            ..AppOptions::default()
        };
        let shared = Rc::new(RefCell::new(InspectorShared {
            inspector: Inspector::default(),
            model: InspectorModel::default(),
            response: InspectorResponse::default(),
        }));
        let view_shared = shared.clone();
        let view: View = Box::new(move |ui| {
            let shared = &mut *view_shared.borrow_mut();
            shared.response = shared.inspector.ui(ui, &shared.model);
        });
        let key = self.allocate_key();
        if self.open(event_loop, key, options, view, None) {
            self.inspectors
                .insert(key, InspectorLink { target, shared });
            self.refresh_inspector(key);
        }
    }

    /// Copies the target's current tree and history before the inspector declares.
    fn refresh_inspector(&self, key: WindowKey) {
        let Some(link) = self.inspectors.get(&key) else {
            return;
        };
        let (Some(target), Some(own)) = (self.windows.get(&link.target), self.windows.get(&key))
        else {
            return;
        };
        let mut shared = link.shared.borrow_mut();
        let export_path = shared.model.export_path.take();
        let viewport = shared.model.viewport;
        shared.model = InspectorModel::capture(
            &target.context,
            &target.options.title,
            target.logical_size,
            &target.recorder,
        );
        shared.model.viewport = if own.logical_size.width > 0.0 {
            own.logical_size
        } else {
            viewport
        };
        shared.model.export_path = export_path;
    }

    /// Applies the inspector's overlay and requests to its target after it declared.
    fn apply_inspector(&mut self, key: WindowKey) {
        let Some(link) = self.inspectors.get(&key) else {
            return;
        };
        let target_key = link.target;
        let (overlay, response) = {
            let mut shared = link.shared.borrow_mut();
            let overlay = shared.inspector.overlay(&shared.model);
            (overlay, std::mem::take(&mut shared.response))
        };
        let Some(target) = self.windows.get_mut(&target_key) else {
            return;
        };
        target.set_overlay(overlay);
        if let Some(id) = response.focus
            && target.context.action(id, Action::Focus)
        {
            target.request_redraw();
        }
        if response.export_csv {
            let path = std::env::current_dir()
                .unwrap_or_default()
                .join(format!("rust-gui-frames-{}.csv", target_key.0));
            let written = match std::fs::write(&path, target.recorder.csv()) {
                Ok(()) => path.display().to_string(),
                Err(error) => format!("failed: {error}"),
            };
            link.shared.borrow_mut().model.export_path = Some(written);
            if let Some(state) = self.windows.get(&key).and_then(|app| app.state.as_ref()) {
                state.window.request_redraw();
            }
        }
    }

    fn window_key(&self, id: WindowId) -> Option<WindowKey> {
        self.windows.iter().find_map(|(key, app)| {
            app.state
                .as_ref()
                .is_some_and(|state| state.window.id() == id)
                .then_some(*key)
        })
    }
}

impl ApplicationHandler<UserEvent> for Desktop {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.resumed = true;
        for Pending {
            key,
            options,
            view,
            owner,
        } in std::mem::take(&mut self.pending)
        {
            if !self.open(event_loop, key, options, view, owner) {
                return;
            }
        }
        let restore: Vec<_> = self
            .windows
            .iter()
            .filter(|(_, app)| app.state.is_none())
            .map(|(key, app)| (*key, app.options.clone()))
            .collect();
        for (key, options) in restore {
            match self.attach(event_loop, &options, None) {
                Ok(state) => self.windows.get_mut(&key).unwrap().state = Some(state),
                Err(error) => {
                    self.error = Some(error);
                    event_loop.exit();
                    return;
                }
            }
        }
        self.finish(event_loop);
    }
    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.resumed = false;
        for app in self.windows.values_mut() {
            app.suspended();
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Window(request) => {
                if self.resumed {
                    self.request(event_loop, request);
                } else {
                    self.queue(request);
                }
            }
            UserEvent::Exit => event_loop.exit(),
            UserEvent::Wake => {
                self.wake.acknowledge();
                for app in self.windows.values() {
                    app.request_redraw();
                }
            }
            UserEvent::Accessibility(event) => {
                if let Some(key) = self.window_key(event.window_id) {
                    self.windows
                        .get_mut(&key)
                        .unwrap()
                        .accessibility(event_loop, event);
                }
            }
        }
        self.finish(event_loop);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(key) = self.window_key(id) else {
            return;
        };
        let redraw = matches!(event, WindowEvent::RedrawRequested);
        // An owner with an open native modal ignores input and yields focus.
        if let Some(dialog) = self
            .modals
            .iter()
            .find(|(_, owner)| **owner == key)
            .map(|(dialog, _)| *dialog)
        {
            match &event {
                WindowEvent::Focused(true) => {
                    if let Some(state) =
                        self.windows.get(&dialog).and_then(|app| app.state.as_ref())
                    {
                        state.window.focus_window();
                    }
                }
                WindowEvent::CursorMoved { .. }
                | WindowEvent::CursorLeft { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::Touch(_)
                | WindowEvent::PinchGesture { .. }
                | WindowEvent::RotationGesture { .. }
                | WindowEvent::KeyboardInput { .. }
                | WindowEvent::ModifiersChanged(_)
                | WindowEvent::Ime(_)
                | WindowEvent::DroppedFile(_) => return,
                _ => {}
            }
        }
        if redraw && self.inspectors.contains_key(&key) {
            self.refresh_inspector(key);
        }
        let (profile_gpu, inspected) = (
            !self.inspectors.is_empty(),
            self.inspector_of(key).is_some(),
        );
        let app = self.windows.get_mut(&key).unwrap();
        (app.profile_gpu, app.inspected) = (profile_gpu, inspected);
        app.window_event(event_loop, event);
        if redraw {
            if self.inspectors.contains_key(&key) {
                self.apply_inspector(key);
            } else if let Some(inspector) = self.inspector_of(key)
                && let Some(app) = self.windows.get(&inspector)
            {
                app.request_redraw();
            }
        }
        self.finish(event_loop);
    }
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        for app in self.windows.values_mut() {
            app.new_events(cause);
        }
        if self.windows.values().any(|app| app.close_requested) {
            // A development restart asked the window to stop.
            self.finish(event_loop);
        } else if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.schedule(event_loop);
        }
    }
    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.wake.close();
    }
}
