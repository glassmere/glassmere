use super::*;
// A window of an application, as `Effects::open_window` names it.
use seaglass_core::WindowId as AppWindow;
use seaglass_core::{AppDriver, Effects, Pacing, TextMeasurer};
use seaglass_devtools::{Inspector, InspectorModel, InspectorMsg, InspectorResponse};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) type View = Box<dyn Declare>;
/// What `update` asked for and the host has not carried out yet.
type AppRequests = (
    Vec<(AppWindow, seaglass_core::WindowRequest)>,
    Vec<AppWindow>,
);
/// The window `run` opens first. Its view is `App::view`.
const MAIN: AppWindow = AppWindow::MAIN;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct WindowKey(pub u64);

/// The application's first window. Its setup also returns the host of the
/// windows `update` opens.
pub(crate) fn run_single(
    options: AppOptions,
    setup: impl FnOnce(WakeHandle) -> (View, Option<Box<dyn Hosting>>),
) -> Result<()> {
    diagnostics::checkpoint("process", None);
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let wake = WakeHandle::new(proxy.clone());
    let (view, hosting) = setup(wake.clone());
    diagnostics::checkpoint("event_loop", None);
    let graphics = options.graphics.clone();
    let mut desktop = Desktop::new(proxy, wake, Arc::new(AtomicU64::new(2)), graphics);
    if hosting.is_some() {
        desktop.app_windows.insert(MAIN, WindowKey(1));
    }
    desktop.hosting = hosting;
    desktop.pending.push(Pending {
        key: WindowKey(1),
        options,
        view,
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

/// The options of a window `update` opened: the main window's, with the
/// title and size it asked for and the system's placement.
fn window_options(main: &AppOptions, request: seaglass_core::WindowRequest) -> AppOptions {
    AppOptions {
        title: request.title,
        size: request.size.unwrap_or(AppOptions::default().size),
        placement: Placement::System,
        ..main.clone()
    }
}

struct Pending {
    key: WindowKey,
    options: AppOptions,
    view: View,
    owner: Option<WindowKey>,
}

/// What the desktop asks of an application run with `run`, whatever its
/// state is. The windows it opens and closes come from `update`.
pub(crate) trait Hosting {
    /// The view of a window `update` opened.
    fn open_view(&self, window: AppWindow) -> View;
    /// What `update` asked to open and to close since the last call.
    fn take_requests(&mut self) -> AppRequests;
    /// The windows whose view a batch made due, which need a frame.
    fn due(&self) -> Vec<AppWindow>;
    /// A window is gone; its pacing goes with it. `by_app` when `update`
    /// closed it, otherwise the app is told and its answer is a request.
    fn closed(&mut self, window: AppWindow, by_app: bool);
}

/// The state every window of an application with `run` declares from:
/// the `App`, its driver with the one message queue, and each window's pacing.
struct Shared<A: App> {
    app: A,
    driver: AppDriver<A::Message>,
    windows: HashMap<AppWindow, Pacing>,
    opened: Vec<(AppWindow, seaglass_core::WindowRequest)>,
    closed: Vec<AppWindow>,
}

impl<A: App> Shared<A> {
    /// One window's frame. Its batch runs on its own context; when the batch
    /// changed what views read, every window's view is due.
    fn frame(
        &mut self,
        window: AppWindow,
        context: &mut Context,
        size: Size,
        text: &mut impl TextMeasurer,
    ) -> FrameStats {
        let (messages, dirty) = self.driver.batch_in(window, &mut self.app, context);
        let (opened, closed) = self.driver.take_windows();
        self.opened.extend(opened);
        self.closed.extend(closed);
        if dirty {
            for pacing in self.windows.values_mut() {
                pacing.invalidate();
            }
        }
        let mut pacing = self.windows.remove(&window).unwrap_or_default();
        let stats = self.driver.declare(
            &self.app,
            context,
            size,
            text,
            (window != MAIN).then_some(window),
            &mut pacing,
            messages,
        );
        self.windows.insert(window, pacing);
        stats
    }
}

/// The host of an application run with `run`, shared with its windows.
struct AppHost<A: App> {
    shared: Rc<RefCell<Shared<A>>>,
}

impl<A: App> Hosting for AppHost<A> {
    fn open_view(&self, window: AppWindow) -> View {
        self.shared.borrow_mut().windows.entry(window).or_default();
        Box::new(WindowView {
            shared: self.shared.clone(),
            window,
        })
    }

    fn take_requests(&mut self) -> AppRequests {
        let mut shared = self.shared.borrow_mut();
        (
            std::mem::take(&mut shared.opened),
            std::mem::take(&mut shared.closed),
        )
    }

    fn due(&self) -> Vec<AppWindow> {
        let shared = self.shared.borrow();
        let mut due: Vec<_> = shared
            .windows
            .iter()
            .filter(|(_, pacing)| pacing.invalidated())
            .map(|(window, _)| *window)
            .collect();
        // Something aimed at a window other than the one that ran `update`
        // is applied by that window's own batch.
        for window in shared.driver.awaiting() {
            if !due.contains(&window) {
                due.push(window);
            }
        }
        due
    }

    fn closed(&mut self, window: AppWindow, by_app: bool) {
        let mut shared = self.shared.borrow_mut();
        let shared = &mut *shared;
        shared.windows.remove(&window);
        if by_app || window == MAIN {
            return;
        }
        shared.driver.window_closed(&mut shared.app, window);
        let (opened, closed) = shared.driver.take_windows();
        shared.opened.extend(opened);
        shared.closed.extend(closed);
        for pacing in shared.windows.values_mut() {
            pacing.invalidate();
        }
    }
}

/// One window of an application run with `run`. It declares through the
/// shared state, with `App::view` for the main window and `App::view_window`
/// for the others.
struct WindowView<A: App> {
    shared: Rc<RefCell<Shared<A>>>,
    window: AppWindow,
}

impl<A: App> Declare for WindowView<A> {
    fn frame(&mut self, context: &mut Context, size: Size, text: &mut TextSystem) -> FrameStats {
        self.shared
            .borrow_mut()
            .frame(self.window, context, size, text)
    }

    fn hot_state(&mut self, _: &mut Context, _: Size, _: &mut TextSystem) -> String {
        self.shared.borrow().app.save_state().unwrap_or_default()
    }

    fn invalidate(&mut self) {
        self.shared
            .borrow_mut()
            .windows
            .entry(self.window)
            .or_default()
            .invalidate();
    }

    /// The application's state is one, so only the main window saves it.
    fn saves_state(&self) -> bool {
        self.window == MAIN
    }
}

/// Runs an application: its first window declares with `App::view`, and the
/// windows `Effects::open_window` opens declare with `App::view_window`.
pub(crate) fn run_app<A: App>(options: AppOptions, app: A) -> Result<()> {
    run_single(options, |wake| {
        let shared = Rc::new(RefCell::new(Shared {
            app,
            driver: AppDriver::new(move || wake.request_redraw()),
            windows: HashMap::new(),
            opened: Vec::new(),
            closed: Vec::new(),
        }));
        let view: View = Box::new(WindowView {
            shared: shared.clone(),
            window: MAIN,
        });
        let host: Box<dyn Hosting> = Box::new(AppHost { shared });
        (view, Some(host))
    })
}

struct InspectorShared {
    inspector: Inspector,
    model: InspectorModel,
    response: InspectorResponse,
}

/// The inspector's window as an application: its messages are
/// `InspectorMsg`, and what it asks of the inspected window collects in the
/// shared response.
struct InspectorApp {
    shared: Rc<RefCell<InspectorShared>>,
}

impl App for InspectorApp {
    type Message = InspectorMsg;

    fn update(&mut self, message: InspectorMsg, _: &mut Effects<'_, InspectorMsg>) {
        let shared = &mut *self.shared.borrow_mut();
        let response = shared.inspector.update(message, &shared.model);
        shared.response.export_csv |= response.export_csv;
        shared.response.focus = response.focus.or(shared.response.focus);
    }

    fn view(&self, ui: &mut Ui<'_, InspectorMsg>) {
        let shared = self.shared.borrow();
        shared.inspector.view(ui, &shared.model);
    }
}

/// The inspector window's declaration. The model is replaced before every
/// frame, so its view is always due.
struct InspectorView {
    app: InspectorApp,
    driver: AppDriver<InspectorMsg>,
    pacing: Pacing,
}

impl Declare for InspectorView {
    fn frame(&mut self, context: &mut Context, size: Size, text: &mut TextSystem) -> FrameStats {
        let (messages, _) = self.driver.batch(&mut self.app, context);
        self.pacing.invalidate();
        self.driver.declare(
            &self.app,
            context,
            size,
            text,
            None,
            &mut self.pacing,
            messages,
        )
    }

    fn hot_state(&mut self, _: &mut Context, _: Size, _: &mut TextSystem) -> String {
        String::new()
    }
}

struct InspectorLink {
    target: WindowKey,
    shared: Rc<RefCell<InspectorShared>>,
}

/// A drag that has left the window it started in. The pointer stays
/// captured by that window while a button is held, so its events are what
/// carry the payload over the others.
struct Crossing {
    source: WindowKey,
    /// The window holding the payload now, if the pointer is over one.
    over: Option<WindowKey>,
    payload: seaglass_core::DragPayload,
}

/// What a window event says about the pointer, which is all the carrying
/// needs to know.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PointerNews {
    Moved,
    Released,
    Nothing,
}

struct Desktop {
    windows: HashMap<WindowKey, Application>,
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
    /// A drag being carried from the window it started in to another of this
    /// application's windows.
    crossing: Option<Crossing>,
    /// The window that last took the keyboard, for menus and tray icons,
    /// which belong to the application rather than to a window.
    focused: Option<WindowKey>,
    clipboard: Rc<RefCell<input::HostClipboard>>,
    error: Option<Box<dyn Error>>,
    /// The application's windows, when `run` runs one.
    hosting: Option<Box<dyn Hosting>>,
    /// Application window → the window it is shown in.
    app_windows: HashMap<AppWindow, WindowKey>,
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
    /// Carries a drag that has left its window over the application's other
    /// windows, and finishes it when the button comes up. Called after the
    /// window's own handling, so the source has already seen the event.
    fn carry_drag(&mut self, key: WindowKey, news: PointerNews) {
        let escaped = self
            .windows
            .get_mut(&key)
            .is_some_and(|app| std::mem::take(&mut app.drag_escaped));
        let Some(at) = self.windows.get(&key).and_then(Application::screen_pointer) else {
            return;
        };
        if self.crossing.is_none() {
            // Nothing starts unless another window is actually under it.
            if !escaped || self.window_at(at, Some(key)).is_none() {
                return;
            }
            let Some(payload) = self
                .windows
                .get_mut(&key)
                .and_then(|app| app.context.export_drag())
            else {
                return;
            };
            self.crossing = Some(Crossing {
                source: key,
                over: None,
                payload,
            });
        }
        let Some(mut crossing) = self.crossing.take() else {
            return;
        };
        match news {
            PointerNews::Moved => {
                let target = self.window_at(at, None);
                if target != crossing.over {
                    if let Some(old) = crossing.over
                        && let Some(app) = self.windows.get_mut(&old)
                    {
                        app.context.withdraw_drag();
                        app.request_redraw();
                    }
                    if let Some(new) = target
                        && let Some(app) = self.windows.get_mut(&new)
                        && let Some(local) = app.local_pointer(at)
                    {
                        app.context.adopt_drag(crossing.payload.clone(), local);
                        app.request_redraw();
                    }
                    crossing.over = target;
                } else if let Some(over) = target.filter(|over| *over != key)
                    && let Some(app) = self.windows.get_mut(&over)
                    && let Some(local) = app.local_pointer(at)
                {
                    // A window the pointer is merely over has no events of
                    // its own while another holds the pointer, so its moves
                    // come from here. The source window moved itself.
                    app.input(Event::PointerMoved(local));
                }
                self.crossing = Some(crossing);
            }
            PointerNews::Released => {
                let mut dropped = false;
                if let Some(over) = crossing.over
                    && let Some(app) = self.windows.get_mut(&over)
                    && let Some(local) = app.local_pointer(at)
                {
                    dropped = app.context.accepts_drop(local, &crossing.payload);
                    app.input(Event::PointerUp);
                }
                if let Some(source) = self.windows.get_mut(&crossing.source) {
                    source.context.finish_exported_drag(if dropped {
                        seaglass_core::DragOutcome::Dropped
                    } else {
                        seaglass_core::DragOutcome::Cancelled
                    });
                    source.request_redraw();
                }
            }
            PointerNews::Nothing => self.crossing = Some(crossing),
        }
    }

    /// Which window that point on the desktop is inside. Overlapping windows
    /// are not ordered here, so the first match wins.
    fn window_at(&self, at: (f64, f64), except: Option<WindowKey>) -> Option<WindowKey> {
        self.windows
            .iter()
            .filter(|(key, _)| Some(**key) != except)
            .find(|(_, app)| app.covers(at))
            .map(|(key, _)| *key)
    }

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
            crossing: None,
            focused: None,
            clipboard: Rc::new(RefCell::new(input::HostClipboard::default())),
            error: None,
            hosting: None,
            app_windows: HashMap::new(),
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
            .with_decorations(options.decorations)
            .with_resizable(options.resizable)
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
                // SEAGLASS_INSPECT opens the inspector immediately, for smoke
                // runs and for tools without keyboard focus.
                app.inspector_requested =
                    app.options.devtools && std::env::var_os("SEAGLASS_INSPECT").is_some();
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

    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        self.apply_app_requests(event_loop);
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
        self.sync_app_windows();
        // The app's answer to a window it was told is gone.
        self.apply_app_requests(event_loop);
        self.sync_inspectors(event_loop);
        // An application stops when its first window closes.
        let main_closed = self.hosting.is_some() && !self.app_windows.contains_key(&MAIN);
        if self.opened && (self.windows.is_empty() || main_closed) {
            event_loop.exit();
        }
        self.wake_due_windows();
        self.schedule(event_loop);
    }

    /// Carries out what `update` asked for: the windows it closes, then the
    /// windows it opens.
    fn apply_app_requests(&mut self, event_loop: &ActiveEventLoop) {
        let Some(hosting) = self.hosting.as_mut() else {
            return;
        };
        let (opened, closed) = hosting.take_requests();
        for window in closed {
            if let Some(key) = self.app_windows.remove(&window) {
                self.windows.remove(&key);
                if let Some(hosting) = self.hosting.as_mut() {
                    hosting.closed(window, true);
                }
            }
        }
        for (window, request) in opened {
            let key = self.allocate_key();
            let owner = request
                .modal_owner
                .and_then(|owner| self.app_windows.get(&owner).copied());
            let main = self
                .app_windows
                .get(&MAIN)
                .and_then(|key| self.windows.get(key))
                .map(|app| &app.options);
            let options = match main {
                Some(main) => window_options(main, request),
                None => AppOptions {
                    title: request.title,
                    size: request.size.unwrap_or(AppOptions::default().size),
                    ..AppOptions::default()
                },
            };
            let Some(view) = self
                .hosting
                .as_ref()
                .map(|hosting| hosting.open_view(window))
            else {
                return;
            };
            if self.open(event_loop, key, options, view, owner) {
                self.app_windows.insert(window, key);
            }
        }
    }

    /// Tells the application about the windows that are gone, however they
    /// went: closed by the user, by `update`, or with their owner.
    fn sync_app_windows(&mut self) {
        let gone: Vec<AppWindow> = self
            .app_windows
            .iter()
            .filter(|(_, key)| !self.windows.contains_key(key))
            .map(|(window, _)| *window)
            .collect();
        for window in gone {
            self.app_windows.remove(&window);
            if let Some(hosting) = self.hosting.as_mut() {
                hosting.closed(window, false);
            }
        }
    }

    /// Asks each window whose view a batch made due for a frame of its own.
    fn wake_due_windows(&self) {
        let Some(hosting) = &self.hosting else {
            return;
        };
        for window in hosting.due() {
            if let Some(app) = self
                .app_windows
                .get(&window)
                .and_then(|key| self.windows.get(key))
            {
                app.request_redraw();
            }
        }
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
        let view: View = Box::new(InspectorView {
            app: InspectorApp {
                shared: shared.clone(),
            },
            driver: AppDriver::new(|| true),
            pacing: Pacing::default(),
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
        let shared = &mut *shared;
        shared.inspector.sync(&shared.model);
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
            && target.context.action(id, seaglass_core::Action::Focus)
        {
            target.request_redraw();
        }
        if response.export_csv {
            let path = std::env::current_dir()
                .unwrap_or_default()
                .join(format!("seaglass-frames-{}.csv", target_key.0));
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
        if matches!(event, WindowEvent::Focused(true)) {
            self.focused = Some(key);
        }
        let news = match &event {
            WindowEvent::CursorMoved { .. } => PointerNews::Moved,
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Released,
                button: winit::event::MouseButton::Left,
                ..
            } => PointerNews::Released,
            _ => PointerNews::Nothing,
        };
        app.window_event(event_loop, event);
        self.carry_drag(key, news);
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
    /// Between frames: what the declarations asked the desktop for, and
    /// what the desktop's own menus have to say back.
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        for app in self.windows.values_mut() {
            app.platform_requests();
        }
        for id in crate::platform::chosen_commands() {
            // A menu belongs to the application, so the command goes to the
            // window that has the keyboard, or to the first one.
            let target = self
                .focused
                .filter(|key| self.windows.contains_key(key))
                .or_else(|| self.windows.keys().copied().next());
            if let Some(app) = target.and_then(|key| self.windows.get_mut(&key)) {
                app.input(Event::Command(id));
                app.request_redraw();
            }
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.wake.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seaglass_core::MonospaceMeasurer;

    const SIZE: Size = Size::new(400.0, 300.0);

    #[derive(Clone)]
    enum Msg {
        Open,
        Bump,
        Close,
    }

    /// What each window's view saw: the window (`None` for the main one) and the count.
    type Seen = Rc<RefCell<Vec<(Option<AppWindow>, i32)>>>;

    #[derive(Default)]
    struct Probe {
        count: i32,
        second: Option<AppWindow>,
        seen: Seen,
        /// The windows the app was told were closed from outside.
        gone: Vec<AppWindow>,
    }

    impl App for Probe {
        type Message = Msg;

        fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
            match message {
                Msg::Open => {
                    self.second = Some(effects.open_window(seaglass_core::WindowRequest {
                        title: "Second".into(),
                        ..Default::default()
                    }));
                }
                Msg::Bump => self.count += 1,
                Msg::Close => {
                    if let Some(window) = self.second {
                        effects.close_window(window);
                    }
                }
            }
        }

        fn window_closed(&mut self, window: AppWindow, effects: &mut Effects<'_, Msg>) {
            self.gone.push(window);
            self.second = Some(effects.open_window(Default::default()));
        }

        fn view(&self, ui: &mut Ui<'_, Msg>) {
            self.seen.borrow_mut().push((None, self.count));
            ui.label(format!("count {}", self.count));
        }

        fn view_window(&self, window: AppWindow, ui: &mut Ui<'_, Msg>) {
            self.seen.borrow_mut().push((Some(window), self.count));
            ui.label(format!("count {}", self.count));
        }

        fn save_state(&self) -> Option<String> {
            Some(format!("count {}", self.count))
        }
    }

    fn shared(app: Probe) -> Rc<RefCell<Shared<Probe>>> {
        Rc::new(RefCell::new(Shared {
            app,
            driver: AppDriver::new(|| true),
            windows: HashMap::new(),
            opened: Vec::new(),
            closed: Vec::new(),
        }))
    }

    #[test]
    fn an_opened_window_inherits_the_main_windows_options_but_its_own_title_and_size() {
        let main = AppOptions {
            title: "Main".into(),
            size: Size::new(1200.0, 800.0),
            placement: Placement::Centered,
            theme: ThemeChoice::Fixed(Theme::dark()),
            stylesheet: Some("app.css".into()),
            decorations: false,
            devtools: false,
            reduced_motion: true,
            ..AppOptions::default()
        };
        let request = seaglass_core::WindowRequest {
            title: "Second".into(),
            size: None,
            modal_owner: None,
        };
        let options = window_options(&main, request);
        assert_eq!(options.title, "Second");
        assert_eq!(options.size, AppOptions::default().size);
        assert_eq!(options.placement, Placement::System);
        assert_eq!(options.theme, main.theme);
        assert_eq!(options.stylesheet, main.stylesheet);
        assert!(!options.decorations && !options.devtools && options.reduced_motion);
    }

    #[test]
    fn open_and_close_requests_reach_the_host() {
        let shared = shared(Probe::default());
        let mut host = AppHost {
            shared: shared.clone(),
        };
        let mut main = Context::new();
        shared.borrow_mut().driver.send(Msg::Open);
        shared
            .borrow_mut()
            .frame(MAIN, &mut main, SIZE, &mut MonospaceMeasurer);
        let (opened, closed) = host.take_requests();
        assert!(closed.is_empty());
        assert_eq!(opened.len(), 1);
        assert_eq!(
            (opened[0].0, opened[0].1.title.as_str()),
            (AppWindow(1), "Second")
        );

        shared.borrow_mut().driver.send(Msg::Close);
        shared
            .borrow_mut()
            .frame(MAIN, &mut main, SIZE, &mut MonospaceMeasurer);
        assert_eq!(host.take_requests().1, vec![AppWindow(1)]);
        assert!(host.take_requests().0.is_empty());
    }

    #[test]
    fn a_window_closed_from_outside_is_reported_and_its_answer_reaches_the_host() {
        let shared = shared(Probe::default());
        let mut host = AppHost {
            shared: shared.clone(),
        };
        let mut main = Context::new();
        shared
            .borrow_mut()
            .frame(MAIN, &mut main, SIZE, &mut MonospaceMeasurer);
        let second = AppWindow(1);
        host.open_view(second);

        // The app closed it itself: nothing to report.
        host.closed(second, true);
        assert!(shared.borrow().app.gone.is_empty());
        assert!(host.take_requests().0.is_empty());

        // The user closed it: the app answers, and every view is due.
        host.open_view(second);
        host.closed(second, false);
        assert_eq!(shared.borrow().app.gone, [second]);
        assert_eq!(host.take_requests().0.len(), 1);
        assert_eq!(host.due(), vec![MAIN]);
    }

    #[test]
    fn a_message_from_a_second_window_is_seen_by_the_first_windows_next_view() {
        let shared = shared(Probe::default());
        let seen = shared.borrow().app.seen.clone();
        let second = AppWindow(1);
        let (mut main, mut other) = (Context::new(), Context::new());
        shared
            .borrow_mut()
            .frame(MAIN, &mut main, SIZE, &mut MonospaceMeasurer);
        shared
            .borrow_mut()
            .frame(second, &mut other, SIZE, &mut MonospaceMeasurer);
        assert_eq!(*seen.borrow(), vec![(None, 0), (Some(second), 0)]);

        // The message arrives in the second window's batch and changes the state.
        shared.borrow_mut().driver.send(Msg::Bump);
        let stats = shared
            .borrow_mut()
            .frame(second, &mut other, SIZE, &mut MonospaceMeasurer);
        assert_eq!((stats.declared, stats.messages), (true, 1));
        assert!(shared.borrow().windows[&MAIN].invalidated());

        // The first window's frame declares, and its view sees the count.
        let stats = shared
            .borrow_mut()
            .frame(MAIN, &mut main, SIZE, &mut MonospaceMeasurer);
        assert_eq!((stats.declared, stats.messages), (true, 0));
        assert_eq!(seen.borrow().last(), Some(&(None, 1)));
        assert!(!shared.borrow().windows[&MAIN].invalidated());
    }

    #[test]
    fn a_window_without_a_batch_declares_only_when_due() {
        let shared = shared(Probe::default());
        let mut main = Context::new();
        shared
            .borrow_mut()
            .frame(MAIN, &mut main, SIZE, &mut MonospaceMeasurer);
        let stats = shared
            .borrow_mut()
            .frame(MAIN, &mut main, SIZE, &mut MonospaceMeasurer);
        assert!(!stats.declared, "nothing changed, so no declaration");
    }

    #[test]
    fn only_the_main_window_saves_the_application_state() {
        let shared = shared(Probe {
            count: 4,
            ..Probe::default()
        });
        assert!(
            WindowView {
                shared: shared.clone(),
                window: MAIN,
            }
            .saves_state()
        );
        assert!(
            !WindowView {
                shared: shared.clone(),
                window: AppWindow(1),
            }
            .saves_state()
        );
        assert_eq!(shared.borrow().app.save_state().as_deref(), Some("count 4"));
    }
}
