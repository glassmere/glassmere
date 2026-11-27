//! Event-driven desktop host with logical coordinates and a wgpu surface.
mod accessibility;
mod desktop;
mod diagnostics;
mod graphics;
pub mod hot;
mod images;
mod input;
mod os_drag;
mod platform;
mod system_motion;
mod wakeup;
use glassmere_core::{
    App, Context, DragPayload, Effects, Event, FrameStats, TextEdit, Theme, Ui, app::Action,
};
pub use glassmere_core::{Mailbox, MessageSender};
use glassmere_devtools::Recorder;
use glassmere_render_cpu::CpuRenderer;
use glassmere_render_wgpu::Renderer;
use glassmere_render_wgpu::TextSystem;
use glassmere_scene::{Color, Point, Primitive, Scene, Size};
pub use graphics::{GpuMemoryPolicy, GraphicsBackend, GraphicsOptions};
use std::{
    cell::RefCell,
    error::Error,
    ffi::OsStr,
    num::NonZeroU32,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use wakeup::UserEvent;
use wakeup::WakeHandle;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::ModifiersState,
    window::{CursorIcon, Window, WindowId},
};

/// What an application draws itself, registered once the window's renderer
/// exists. Inside it, `Renderer::set_custom_draw` takes a callback for an id,
/// and a `Primitive::Custom` with that id in a `CustomWidget` is where it
/// draws. Nothing runs on a window painted without a graphics adapter.
///
/// The closure is `Send` because the options travel to the thread a window
/// opens on; it runs there, on the renderer's own thread.
///
/// ```ignore
/// AppOptions {
///     gpu: Some(GpuSetup::new(|renderer| {
///         renderer.set_custom_draw(7, Some(Box::new(|draw| { /* wgpu here */ })));
///     })),
///     ..AppOptions::default()
/// }
/// ```
#[derive(Clone)]
pub struct GpuSetup(SharedSetup);

type SharedSetup = std::sync::Arc<std::sync::Mutex<dyn FnMut(&mut Renderer) + Send>>;

impl GpuSetup {
    pub fn new(setup: impl FnMut(&mut Renderer) + Send + 'static) -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(setup)))
    }
}

impl std::fmt::Debug for GpuSetup {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("GpuSetup")
    }
}

/// Which screen a window opens on, and where.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    /// Wherever the system puts it, which is usually sensible.
    #[default]
    System,
    /// In the middle of the screen the pointer is on, or of the primary one.
    Centered,
    /// In the middle of the screen its owner is on. A window without an owner
    /// is centred on the pointer's screen instead, so a dialog stays with the
    /// window it belongs to.
    WithOwner,
    /// In the middle of the screen whose name contains this text, ignoring
    /// case, or of the primary one when nothing matches. The names are the
    /// platform's own, as the display settings show them.
    Named(String),
}
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
    pub fn resolve(self) -> glassmere_core::Direction {
        match self {
            Self::Ltr => glassmere_core::Direction::Ltr,
            Self::Rtl => glassmere_core::Direction::Rtl,
            Self::Auto => sys_locale::get_locale()
                .map_or(glassmere_core::Direction::Ltr, |locale| {
                    locale_direction(&locale)
                }),
        }
    }
}

/// Direction of a BCP 47 locale such as `he-IL` or `ar`, by its language.
pub fn locale_direction(locale: &str) -> glassmere_core::Direction {
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
        glassmere_core::Direction::Rtl
    } else {
        glassmere_core::Direction::Ltr
    }
}

#[cfg(test)]
mod direction_tests {
    use super::{DirectionChoice, locale_direction};
    use glassmere_core::Direction;

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

/// Whether windows follow the system's reduced-motion preference, see
/// [`follow_system_reduced_motion`].
static FOLLOW_SYSTEM_MOTION: AtomicBool = AtomicBool::new(true);

/// The environment variable that, set to exactly this, keeps the system's
/// reduced-motion preference from being asked or applied.
const SYSTEM_MOTION_VARIABLE: &str = "GLASSMERE_SYSTEM_MOTION";

/// Sets whether a window follows the system's reduced-motion preference.
///
/// By default it does, where the framework can read it: on Windows, and on
/// Linux and the BSDs where `gsettings` reports it. On Linux and the BSDs
/// reading it starts the `gsettings` tool, on a background thread. An
/// application that must not start processes calls this with `false` before
/// [`run`], and then `AppOptions::reduced_motion` alone decides. Setting
/// `GLASSMERE_SYSTEM_MOTION=off` in the environment has the same effect.
pub fn follow_system_reduced_motion(follow: bool) {
    FOLLOW_SYSTEM_MOTION.store(follow, Ordering::Relaxed);
}

/// Whether `value`, the content of `GLASSMERE_SYSTEM_MOTION`, switches the
/// system's preference off. Only `off` does.
fn system_motion_switched_off(value: Option<&OsStr>) -> bool {
    value == Some(OsStr::new("off"))
}

/// Whether the system's preference is followed: `following` is the setting of
/// [`follow_system_reduced_motion`] and `variable` the environment variable.
fn system_motion_followed(following: bool, variable: Option<&OsStr>) -> bool {
    following && !system_motion_switched_off(variable)
}

/// Whether asking the system can change what a window does: not when the
/// application already asks for reduced motion, and not when the system is
/// not followed.
fn probe_wanted(app: bool, following: bool) -> bool {
    !app && following
}

/// Asks the system in the background for a window whose option is `app`,
/// unless that cannot matter.
fn probe_system_motion(wake: &WakeHandle, app: bool) {
    if probe_wanted(app, FOLLOW_SYSTEM_MOTION.load(Ordering::Relaxed)) {
        system_motion::refresh(wake);
    }
}

/// The system's answer when it is followed, and no answer when it is not.
/// `read` is not called then, so that nothing is asked of the system.
fn system_answer(
    followed: bool,
    read: impl FnOnce() -> (usize, Option<bool>),
) -> (usize, Option<bool>) {
    if followed { read() } else { (0, None) }
}

/// Whether animations are skipped: the application asked for it, or the
/// system did. An application cannot turn motion on against the system.
fn resolve_reduced_motion(app: bool, system: Option<bool>) -> bool {
    app || system.unwrap_or(false)
}

/// Takes the system's current preference into `context`, with the
/// application's own option on top, and notes in `seen` which answer that was,
/// so that a later change of the answer is noticed. It takes the fields and
/// not `self` because its callers hold a borrow of the window state, as
/// `check_families` does.
fn apply_system_motion(context: &mut Context, seen: &mut usize, app: bool) {
    let followed = system_motion_followed(
        FOLLOW_SYSTEM_MOTION.load(Ordering::Relaxed),
        std::env::var_os(SYSTEM_MOTION_VARIABLE).as_deref(),
    );
    let answer = system_answer(followed, system_motion::snapshot);
    take_system_motion(context, seen, app, answer);
}

/// `apply_system_motion` for the answer `snapshot` pairs with its generation.
fn take_system_motion(
    context: &mut Context,
    seen: &mut usize,
    app: bool,
    (generation, system): (usize, Option<bool>),
) {
    *seen = generation;
    context.set_reduced_motion(resolve_reduced_motion(app, system));
}

/// Takes the system's answer again when it is not the one `seen` stands for.
fn follow_system_motion(context: &mut Context, seen: &mut usize, app: bool) {
    if system_motion::generation() != *seen {
        apply_system_motion(context, seen, app);
    }
}

#[cfg(test)]
mod reduced_motion_tests {
    use super::{
        FOLLOW_SYSTEM_MOTION, apply_system_motion, follow_system_motion,
        follow_system_reduced_motion, probe_wanted, resolve_reduced_motion, system_answer,
        system_motion, system_motion_followed, system_motion_switched_off, take_system_motion,
    };
    use glassmere_core::{Context, MonospaceMeasurer, Spinner};
    use glassmere_scene::Size;
    use std::{ffi::OsStr, sync::atomic::Ordering};

    #[test]
    fn the_application_forces_it_and_the_system_adds_to_it() {
        assert!(!resolve_reduced_motion(false, None));
        assert!(!resolve_reduced_motion(false, Some(false)));
        assert!(resolve_reduced_motion(false, Some(true)));
        assert!(resolve_reduced_motion(true, None));
        assert!(resolve_reduced_motion(true, Some(false)));
        assert!(resolve_reduced_motion(true, Some(true)));
    }

    #[test]
    fn the_system_is_asked_only_where_it_can_matter() {
        for app in [false, true] {
            for following in [false, true] {
                assert_eq!(probe_wanted(app, following), !app && following);
            }
        }
    }

    #[test]
    fn only_off_switches_the_system_off() {
        assert!(system_motion_switched_off(Some(OsStr::new("off"))));
        for value in [
            None,
            Some(""),
            Some("on"),
            Some("OFF"),
            Some("off "),
            Some("1"),
        ] {
            assert!(
                !system_motion_switched_off(value.map(OsStr::new)),
                "{value:?}"
            );
        }
        for following in [false, true] {
            for variable in [None, Some("off"), Some("on")] {
                let followed = system_motion_followed(following, variable.map(OsStr::new));
                assert_eq!(followed, following && variable != Some("off"));
            }
        }
    }

    #[test]
    fn without_following_a_system_answer_changes_nothing() {
        for app in [false, true] {
            for following in [false, true] {
                for answer in [None, Some(false), Some(true)] {
                    let mut asked = false;
                    let (generation, system) = system_answer(following, || {
                        asked = true;
                        (4, answer)
                    });
                    assert_eq!(asked, following);
                    assert_eq!(generation, if following { 4 } else { 0 });
                    assert_eq!(
                        resolve_reduced_motion(app, system),
                        app || (following && answer == Some(true)),
                        "{app} {following} {answer:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_setting_starts_on_and_can_be_switched() {
        // The only test that touches the setting: tests share the process.
        assert!(FOLLOW_SYSTEM_MOTION.load(Ordering::Relaxed));
        follow_system_reduced_motion(false);
        assert!(!FOLLOW_SYSTEM_MOTION.load(Ordering::Relaxed));
        follow_system_reduced_motion(true);
        assert!(FOLLOW_SYSTEM_MOTION.load(Ordering::Relaxed));
    }

    /// Whether `context` skips animations, as a spinner shows: it asks for
    /// another frame only while animations are on.
    fn skips_animations(context: &mut Context) -> bool {
        context.run(Size::new(100.0, 100.0), &mut MonospaceMeasurer, |ui| {
            ui.spinner(Spinner::new("Busy"));
        });
        !context.animating()
    }

    #[test]
    fn a_context_takes_the_answer_and_the_option_and_notes_which_answer() {
        let answers = [None, Some(false), Some(true)];
        for app in [false, true] {
            for system in answers {
                let mut context = Context::new();
                let mut seen = 99;
                take_system_motion(&mut context, &mut seen, app, (7, system));
                assert_eq!(seen, 7);
                let expected = app || system == Some(true);
                assert_eq!(skips_animations(&mut context), expected, "{app} {system:?}");
            }
        }
        // A later answer replaces an earlier one, in both directions.
        let mut context = Context::new();
        let mut seen = 0;
        take_system_motion(&mut context, &mut seen, false, (1, Some(true)));
        assert!(skips_animations(&mut context));
        take_system_motion(&mut context, &mut seen, false, (2, Some(false)));
        assert_eq!(seen, 2);
        assert!(!skips_animations(&mut context));
    }

    #[test]
    fn a_window_takes_the_answer_the_system_has_now() {
        let (generation, system) = system_motion::snapshot();
        for app in [false, true] {
            let mut context = Context::new();
            let mut seen = generation + 5;
            apply_system_motion(&mut context, &mut seen, app);
            assert_eq!(seen, generation);
            let expected = resolve_reduced_motion(app, system);
            assert_eq!(skips_animations(&mut context), expected, "{app}");
        }
    }

    #[test]
    fn a_frame_takes_the_answer_only_when_it_is_a_newer_one() {
        let (generation, system) = system_motion::snapshot();
        let taken = resolve_reduced_motion(false, system);
        // Set apart from what the answer would make it, to see whether it ran.
        let mut context = Context::new();
        context.set_reduced_motion(!taken);

        let mut seen = generation;
        follow_system_motion(&mut context, &mut seen, false);
        assert_eq!(seen, generation);
        assert_eq!(skips_animations(&mut context), !taken);

        let mut seen = generation + 5;
        follow_system_motion(&mut context, &mut seen, false);
        assert_eq!(seen, generation);
        assert_eq!(skips_animations(&mut context), taken);
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
    /// Registers the application's own GPU drawing when the window opens.
    pub gpu: Option<GpuSetup>,
    /// Which screen the window opens on. Multiple monitors otherwise need no
    /// care: each window follows the scale factor of the screen it is on.
    pub placement: Placement,
    /// F12 opens an inspector window for this window; `GLASSMERE_INSPECT` opens it at start.
    pub devtools: bool,
    pub theme: ThemeChoice,
    /// A stylesheet file installed at start and reloaded when it changes on disk.
    pub stylesheet: Option<PathBuf>,
    /// Skips animations. Animations are also skipped while the OS asks for
    /// reduced motion (read on Windows and, where `gsettings` reports it, on
    /// Linux), so `false` means "follow the system", not "always animate";
    /// `true` forces them off, and then the system is not asked. The system
    /// is not asked either after `follow_system_reduced_motion(false)` or with
    /// `GLASSMERE_SYSTEM_MOTION=off` in the environment.
    pub reduced_motion: bool,
    /// Whether Left and Right move on screen or in reading order in
    /// bidirectional text.
    pub caret_movement: glassmere_core::CaretMovement,
    /// The pointer over a button, a check box, a tab or a link: a hand
    /// (`Cursor::Pointer`) by default. `Cursor::Default` is the arrow a browser
    /// shows there. A node's own `Style::cursor` still wins.
    pub pressable_cursor: glassmere_core::Cursor,
    /// Layout and text direction of the window.
    pub direction: DirectionChoice,
    /// The system's title bar and border. Without them the window has no way
    /// to be moved or resized but the one the application draws.
    pub decorations: bool,
    pub resizable: bool,
    /// The smallest the window can be dragged to, in logical pixels, for a
    /// layout that does not fit in less. `None` leaves it to the system. Windows
    /// the application opens inherit it, as they do the rest of these options.
    pub min_size: Option<Size>,
    /// The largest the window can be dragged to, in logical pixels.
    pub max_size: Option<Size>,
    /// The name the desktop knows the application by: the Wayland `app_id`
    /// and the X11 `WM_CLASS` on Linux, which a `<app_id>.desktop` file is
    /// matched against for the taskbar icon and grouping, and the
    /// application name notifications are posted under. `None` uses the
    /// executable's file name. Read when a window opens.
    pub app_id: Option<String>,
    /// The window's icon, on Windows and X11. A Wayland compositor takes the
    /// icon from the `.desktop` file that `app_id` names instead, and macOS
    /// from the application bundle. A vector image is not used. Read when a
    /// window opens.
    pub icon: Option<glassmere_scene::Image>,
}

impl AppOptions {
    /// `app_id`, or the executable's file name when it is not set.
    pub fn resolved_app_id(&self) -> String {
        self.app_id
            .clone()
            .filter(|id| !id.is_empty())
            .or_else(|| {
                std::env::current_exe()
                    .ok()?
                    .file_stem()?
                    .to_str()
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "glassmere".to_owned())
    }
}

impl Default for AppOptions {
    fn default() -> Self {
        Self {
            title: "Glassmere".into(),
            size: Size::new(900.0, 650.0),
            background: None,
            frame_limit: None,
            graphics: GraphicsOptions::default(),
            gpu: None,
            placement: Placement::default(),
            devtools: true,
            theme: ThemeChoice::System,
            stylesheet: None,
            reduced_motion: false,
            caret_movement: glassmere_core::CaretMovement::Visual,
            pressable_cursor: glassmere_core::Cursor::Pointer,
            direction: DirectionChoice::Auto,
            decorations: true,
            resizable: true,
            min_size: None,
            max_size: None,
            app_id: None,
            icon: None,
        }
    }
}

/// When the host was asked to run, for the startup measurement. Whatever
/// happened before that, including loading the executable, is not in it.
pub(crate) static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
static STARTUP_REPORTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The way winit names an edge, and the cursor that shows it.
fn direction(edge: glassmere_core::ResizeEdge) -> winit::window::ResizeDirection {
    use glassmere_core::ResizeEdge as Edge;
    use winit::window::ResizeDirection as Direction;
    match edge {
        Edge::North => Direction::North,
        Edge::South => Direction::South,
        Edge::East => Direction::East,
        Edge::West => Direction::West,
        Edge::NorthEast => Direction::NorthEast,
        Edge::NorthWest => Direction::NorthWest,
        Edge::SouthEast => Direction::SouthEast,
        Edge::SouthWest => Direction::SouthWest,
    }
}

/// The system's pointer for what `Context::cursor` chose.
fn cursor_icon(cursor: glassmere_core::Cursor) -> CursorIcon {
    use glassmere_core::Cursor;
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Move => CursorIcon::Move,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::Help => CursorIcon::Help,
        Cursor::Wait => CursorIcon::Wait,
        Cursor::ColResize => CursorIcon::ColResize,
        Cursor::RowResize => CursorIcon::RowResize,
    }
}

fn resize_cursor(edge: glassmere_core::ResizeEdge) -> CursorIcon {
    use glassmere_core::ResizeEdge as Edge;
    match edge {
        Edge::North => CursorIcon::NResize,
        Edge::South => CursorIcon::SResize,
        Edge::East => CursorIcon::EResize,
        Edge::West => CursorIcon::WResize,
        Edge::NorthEast => CursorIcon::NeResize,
        Edge::NorthWest => CursorIcon::NwResize,
        Edge::SouthEast => CursorIcon::SeResize,
        Edge::SouthWest => CursorIcon::SwResize,
    }
}

/// Files the operating system started the application with: the arguments
/// that name something on disk. An association opens a file this way, and so
/// does dragging one onto the executable.
fn opened_files() -> Vec<PathBuf> {
    std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .collect()
}

/// Prints how far into startup one step of it was reached, so that a slow
/// start can be blamed on the right thing. Costs an environment lookup.
pub(crate) fn startup_stage(stage: &str) {
    if std::env::var_os("GLASSMERE_STARTUP").is_none()
        || STARTUP_REPORTED.load(std::sync::atomic::Ordering::Acquire)
    {
        return;
    }
    if let Some(started) = STARTED.get() {
        eprintln!(
            "GLASSMERE_STARTUP {stage}_ms={:.1}",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}

/// Prints how long the first window took to show something, when
/// `GLASSMERE_STARTUP` is set. Only the first frame of the first window is
/// reported; later windows and inspectors are not startup.
fn report_startup() {
    if std::env::var_os("GLASSMERE_STARTUP").is_none()
        || STARTUP_REPORTED.swap(true, std::sync::atomic::Ordering::AcqRel)
    {
        return;
    }
    if let Some(started) = STARTED.get() {
        eprintln!(
            "GLASSMERE_STARTUP first_frame_ms={:.1}",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}

/// Runs an [`App`]: each batch of input becomes messages, `update` runs on
/// them, and a window's view declares at most once, only when something it
/// reads may have changed. The first window declares with `App::view`; the
/// windows `Effects::open_window` opens declare with `App::view_window`. The
/// app stops when its first window closes.
pub fn run<A: App>(options: AppOptions, app: A) -> Result<()> {
    let _ = STARTED.set(Instant::now());
    desktop::run_app(options, app)
}

/// Runs a state and a view whose widgets send [`Action`]s, changes to apply
/// to the state: the closure form of [`run`], for small programs. Each
/// handler costs one `Rc` per declaration, so applications of any size use an
/// enum message with `run`.
pub fn run_with<S: 'static>(
    options: AppOptions,
    state: S,
    view: impl Fn(&S, &mut Ui<'_, Action<S>>) + 'static,
) -> Result<()> {
    run(options, Closures { state, view })
}

/// A state and a view, as an [`App`] whose messages are actions on it.
struct Closures<S, V> {
    state: S,
    view: V,
}

impl<S: 'static, V: Fn(&S, &mut Ui<'_, Action<S>>) + 'static> App for Closures<S, V> {
    type Message = Action<S>;

    fn update(&mut self, action: Action<S>, _effects: &mut Effects<'_, Action<S>>) {
        action(&mut self.state);
    }

    fn view(&self, ui: &mut Ui<'_, Action<S>>) {
        (self.view)(&self.state, ui);
    }
}

/// What a window declares: once per frame, as its view decides.
pub(crate) trait Declare {
    /// Declares if the view needs to, then lays out and paints.
    fn frame(&mut self, context: &mut Context, size: Size, text: &mut TextSystem) -> FrameStats;
    /// What the view keeps across a development restart.
    fn hot_state(&mut self, context: &mut Context, size: Size, text: &mut TextSystem) -> String;
    /// The next frame declares: the scale factor changed.
    fn invalidate(&mut self) {}
    /// Whether this window's state is saved for a development restart.
    fn saves_state(&self) -> bool {
        true
    }
}

struct WindowState {
    surfaces: Surfaces,
    renderer: Rc<RefCell<SharedRenderer>>,
    pending_scene: bool,
    accessibility: Option<accesskit_winit::Adapter>,
    window: Arc<Window>,
    ime_target: Option<glassmere_core::NodeId>,
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
    fn text_system(&mut self) -> &mut glassmere_render_wgpu::TextSystem {
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
    damage: &'a [glassmere_scene::Rect],
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
        let renderer = std::env::var("GLASSMERE_RENDERER").ok();
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
                eprintln!("glassmere: {error}. Falling back to the CPU renderer.");
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
        crate::startup_stage("gpu_instance");
        diagnostics::checkpoint("gpu_instance", None);
        let surface = instance.create_surface(window.clone())?;
        crate::startup_stage("surface_created");
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
        crate::startup_stage("adapter_detected");
        diagnostics::checkpoint("adapter_detected", None);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("glassmere device"),
                // Lets the inspector time render passes on the GPU's own
                // clock where the adapter can; nothing is measured until an
                // inspector is open.
                required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
                memory_hints: diagnostics::memory_hints(options.memory_policy.hints()),
                required_limits: diagnostics::device_limits(options.limits()),
                ..Default::default()
            })
            .await?;
        crate::startup_stage("gpu_device");
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
        crate::startup_stage("presentation_buffers");
        diagnostics::checkpoint("presentation_buffers", Some(&device));
        let renderer = Renderer::new(&device, &queue, config.format);
        crate::startup_stage("renderer_and_fonts");
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
            let _render_span = tracing::trace_span!("glassmere.render").entered();
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

struct Application {
    options: AppOptions,
    view: desktop::View,
    context: Context,
    state: Option<WindowState>,
    modifiers: ModifiersState,
    error: Option<Box<dyn Error>>,
    frames: u64,
    accessibility_requested: bool,
    clipboard: Rc<RefCell<input::HostClipboard>>,
    restore_focus: Option<glassmere_core::NodeId>,
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
    schedule: hot::Schedule,
    /// The development runner may ask this process to stop.
    runner: bool,
    /// The development runner asked this process to stop.
    hot_stopping: bool,
    retry_pending: bool,
    /// Framework diagnostics already printed.
    diagnostics_seen: usize,
    /// A drag that was handed to the operating system and has not ended.
    os_drag: Option<std::sync::mpsc::Receiver<glassmere_core::DragOutcome>>,
    /// An in-app drag has left this window and the platform would not carry
    /// it. The desktop decides whether another of its windows is under the
    /// pointer.
    pub(crate) drag_escaped: bool,
    /// Font families already reported as missing, so each is said once.
    families_seen: std::collections::HashSet<String>,
    /// The system's answer this window last took, see `system_motion`.
    motion_seen: usize,
    /// The platform's menu and tray icon, kept alive while they are shown.
    menu: Option<platform::NativeMenu>,
    tray: Option<platform::NativeTray>,
    /// Where Windows is told the application's own maximize button is, so
    /// that it offers its snap layouts over it.
    #[cfg(windows)]
    caption: Option<glassmere_win32::Caption>,
    /// Some inspector is open, so the shared renderer profiles its frames.
    profile_gpu: bool,
    /// An inspector is looking at this window.
    inspected: bool,
    started: Instant,
}

impl Application {
    fn new(
        options: AppOptions,
        view: desktop::View,
        state: WindowState,
        clipboard: Rc<RefCell<input::HostClipboard>>,
        wake: WakeHandle,
    ) -> Self {
        let mut context = Context::new();
        context.set_opened_files(opened_files());
        let mut motion_seen = 0;
        apply_system_motion(&mut context, &mut motion_seen, options.reduced_motion);
        // A window that follows the system does not wait for its first focus
        // when the probe at startup was skipped; the limit on probes keeps
        // this from repeating.
        probe_system_motion(&wake, options.reduced_motion);
        context.set_caret_movement(options.caret_movement);
        context.set_pressable_cursor(options.pressable_cursor);
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
            schedule: hot::Schedule::new(Instant::now()),
            runner: hot::watching(),
            hot_stopping: false,
            retry_pending: false,
            diagnostics_seen: 0,
            os_drag: None,
            drag_escaped: false,
            families_seen: std::collections::HashSet::new(),
            motion_seen,
            menu: None,
            tray: None,
            #[cfg(windows)]
            caption: None,
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
        let now = Instant::now();
        let wake = self.context.next_wake().map(|at| self.started + at);
        let (files, rest) = self.follows();
        let files = files.then(|| {
            let settle = self.poller.settle_deadline(self.followed_paths());
            self.schedule.files_next(now, settle)
        });
        let rest = rest.then(|| self.schedule.rest_next());
        let retry = self.retry_pending.then(|| now + Duration::from_millis(100));
        [wake, files, rest, retry].into_iter().flatten().min()
    }

    /// What this window has to look at: the stylesheet and watched files, and
    /// image files and the runner's stop request.
    fn follows(&self) -> (bool, bool) {
        hot::tiers(
            self.options.stylesheet.is_some(),
            self.context.watched_paths().len(),
            self.context.image_paths().len(),
            self.runner,
        )
    }

    /// The stylesheet and the files the view asked to watch.
    fn followed_paths(&self) -> impl Iterator<Item = &Path> {
        self.options
            .stylesheet
            .as_deref()
            .into_iter()
            .chain(self.context.watched_paths().iter().map(PathBuf::as_path))
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
        hot::install_stylesheet(
            self.options.stylesheet.as_deref(),
            &mut self.poller,
            &mut self.context,
            Instant::now(),
        )
    }

    /// The tick for the stylesheet and watched files.
    fn poll_followed(&mut self, now: Instant) {
        let redraw = hot::follow(
            self.options.stylesheet.as_deref(),
            &mut self.poller,
            &mut self.schedule,
            &mut self.context,
            now,
        );
        if redraw {
            self.request_redraw();
        }
    }

    /// The slow tick: image files and the development runner's stop request.
    fn poll_rest(&mut self) {
        self.images.request(&mut self.context, true);
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
        if !self.view.saves_state() {
            return;
        }
        self.context.set_now(self.started.elapsed());
        let text = {
            let shared = state.renderer.clone();
            let mut shared = shared.borrow_mut();
            self.view.hot_state(
                &mut self.context,
                self.logical_size,
                shared.renderer.text_system(),
            )
        };
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
        if let Err(error) = hot::save(window, &text) {
            eprintln!("hot state not saved: {error}");
        }
    }

    /// Fulfils what the last declaration asked the desktop for. Called
    /// between frames, because a file dialog runs a message loop of its own
    /// and nothing may be borrowed across it.
    pub(crate) fn platform_requests(&mut self) {
        self.caption_frame();
        // A window that draws its own title bar asks for what the system's
        // one would have done. The platform takes the pointer over for a
        // move or a resize, so this happens between frames, where nothing of
        // this window's is borrowed.
        for command in self.context.take_window_commands() {
            let Some(state) = &self.state else {
                continue;
            };
            match command {
                glassmere_core::WindowCommand::Drag => {
                    let _ = state.window.drag_window();
                }
                glassmere_core::WindowCommand::Resize(edge) => {
                    let _ = state.window.drag_resize_window(direction(edge));
                }
                glassmere_core::WindowCommand::Minimize => state.window.set_minimized(true),
                glassmere_core::WindowCommand::ToggleMaximize => {
                    state.window.set_maximized(!state.window.is_maximized());
                }
                glassmere_core::WindowCommand::Close => self.close_requested = true,
            }
        }
        for notification in self.context.take_notifications() {
            platform::notify(&notification, &self.options.resolved_app_id());
        }
        if let Some(menu) = self.context.take_menu() {
            // Dropping the old one first: two menus is one too many.
            self.menu = None;
            self.menu = platform::NativeMenu::install(&menu);
        }
        if let Some(tray) = self.context.take_tray() {
            self.tray = None;
            self.tray = tray.as_ref().and_then(platform::NativeTray::install);
        }
        if let Some(request) = self.context.take_file_request()
            && let Some(state) = &self.state
        {
            let window = state.window.clone();
            let paths = platform::choose_files(&window, request);
            self.context.set_chosen_files(paths);
            self.request_redraw();
        }
    }

    /// Tells Windows where the application's own maximize button is, and
    /// turns what its window procedure saw over it into ordinary events: the
    /// button belongs to the caption as far as Windows is concerned, so no
    /// mouse message of its own reaches it. On every other platform this
    /// does nothing.
    #[cfg(windows)]
    fn caption_frame(&mut self) {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Some(state) = &self.state else {
            return;
        };
        let scale = state.window.scale_factor() as f32;
        if self.caption.is_none() {
            let handle = state.window.window_handle().map(|handle| handle.as_raw());
            let Ok(RawWindowHandle::Win32(handle)) = handle else {
                return;
            };
            self.caption = glassmere_win32::attach(isize::from(handle.hwnd));
        }
        let Some(caption) = &self.caption else {
            return;
        };
        let pixels = |button: Option<glassmere_scene::Rect>| {
            button.map(|button| {
                (
                    (button.x * scale) as i32,
                    (button.y * scale) as i32,
                    ((button.x + button.width) * scale) as i32,
                    ((button.y + button.height) * scale) as i32,
                )
            })
        };
        caption.set_minimize(pixels(self.context.minimize_button()));
        caption.set_maximize(pixels(self.context.maximize_button()));
        caption.set_close(pixels(self.context.close_button()));
        let seen = caption.take_input();
        for event in seen {
            match event {
                glassmere_win32::CaptionInput::Hover(x, y) => self.caption_pointer(x, y, scale),
                glassmere_win32::CaptionInput::Left => self.input(Event::PointerLeft),
                glassmere_win32::CaptionInput::Down(x, y) => {
                    self.caption_pointer(x, y, scale);
                    self.input(Event::PointerDown);
                }
                glassmere_win32::CaptionInput::Up(x, y) => {
                    self.caption_pointer(x, y, scale);
                    self.input(Event::PointerUp);
                }
                _ => {}
            }
            self.request_redraw();
        }
    }

    /// The pointer over the maximize button, which Windows reports in client
    /// pixels outside the usual mouse messages.
    #[cfg(windows)]
    fn caption_pointer(&mut self, x: i32, y: i32, scale: f32) {
        self.pointer = Point {
            x: x as f32 / scale,
            y: y as f32 / scale,
        };
        self.input(Event::PointerMoved(self.pointer));
    }

    #[cfg(not(windows))]
    fn caption_frame(&mut self) {}

    /// Says so when a style asked for a font family that is not installed.
    /// The text is drawn in another one either way; the point is that the
    /// application finds out.
    fn check_families(
        context: &mut Context,
        seen: &mut std::collections::HashSet<String>,
        fonts: &glassmere_render_wgpu::TextSystem,
    ) {
        let missing: Vec<String> = context
            .requested_families()
            .into_iter()
            .filter(|name| !seen.contains(name) && !fonts.has_family(name))
            .collect();
        if missing.is_empty() {
            return;
        }
        for name in &missing {
            seen.insert(name.clone());
        }
        context.report_font(format!(
            "font families not installed, so another font is used: {}",
            missing.join(", ")
        ));
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
        let _frame_span = tracing::trace_span!("glassmere.frame").entered();
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
        // What a title bar of the application's own needs to know about its
        // window before it draws itself.
        self.context.set_window_info(glassmere_core::WindowInfo {
            maximized: state.window.is_maximized(),
            focused: state.window.has_focus(),
        });
        // `gsettings` answers off the event loop and wakes it: take a newer
        // answer.
        follow_system_motion(
            &mut self.context,
            &mut self.motion_seen,
            self.options.reduced_motion,
        );
        // Images decoded in the background since the last frame.
        self.images.deliver(&mut self.context);
        let stats = self.view.frame(
            &mut self.context,
            logical_size,
            shared.renderer.text_system(),
        );
        Self::check_families(
            &mut self.context,
            &mut self.families_seen,
            shared.renderer.text_system(),
        );
        if let Some(clip) = self.context.take_clip() {
            self.clipboard.borrow_mut().write_clip(clip);
        }
        if self.context.take_paste_request() {
            let clip = self.clipboard.borrow_mut().read_clip();
            self.context.set_clip(clip);
        }
        // Each new framework diagnostic is printed once; the inspector (F12)
        // lists them with their nodes. They are collected in debug builds.
        let diagnostics = self.context.diagnostics();
        for entry in diagnostics.iter().skip(self.diagnostics_seen) {
            eprintln!(
                "glassmere diagnostic: {:?} at `{}`: {}",
                entry.kind, entry.path, entry.message
            );
        }
        self.diagnostics_seen = diagnostics.len();
        self.images.request(&mut self.context, false);
        // Files asked for the first time load right away; the next frame shows them.
        let now = Instant::now();
        if hot::load_fresh(&mut self.poller, &mut self.schedule, &mut self.context, now) {
            state.window.request_redraw();
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
        let grip = self
            .context
            .hovered()
            .and_then(|id| self.context.resize_grip(id));
        // The window buttons keep the arrow, as the system's do. Windows sets
        // it over them itself, and a hand from here would fight it.
        state.window.set_cursor(if let Some(edge) = grip {
            resize_cursor(edge)
        } else {
            self.context.hovered().map_or(CursorIcon::Default, |id| {
                cursor_icon(self.context.cursor(id))
            })
        });
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
        if self.frames == 1 {
            report_startup();
        }
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
                            glassmere_devtools::GpuSnapshot {
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
                            // One entry per window that ever drew while an
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
                        let snapshot = glassmere_devtools::GpuSnapshot {
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

    pub(crate) fn input(&mut self, event: Event) {
        self.offer(event);
    }

    /// Where the pointer is on the desktop, in physical pixels, which is the
    /// only frame of reference two windows share.
    pub(crate) fn screen_pointer(&self) -> Option<(f64, f64)> {
        let state = self.state.as_ref()?;
        let origin = state.window.inner_position().ok()?;
        let scale = state.window.scale_factor();
        Some((
            f64::from(origin.x) + f64::from(self.pointer.x) * scale,
            f64::from(origin.y) + f64::from(self.pointer.y) * scale,
        ))
    }

    /// That point back in this window's own logical coordinates.
    pub(crate) fn local_pointer(&self, (x, y): (f64, f64)) -> Option<Point> {
        let state = self.state.as_ref()?;
        let origin = state.window.inner_position().ok()?;
        let scale = state.window.scale_factor();
        Some(Point {
            x: ((x - f64::from(origin.x)) / scale) as f32,
            y: ((y - f64::from(origin.y)) / scale) as f32,
        })
    }

    /// Whether that point is inside this window.
    pub(crate) fn covers(&self, (x, y): (f64, f64)) -> bool {
        let Some(state) = &self.state else {
            return false;
        };
        let Ok(origin) = state.window.inner_position() else {
            return false;
        };
        let size = state.window.inner_size();
        let (left, top) = (f64::from(origin.x), f64::from(origin.y));
        x >= left
            && y >= top
            && x < left + f64::from(size.width)
            && y < top + f64::from(size.height)
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
            Some(Err(TryRecvError::Disconnected)) => glassmere_core::DragOutcome::Cancelled,
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
        let now = Instant::now();
        let (files, rest) = self.follows();
        if files
            && self
                .schedule
                .files_due(now, self.poller.settle_deadline(self.followed_paths()))
        {
            self.schedule.looked_files(now);
            self.poll_followed(now);
        }
        if rest && self.schedule.rest_due(now) {
            self.schedule.looked_rest(now);
            self.poll_rest();
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
                if matches!(event, WindowEvent::ScaleFactorChanged { .. }) {
                    self.view.invalidate();
                }
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
                if focused {
                    // The setting may have changed while another window had
                    // focus. The host asked for it in the background when the
                    // focus event came in (`desktop.rs`); this takes what is
                    // known now.
                    apply_system_motion(
                        &mut self.context,
                        &mut self.motion_seen,
                        self.options.reduced_motion,
                    );
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
                if outside && self.os_drag.is_none() && self.context.dragging().is_some() {
                    let exported = self
                        .state
                        .as_ref()
                        .and_then(|state| os_drag::start(&state.window, self.context.dragging()?));
                    if let Some(done) = exported {
                        self.context.export_drag();
                        self.os_drag = Some(done);
                        self.poll_os_drag();
                    } else {
                        // Another window of this application may be under the
                        // pointer. Only the desktop knows where they are.
                        self.drag_escaped = true;
                    }
                }
            }
            // winit 0.30 carries no drop position and Windows sends no
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
                        winit::event::TouchPhase::Started => glassmere_core::TouchPhase::Started,
                        winit::event::TouchPhase::Moved => glassmere_core::TouchPhase::Moved,
                        winit::event::TouchPhase::Ended => glassmere_core::TouchPhase::Ended,
                        winit::event::TouchPhase::Cancelled => {
                            glassmere_core::TouchPhase::Cancelled
                        }
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
                        Event::Editor(glassmere_core::EditorCommand::AddCursorAt(self.pointer))
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

/// Draws the system's shadow around a window without the system's frame.
#[cfg(windows)]
fn frame_shadow(window: &winit::window::Window) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(RawWindowHandle::Win32(handle)) = window.window_handle().map(|handle| handle.as_raw())
    {
        glassmere_win32::frame_shadow(isize::from(handle.hwnd));
    }
}
