//! The reduced-motion preference of GNOME and the desktops that follow it,
//! asked of `gsettings`. The tool takes tens of milliseconds and may hang, so
//! it runs on a thread of its own and the window only ever reads the answer
//! that was kept.
use crate::wakeup::WakeHandle;
use std::{
    ffi::{OsStr, OsString},
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// No probe starts within this of the start of the previous one, whatever the
/// number of windows that gained focus.
const MIN_INTERVAL: Duration = Duration::from_secs(1);
/// A tool that has not answered by then is killed and has no answer.
const TOOL_TIMEOUT: Duration = Duration::from_secs(3);
const TOOL_POLL: Duration = Duration::from_millis(5);

static PROBE: Probe = Probe::new();

/// `enable-animations` is on or off; off is reduced motion.
fn reduced_from_enable_animations(stdout: &str) -> Option<bool> {
    match stdout.trim() {
        "false" => Some(true),
        "true" => Some(false),
        _ => None,
    }
}

fn probe_due(running: bool, last_start: Option<Instant>, now: Instant) -> bool {
    !running && last_start.is_none_or(|start| now.saturating_duration_since(start) >= MIN_INTERVAL)
}

/// Without a session bus there is no desktop session whose setting could be
/// followed. `gsettings` would still be started, and GLib would create its
/// dconf cache file, on every headless run, so it is not asked.
fn session_bus_present(address: Option<&OsStr>, runtime_bus_exists: bool) -> bool {
    address.is_some_and(|address| !address.is_empty()) || runtime_bus_exists
}

/// What `program` printed on its standard output, when it started, exited
/// with success within `limit` and printed text. Anything else, including a
/// program that is not installed, is no answer.
fn read_stdout(program: &str, args: &[&str], limit: Duration) -> Option<String> {
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        // A child that outlives this call must not hold the pipe of a log.
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    collect(child, limit)
}

/// What `child` printed, when it exited with success within `limit`.
fn collect(mut child: Child, limit: Duration) -> Option<String> {
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < limit => thread::sleep(TOOL_POLL),
            _ => {
                // Reaped as well, so a hung tool leaves no zombie behind.
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut text = String::new();
    child
        .stdout
        .take()?
        .take(1024)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

struct State {
    answer: Option<bool>,
    running: bool,
    last_start: Option<Instant>,
}

/// The last answer, how many times it changed, and whether a thread is asking.
/// The generation changes together with the answer under one lock, so the
/// pair a `snapshot` returns is always one the probe stored. The atomic
/// copy of it lets a frame ask whether anything changed without the lock.
struct Probe {
    generation: AtomicUsize,
    state: Mutex<State>,
}

impl Probe {
    const fn new() -> Self {
        Self {
            generation: AtomicUsize::new(0),
            state: Mutex::new(State {
                answer: None,
                running: false,
                last_start: None,
            }),
        }
    }

    /// Nothing that can panic runs under the lock; a poisoned one is still
    /// consistent.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn generation(&self) -> usize {
        self.generation.load(Ordering::Acquire)
    }

    fn snapshot(&self) -> (usize, Option<bool>) {
        let state = self.lock();
        (self.generation.load(Ordering::Acquire), state.answer)
    }

    /// Keeps `answer` and says whether it differs from the one kept. No
    /// answer, a failed read, changes nothing: the setting did not change
    /// because it could not be read.
    fn store(&self, answer: Option<bool>) -> bool {
        let Some(answer) = answer else {
            return false;
        };
        let mut state = self.lock();
        if state.answer == Some(answer) {
            return false;
        }
        state.answer = Some(answer);
        self.generation.fetch_add(1, Ordering::Release);
        true
    }

    /// Starts a thread that runs `read` and keeps what it returns, then calls
    /// `changed` when that differs from the answer kept, unless a thread is
    /// still asking or one started less than `MIN_INTERVAL` ago. The handle is
    /// there for tests to wait on.
    fn refresh(
        &'static self,
        now: Instant,
        read: impl FnOnce() -> Option<bool> + Send + 'static,
        changed: impl FnOnce() + Send + 'static,
    ) -> Option<JoinHandle<()>> {
        {
            let mut state = self.lock();
            if !probe_due(state.running, state.last_start, now) {
                return None;
            }
            state.running = true;
            state.last_start = Some(now);
        }
        // Clears `running` when the thread ends, however it ends, and when it
        // cannot be made: the closure is dropped unrun then. `last_start`
        // stays, so a thread that cannot be made is not retried on every focus
        // either.
        let running = Running(self);
        thread::Builder::new()
            .name("glassmere-motion".into())
            .spawn(move || {
                let _running = running;
                if self.store(read()) {
                    changed();
                }
            })
            .ok()
    }
}

struct Running(&'static Probe);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.lock().running = false;
    }
}

/// The answer kept and the generation it has, together.
pub(crate) fn snapshot() -> (usize, Option<bool>) {
    PROBE.snapshot()
}

/// Differs from the generation of a `snapshot` once the answer changed since.
pub(crate) fn generation() -> usize {
    PROBE.generation()
}

/// What `gsettings` is asked: the value of `enable-animations`.
const QUESTION: [&str; 3] = ["get", "org.gnome.desktop.interface", "enable-animations"];

/// Whether the tool `command` (a program and the arguments that go before the
/// question) says animations are off, and so asks for reduced motion.
fn ask(command: &[&str]) -> Option<bool> {
    let (program, before) = command.split_first()?;
    let args: Vec<&str> = before.iter().copied().chain(QUESTION).collect();
    read_stdout(program, &args, TOOL_TIMEOUT).and_then(|text| reduced_from_enable_animations(&text))
}

/// Whether the environment, read through `var`, has a session bus.
fn session_bus_in(var: impl Fn(&str) -> Option<OsString>) -> bool {
    let address = var("DBUS_SESSION_BUS_ADDRESS");
    let runtime_bus =
        var("XDG_RUNTIME_DIR").is_some_and(|dir| Path::new(&dir).join("bus").exists());
    session_bus_present(address.as_deref(), runtime_bus)
}

/// Starts a probe of `command` unless the environment has no session bus, and
/// calls `changed` when its answer is a new one.
fn refresh_from(
    probe: &'static Probe,
    var: impl Fn(&str) -> Option<OsString>,
    now: Instant,
    command: &'static [&'static str],
    changed: impl FnOnce() + Send + 'static,
) -> Option<JoinHandle<()>> {
    if !session_bus_in(var) {
        return None;
    }
    probe.refresh(now, move || ask(command), changed)
}

/// Asks `gsettings` in the background, and wakes the event loop if the answer
/// changed. Called on the event loop's thread: the environment is read here
/// and the thread only runs the tool.
pub(crate) fn refresh(wake: &WakeHandle) {
    let wake = wake.clone();
    refresh_from(
        &PROBE,
        |name| std::env::var_os(name),
        Instant::now(),
        &["gsettings"],
        move || {
            wake.request_redraw();
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::AtomicBool, mpsc};

    const SECOND: Duration = Duration::from_secs(1);

    /// A stand-in for `gsettings` that says animations are off, and only to
    /// exactly the question the host asks.
    const ANSWERS_THE_QUESTION: &[&str] = &[
        "sh",
        "-c",
        "[ $# -eq 3 ] && [ \"$1\" = get ] && [ \"$2\" = org.gnome.desktop.interface ] \
         && [ \"$3\" = enable-animations ] && echo false",
        "sh",
    ];

    /// An environment of these variables and no others.
    fn environment<'a>(
        variables: &'a [(&'a str, OsString)],
    ) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            variables
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
        }
    }

    /// One probe that answers `answer` at `now`, waited for. Whether it
    /// called `changed`.
    fn probe_once(probe: &'static Probe, now: Instant, answer: Option<bool>) -> bool {
        let called = Arc::new(AtomicBool::new(false));
        let flag = called.clone();
        probe
            .refresh(
                now,
                move || answer,
                move || flag.store(true, Ordering::SeqCst),
            )
            .expect("the probe was due")
            .join()
            .expect("the probe thread did not panic");
        called.load(Ordering::SeqCst)
    }

    #[test]
    fn gsettings_answers_map_to_the_preference() {
        assert_eq!(reduced_from_enable_animations("false\n"), Some(true));
        assert_eq!(reduced_from_enable_animations("true\n"), Some(false));
        assert_eq!(reduced_from_enable_animations("  false  "), Some(true));
        for text in [
            "",
            "\n",
            "No such schema ?org.gnome.desktop.interface?",
            "1",
            "False",
            "(true,)",
            "nonfalse",
            "true false",
            "false true",
        ] {
            assert_eq!(reduced_from_enable_animations(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_probe_is_due_once_per_interval_and_never_twice_at_once() {
        let base = Instant::now();
        assert!(probe_due(false, None, base));
        assert!(!probe_due(true, None, base));
        assert!(!probe_due(true, Some(base), base + 10 * SECOND));
        assert!(!probe_due(
            false,
            Some(base),
            base + Duration::from_millis(999)
        ));
        assert!(probe_due(false, Some(base), base + SECOND));
        // A start later than `now` is not a panic and not due.
        assert!(!probe_due(false, Some(base + 5 * SECOND), base));
    }

    #[test]
    fn a_probe_stores_the_answer_and_signals_only_on_change() {
        static P: Probe = Probe::new();
        let base = Instant::now();
        // The first answer counts as a change.
        assert!(probe_once(&P, base, Some(false)));
        assert_eq!(P.snapshot(), (1, Some(false)));
        assert!(!probe_once(&P, base + 2 * SECOND, Some(false)));
        assert_eq!(P.snapshot(), (1, Some(false)));
        assert!(probe_once(&P, base + 4 * SECOND, Some(true)));
        assert_eq!(P.snapshot(), (2, Some(true)));
        assert_eq!(P.generation(), 2);
    }

    #[test]
    fn a_failed_read_keeps_the_last_answer() {
        static KEPT: Probe = Probe::new();
        let base = Instant::now();
        assert!(probe_once(&KEPT, base, Some(true)));
        assert!(!probe_once(&KEPT, base + 2 * SECOND, None));
        assert_eq!(KEPT.snapshot(), (1, Some(true)));
        assert_eq!(KEPT.generation(), 1);

        static NEVER: Probe = Probe::new();
        assert!(!probe_once(&NEVER, base, None));
        assert_eq!(NEVER.snapshot(), (0, None));
    }

    #[test]
    fn an_overlapping_or_early_refresh_is_refused() {
        static P: Probe = Probe::new();
        let base = Instant::now();
        let (started_tx, started_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let first = P
            .refresh(
                base,
                move || {
                    started_tx.send(()).unwrap();
                    release_rx.recv().ok();
                    Some(true)
                },
                || {},
            )
            .expect("the first probe was due");
        started_rx.recv().unwrap();
        // The first is still asking, however long ago it started.
        let ran = Arc::new(AtomicUsize::new(0));
        let counter = ran.clone();
        let overlapping = P.refresh(
            base + 10 * SECOND,
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                None
            },
            || {},
        );
        assert!(overlapping.is_none());
        assert_eq!(ran.load(Ordering::SeqCst), 0);
        release_tx.send(()).unwrap();
        first.join().unwrap();

        let counter = ran.clone();
        let later = P
            .refresh(
                base + 20 * SECOND,
                move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                    None
                },
                || {},
            )
            .expect("the probe was due once the first had finished");
        later.join().unwrap();
        assert_eq!(ran.load(Ordering::SeqCst), 1);

        // Finished, but started less than a second ago.
        let counter = ran.clone();
        let early = P.refresh(
            base + 20 * SECOND + Duration::from_millis(10),
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                None
            },
            || {},
        );
        assert!(early.is_none());
        assert_eq!(ran.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_panicking_read_does_not_block_later_probes() {
        static P: Probe = Probe::new();
        let base = Instant::now();
        let called = Arc::new(AtomicBool::new(false));
        let flag = called.clone();
        let handle = P
            .refresh(
                base,
                || panic!("the tool failed in a way nobody expected"),
                move || flag.store(true, Ordering::SeqCst),
            )
            .expect("the probe was due");
        assert!(handle.join().is_err());
        assert!(!called.load(Ordering::SeqCst));
        assert_eq!(P.snapshot(), (0, None));

        assert!(probe_once(&P, base + 10 * SECOND, Some(true)));
        assert_eq!(P.snapshot(), (1, Some(true)));
    }

    #[test]
    fn a_session_bus_is_needed_before_asking() {
        assert!(!session_bus_present(None, false));
        assert!(!session_bus_present(Some(OsStr::new("")), false));
        assert!(session_bus_present(Some(OsStr::new("unix:path=/x")), false));
        assert!(session_bus_present(None, true));
        assert!(session_bus_present(Some(OsStr::new("")), true));
    }

    #[test]
    fn a_tool_that_prints_the_answer_is_read() {
        let text = read_stdout("sh", &["-c", "echo false"], 10 * SECOND);
        assert_eq!(text.as_deref(), Some("false\n"));
        assert_eq!(reduced_from_enable_animations(&text.unwrap()), Some(true));
    }

    #[test]
    fn a_tool_that_fails_or_is_missing_reads_as_none() {
        let limit = 10 * SECOND;
        // A failing tool's output is not an answer, whatever it printed.
        assert_eq!(
            read_stdout("sh", &["-c", "echo false; exit 3"], limit),
            None
        );
        assert_eq!(
            read_stdout("sh", &["-c", "echo oops >&2; exit 1"], limit),
            None
        );
        assert_eq!(read_stdout("glassmere-no-such-tool-xyz", &[], limit), None);
        assert_eq!(read_stdout("sh", &["-c", "printf '\\377'"], limit), None);
    }

    #[test]
    fn a_tool_that_prints_a_lot_is_read_up_to_a_limit() {
        let script = "i=0; while [ $i -lt 2000 ]; do printf aaaa; i=$((i + 1)); done";
        let text = read_stdout("sh", &["-c", script], 10 * SECOND);
        assert_eq!(text.map(|text| text.len()), Some(1024));
    }

    #[test]
    fn a_tool_that_hangs_is_killed_and_reads_as_none() {
        let started = Instant::now();
        let text = read_stdout("sh", &["-c", "exec sleep 30"], Duration::from_millis(100));
        assert_eq!(text, None);
        assert!(started.elapsed() < 10 * SECOND);
    }

    #[test]
    fn a_snapshot_pairs_the_generation_with_its_answer() {
        static P: Probe = Probe::new();
        let base = Instant::now();
        probe_once(&P, base, Some(false));
        // A window that took the first answer.
        let (seen, answer) = P.snapshot();
        assert_eq!((seen, answer), (1, Some(false)));
        assert_eq!(P.generation(), seen);

        probe_once(&P, base + 2 * SECOND, Some(true));
        // The window is told the answer moved on, and what it moved to.
        assert_ne!(P.generation(), seen);
        let (now_seen, now_answer) = P.snapshot();
        assert_eq!((now_seen, now_answer), (P.generation(), Some(true)));
        assert_eq!(P.generation(), now_seen);
    }

    #[test]
    fn the_tool_is_asked_for_the_animations_setting_and_off_is_reduced_motion() {
        assert_eq!(ask(ANSWERS_THE_QUESTION), Some(true));
        // Animations on is no reduced motion.
        assert_eq!(ask(&["sh", "-c", "echo true", "sh"]), Some(false));
        assert_eq!(ask(&["sh", "-c", "echo maybe", "sh"]), None);
        assert_eq!(ask(&[]), None);
    }

    #[test]
    fn a_tool_that_takes_a_moment_still_answers() {
        let command = ["sh", "-c", "sleep 0.2; echo false", "sh"];
        assert_eq!(ask(&command), Some(true));
    }

    #[test]
    fn a_session_bus_is_looked_for_in_the_environment() {
        let directory = std::env::temp_dir().join(format!("glassmere-bus-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        assert!(!session_bus_in(environment(&[])));
        let address = [("DBUS_SESSION_BUS_ADDRESS", "unix:path=/x".into())];
        assert!(session_bus_in(environment(&address)));
        let empty = [("DBUS_SESSION_BUS_ADDRESS", "".into())];
        assert!(!session_bus_in(environment(&empty)));
        // The runtime directory has one when it holds a `bus`.
        let runtime = [("XDG_RUNTIME_DIR", directory.clone().into_os_string())];
        assert!(!session_bus_in(environment(&runtime)));
        std::fs::write(directory.join("bus"), "").unwrap();
        assert!(session_bus_in(environment(&runtime)));
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_probe_starts_only_where_a_session_bus_exists() {
        static P: Probe = Probe::new();
        let base = Instant::now();
        let woken = Arc::new(AtomicUsize::new(0));
        let counter = woken.clone();
        let without = refresh_from(
            &P,
            environment(&[]),
            base,
            ANSWERS_THE_QUESTION,
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        );
        assert!(without.is_none());
        assert_eq!(P.snapshot(), (0, None));
        assert_eq!(woken.load(Ordering::SeqCst), 0);

        let bus = [("DBUS_SESSION_BUS_ADDRESS", "unix:path=/x".into())];
        let counter = woken.clone();
        refresh_from(
            &P,
            environment(&bus),
            base,
            ANSWERS_THE_QUESTION,
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        )
        .expect("a session bus exists")
        .join()
        .unwrap();
        assert_eq!(P.snapshot(), (1, Some(true)));
        assert_eq!(woken.load(Ordering::SeqCst), 1);

        // The command it is given is the one it runs.
        let counter = woken.clone();
        refresh_from(
            &P,
            environment(&bus),
            base + 2 * SECOND,
            &["sh", "-c", "echo true", "sh"],
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        )
        .expect("a session bus exists")
        .join()
        .unwrap();
        assert_eq!(P.snapshot(), (2, Some(false)));
        assert_eq!(woken.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_tool_is_given_no_standard_input() {
        let script = "[ /dev/stdin -ef /dev/null ] && echo false";
        let text = read_stdout("sh", &["-c", script], 10 * SECOND);
        assert_eq!(text.as_deref(), Some("false\n"));
    }

    #[test]
    fn a_tool_is_not_given_our_standard_error() {
        let script = "[ /dev/stderr -ef /dev/null ] && echo false";
        let text = read_stdout("sh", &["-c", script], 10 * SECOND);
        assert_eq!(text.as_deref(), Some("false\n"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_tool_that_hangs_is_reaped_once_it_is_killed() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        assert_eq!(collect(child, Duration::from_millis(100)), None);
        // A killed process nobody waited for stays in the process table.
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }

    #[test]
    fn a_probe_works_after_a_thread_died_holding_its_lock() {
        static P: Probe = Probe::new();
        let died = thread::spawn(|| {
            let _held = P.lock();
            panic!("a thread died holding the lock");
        });
        assert!(died.join().is_err());
        assert!(P.state.is_poisoned());
        assert_eq!(P.snapshot(), (0, None));
        assert!(probe_once(&P, Instant::now(), Some(true)));
        assert_eq!(P.snapshot(), (1, Some(true)));
    }
}
