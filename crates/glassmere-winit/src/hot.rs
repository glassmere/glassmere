//! Development restarts that keep state. `glassmere-dev` rebuilds the app when
//! its sources change, asks the running process to stop through a file, and
//! starts the new build; the app saves its state with `App::save_state` and reads
//! it back here. Outside the runner everything in this module is inert.
use glassmere_core::{Context, StyleSheet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Set by the runner: where state is kept between restarts.
pub const STATE_ENV: &str = "GLASSMERE_HOT_STATE";
/// Set by the runner: the file whose appearance asks the app to stop.
pub const STOP_ENV: &str = "GLASSMERE_HOT_STOP";

const HEADER: &str = "glassmere-hot v1";

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Whether this process runs under the development runner.
pub fn active() -> bool {
    env_path(STATE_ENV).is_some()
}

/// Whether the runner may ask this process to stop through a file. The host
/// reads it once, not on every event.
pub(crate) fn watching() -> bool {
    may_stop(env_path(STATE_ENV), env_path(STOP_ENV))
}

/// The runner sets both paths, but a process that has only the stop file is
/// asked to stop through it all the same.
fn may_stop(state: Option<PathBuf>, stop: Option<PathBuf>) -> bool {
    state.is_some() || stop.is_some()
}

/// Window geometry saved by the previous run: outer position in physical
/// pixels and inner size in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SavedWindow {
    pub x: i32,
    pub y: i32,
    pub width: f32,
    pub height: f32,
}

fn parse(text: &str) -> Option<(Option<SavedWindow>, String)> {
    let mut lines = text.split_inclusive('\n');
    if lines.next()?.trim_end() != HEADER {
        return None;
    }
    let mut window = None;
    let mut consumed = HEADER.len() + 1;
    for line in lines {
        consumed += line.len();
        let line = line.trim_end();
        if line == "state" {
            let state = text.get(consumed.min(text.len())..).unwrap_or_default();
            return Some((window, state.to_owned()));
        }
        let mut parts = line.split_whitespace();
        if parts.next() == Some("window") {
            let numbers: Vec<f32> = parts.filter_map(|part| part.parse().ok()).collect();
            if let [x, y, width, height] = numbers[..]
                && width > 0.0
                && height > 0.0
            {
                window = Some(SavedWindow {
                    x: x as i32,
                    y: y as i32,
                    width,
                    height,
                });
            }
        }
    }
    None
}

fn format(window: Option<SavedWindow>, state: &str) -> String {
    let mut text = format!("{HEADER}\n");
    if let Some(window) = window {
        text.push_str(&format!(
            "window {} {} {} {}\n",
            window.x, window.y, window.width, window.height
        ));
    }
    text.push_str("state\n");
    text.push_str(state);
    text
}

fn load() -> Option<(Option<SavedWindow>, String)> {
    parse(&std::fs::read_to_string(env_path(STATE_ENV)?).ok()?)
}

/// The text the previous run saved through `App::save_state`, when this
/// process was started by the development runner after a restart. An `App`
/// reads it itself, before `run`, as `counter.rs` does; the runner only passes
/// the file on.
pub fn saved_state() -> Option<String> {
    load().map(|(_, state)| state)
}

pub(crate) fn saved_window() -> Option<SavedWindow> {
    load()?.0
}

pub(crate) fn save(window: Option<SavedWindow>, state: &str) -> std::io::Result<()> {
    match env_path(STATE_ENV) {
        Some(path) => std::fs::write(path, format(window, state)),
        None => Ok(()),
    }
}

pub(crate) fn stop_requested() -> bool {
    env_path(STOP_ENV).is_some_and(|path| path.exists())
}

/// How often the stylesheet and the watched files are looked at while they
/// are active: for `ACTIVE_FOR` after a window starts to follow one of them or
/// one of them changes.
const FILE_POLL: Duration = Duration::from_millis(100);
const ACTIVE_FOR: Duration = Duration::from_secs(120);
/// How often they are looked at when quiet, and how often image files and the
/// runner's stop file are. Image files are not looked at faster because a
/// window can show any number of them.
const SLOW_POLL: Duration = Duration::from_millis(500);
/// A changed file is read once its stamp has stayed the same this long, so a
/// writer that truncates and then writes is not read in between.
const SETTLE: Duration = Duration::from_millis(40);
/// A file that never stops changing is read this long after its change was
/// first seen, so it is not starved.
const SETTLE_LIMIT: Duration = Duration::from_millis(500);

/// What a look at a file found: its modification time and size, or nothing
/// when it is absent or unreadable.
type Stamp = Option<(SystemTime, u64)>;

/// One look at the file, with the error that made it a `None`.
fn look(path: &Path) -> (Stamp, Option<std::io::Error>) {
    match std::fs::metadata(path)
        .and_then(|metadata| metadata.modified().map(|t| (t, metadata.len())))
    {
        Ok(stamp) => (Some(stamp), None),
        Err(error) => (None, Some(error)),
    }
}

/// The file's content, or the error that prevented reading it: the one `look`
/// found, so a missing file is not opened again.
fn read(path: &Path, error: Option<std::io::Error>) -> Result<Vec<u8>, String> {
    match error {
        Some(error) => Err(error),
        None => std::fs::read(path),
    }
    .map_err(|error| format!("{}: {error}", path.display()))
}

/// A change that was seen and is waiting to stop.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Unsettled {
    stamp: Stamp,
    /// When this wait began, and when the stamp last differed from the
    /// one before.
    first: Instant,
    changed: Instant,
}

/// When a file waiting to settle is read at the latest.
fn settles_at(pending: &Unsettled) -> Instant {
    (pending.changed + SETTLE).min(pending.first + SETTLE_LIMIT)
}

#[derive(Debug, PartialEq)]
enum Verdict {
    /// Nothing to hand out.
    Same,
    /// Changed, but still changing or not yet seen to have stopped.
    Wait,
    /// Changed and quiet: read it.
    Read,
}

/// Decides what a look at `current` means for a file last handed out as
/// `handed`. A look made at or after the file's `settles_at` leaves it either
/// read or settling later than `now`, so a timer set for that moment cannot
/// fire again at once. The order of the checks keeps that so: the limit is
/// tested before a changed stamp restarts the quiet period.
fn settle(
    handed: Stamp,
    current: Stamp,
    pending: Option<Unsettled>,
    now: Instant,
) -> (Verdict, Option<Unsettled>) {
    // A change that was undone, or a gap that closed on the same file.
    if current == handed {
        return (Verdict::Same, None);
    }
    let Some(pending) = pending else {
        let first = Unsettled {
            stamp: current,
            first: now,
            changed: now,
        };
        return (Verdict::Wait, Some(first));
    };
    if now.saturating_duration_since(pending.first) >= SETTLE_LIMIT {
        return (Verdict::Read, None);
    }
    if current != pending.stamp {
        let again = Unsettled {
            stamp: current,
            changed: now,
            ..pending
        };
        return (Verdict::Wait, Some(again));
    }
    if now.saturating_duration_since(pending.changed) >= SETTLE {
        return (Verdict::Read, None);
    }
    (Verdict::Wait, Some(pending))
}

/// What the poller last handed out for a file, and the change it is waiting
/// to settle.
struct Seen {
    stamp: Stamp,
    unsettled: Option<Unsettled>,
}

/// Polling for the files a window depends on. It compares the modification
/// time and the size, and reads a changed file once it has stopped changing
/// (`settled`). The files an application follows are looked at on a
/// `Schedule`; OS change notifications are not used, because they need a
/// dependency or unsafe code.
#[derive(Default)]
pub(crate) struct Poller {
    seen: std::collections::HashMap<PathBuf, Seen>,
}

impl Poller {
    /// Records what was handed out for the file and whether that differs from
    /// the last time. It also drops a change that was waiting to settle.
    fn hand_out(&mut self, path: &Path, stamp: Stamp) -> bool {
        match self.seen.get_mut(path) {
            Some(seen) => {
                seen.unsettled = None;
                std::mem::replace(&mut seen.stamp, stamp) != stamp
            }
            None => {
                let seen = Seen {
                    stamp,
                    unsettled: None,
                };
                self.seen.insert(path.to_owned(), seen);
                true
            }
        }
    }

    /// Whether the file is new, gone or modified since the last call, without
    /// reading it.
    pub fn stamp_changed(&mut self, path: &Path) -> bool {
        let (stamp, _) = look(path);
        self.hand_out(path, stamp)
    }

    /// The file's content when it is new or changed since the last call, or
    /// the error that prevented reading it (reported once per change).
    pub fn changed(&mut self, path: &Path) -> Option<Result<Vec<u8>, String>> {
        let (stamp, error) = look(path);
        if !self.hand_out(path, stamp) {
            return None;
        }
        Some(read(path, error))
    }

    /// Like `changed`, but a file seen before is read only once it has stopped
    /// changing: its stamp has stayed the same for `SETTLE`, or it was first
    /// seen changed `SETTLE_LIMIT` ago. A file that is gone is not reported
    /// until it has stayed gone that long. A file seen for the first time is
    /// handed out at once.
    pub fn settled(&mut self, path: &Path, now: Instant) -> Option<Result<Vec<u8>, String>> {
        let (current, error) = look(path);
        let Some(seen) = self.seen.get_mut(path) else {
            self.hand_out(path, current);
            return Some(read(path, error));
        };
        let (verdict, pending) = settle(seen.stamp, current, seen.unsettled, now);
        seen.unsettled = pending;
        match verdict {
            Verdict::Same | Verdict::Wait => None,
            Verdict::Read => {
                seen.stamp = current;
                Some(read(path, error))
            }
        }
    }

    /// The moment the earliest of the followed files that is waiting to settle
    /// is read, if any is. Files that are not followed do not count, or one that
    /// was left waiting would keep the loop awake.
    pub fn settle_deadline<'a>(
        &self,
        followed: impl IntoIterator<Item = &'a Path>,
    ) -> Option<Instant> {
        if self.seen.values().all(|seen| seen.unsettled.is_none()) {
            return None;
        }
        followed
            .into_iter()
            .filter_map(|path| self.seen.get(path)?.unsettled.as_ref().map(settles_at))
            .min()
    }
}

/// When the files are looked at next. A look is timed from the last look, not
/// from the clock, so events arriving in between do not push it back.
pub(crate) struct Schedule {
    files: Instant,
    rest: Instant,
    /// The files are looked at often until then.
    active_until: Instant,
}

impl Schedule {
    pub fn new(now: Instant) -> Self {
        Self {
            files: now,
            rest: now,
            active_until: now + ACTIVE_FOR,
        }
    }

    /// Something happened to a followed file: look often for a while.
    pub fn activity(&mut self, now: Instant) {
        self.active_until = now + ACTIVE_FOR;
    }

    /// When the stylesheet and the watched files are looked at next, or
    /// sooner when one is due to be read.
    pub fn files_next(&self, now: Instant, settle: Option<Instant>) -> Instant {
        let interval = if now < self.active_until {
            FILE_POLL
        } else {
            SLOW_POLL
        };
        let next = self.files + interval;
        settle.map_or(next, |settle| settle.min(next))
    }

    pub fn files_due(&self, now: Instant, settle: Option<Instant>) -> bool {
        now >= self.files_next(now, settle)
    }

    pub fn looked_files(&mut self, now: Instant) {
        self.files = now;
    }

    /// When image files and the runner's stop file are looked at next.
    pub fn rest_next(&self) -> Instant {
        self.rest + SLOW_POLL
    }

    pub fn rest_due(&self, now: Instant) -> bool {
        now >= self.rest_next()
    }

    pub fn looked_rest(&mut self, now: Instant) {
        self.rest = now;
    }
}

/// What a window has to look at: the stylesheet and watched files (often),
/// and image files and the runner's stop file (rarely).
pub(crate) fn tiers(stylesheet: bool, watched: usize, images: usize, runner: bool) -> (bool, bool) {
    (stylesheet || watched > 0, images > 0 || runner)
}

/// Installs the sheet at `path` when it is new or changed on disk. Returns
/// whether a sheet was installed; parse errors keep the previous sheet.
pub(crate) fn install_stylesheet(
    path: Option<&Path>,
    poller: &mut Poller,
    context: &mut Context,
    now: Instant,
) -> Result<bool, String> {
    let Some(path) = path else {
        return Ok(false);
    };
    let Some(content) = poller.settled(path, now) else {
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
    context.set_stylesheet(sheet);
    Ok(true)
}

/// One look at the stylesheet and the files the view asked to watch: what has
/// settled is read and handed to the context. Whatever it applies or fails to
/// apply keeps the look frequent for a while. Returns whether the window has
/// to redraw.
pub(crate) fn follow(
    stylesheet: Option<&Path>,
    poller: &mut Poller,
    schedule: &mut Schedule,
    context: &mut Context,
    now: Instant,
) -> bool {
    let mut failed = false;
    let sheet = match install_stylesheet(stylesheet, poller, context, now) {
        Ok(installed) => installed,
        Err(error) => {
            eprintln!("stylesheet not reloaded: {error}");
            failed = true;
            false
        }
    };
    let loaded: Vec<_> = context
        .watched_paths()
        .iter()
        .filter_map(|path| Some((path.clone(), poller.settled(path, now)?)))
        .collect();
    let read = !loaded.is_empty();
    let mut changed = false;
    for (path, content) in loaded {
        if let Err(error) = &content {
            eprintln!("watched file not loaded: {error}");
        }
        changed |= context.set_watched_file(&path, content);
    }
    if sheet || failed || read {
        schedule.activity(now);
    }
    sheet || changed
}

/// Reads at once the files the view asked for that were never read. A file
/// asked for late, as when the designer is opened, is likely about to be
/// edited: look often. Returns whether any was read.
pub(crate) fn load_fresh(
    poller: &mut Poller,
    schedule: &mut Schedule,
    context: &mut Context,
    now: Instant,
) -> bool {
    let fresh: Vec<_> = context
        .watched_paths()
        .iter()
        .filter(|path| !context.watched_known(path))
        .cloned()
        .collect();
    let mut loaded = false;
    for path in fresh {
        if let Some(content) = poller.changed(&path) {
            if let Err(error) = &content {
                eprintln!("watched file not loaded: {error}");
            }
            context.set_watched_file(&path, content);
            loaded = true;
        }
    }
    if loaded {
        schedule.activity(now);
    }
    loaded
}

#[cfg(test)]
mod tests {
    use super::*;
    use glassmere_core::{App, AppDriver, Effects, MonospaceMeasurer, Ui};
    use glassmere_scene::Size;

    #[test]
    fn state_files_round_trip_with_and_without_window_geometry() {
        let window = SavedWindow {
            x: -1200,
            y: 40,
            width: 900.5,
            height: 650.0,
        };
        let state = "count 3\nname multi\nline\n";
        assert_eq!(
            parse(&format(Some(window), state)),
            Some((Some(window), state.to_owned()))
        );
        assert_eq!(parse(&format(None, "")), Some((None, String::new())));
        // A state text may itself contain the keywords.
        assert_eq!(
            parse(&format(None, "state\nwindow 1 2 3 4\n")).unwrap().1,
            "state\nwindow 1 2 3 4\n"
        );
        assert_eq!(parse("something else\nstate\nx"), None);
        assert_eq!(parse("glassmere-hot v1\nwindow 0 0 0 0\n"), None);
    }

    #[test]
    fn the_poller_reports_new_changed_and_missing_files_once() {
        let directory =
            std::env::temp_dir().join(format!("glassmere-poller-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("asset.txt");
        let mut poller = Poller::default();
        assert!(matches!(poller.changed(&path), Some(Err(_))));
        assert!(poller.changed(&path).is_none());
        std::fs::write(&path, "one").unwrap();
        assert_eq!(poller.changed(&path), Some(Ok(b"one".to_vec())));
        assert!(poller.changed(&path).is_none());
        // Modification times can be coarse: make the change visible.
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::write(&path, "two").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_eq!(poller.changed(&path), Some(Ok(b"two".to_vec())));
        std::fs::remove_dir_all(&directory).unwrap();
        assert!(matches!(poller.changed(&path), Some(Err(_))));
    }

    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("glassmere-poller-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    /// Writes the file and gives it a modification time of whole seconds, so
    /// that two writes differ in time or agree in it as a test needs.
    fn put(path: &Path, text: &str, seconds: u64) {
        std::fs::write(path, text).unwrap();
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + seconds))
            .unwrap();
    }

    fn ms(milliseconds: u64) -> Duration {
        Duration::from_millis(milliseconds)
    }

    fn content(text: &str) -> Option<Result<Vec<u8>, String>> {
        Some(Ok(text.as_bytes().to_vec()))
    }

    #[test]
    fn a_changed_file_is_read_only_after_it_has_stopped_changing() {
        let directory = scratch("settles");
        let path = directory.join("sheet.style");
        let t0 = Instant::now();
        let at = |milliseconds| t0 + ms(milliseconds);
        let mut poller = Poller::default();
        put(&path, "a", 0);
        assert_eq!(poller.settled(&path, at(0)), content("a"));
        assert_eq!(poller.settled(&path, at(100)), None);
        assert_eq!(poller.settle_deadline([path.as_path()]), None);
        put(&path, "ab", 1);
        assert_eq!(poller.settled(&path, at(200)), None);
        assert_eq!(
            poller.settle_deadline([path.as_path()]),
            Some(at(200) + SETTLE)
        );
        assert_eq!(poller.settled(&path, at(200) + SETTLE - ms(1)), None);
        assert_eq!(poller.settled(&path, at(200) + SETTLE), content("ab"));
        assert_eq!(poller.settled(&path, at(300) + SETTLE), None);
        assert_eq!(poller.settle_deadline([path.as_path()]), None);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_file_truncated_and_rewritten_within_one_timestamp_is_never_read_empty() {
        let directory = scratch("truncated");
        let path = directory.join("sheet.style");
        let t0 = Instant::now();
        let at = |milliseconds| t0 + ms(milliseconds);
        let mut poller = Poller::default();
        let mut returned = Vec::new();
        // Every write leaves the same modification time; only the size moves.
        put(&path, "long text", 5);
        returned.push(poller.settled(&path, at(0)));
        assert_eq!(returned[0], content("long text"));
        put(&path, "", 5);
        returned.push(poller.settled(&path, at(100)));
        put(&path, "final", 5);
        returned.push(poller.settled(&path, at(120)));
        returned.push(poller.settled(&path, at(120) + SETTLE - ms(1)));
        assert_eq!(&returned[1..], [None, None, None]);
        let last = poller.settled(&path, at(120) + SETTLE);
        assert_eq!(last, content("final"));
        returned.push(last);
        assert!(!returned.contains(&content("")));
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_file_replaced_through_a_gap_is_not_reported_missing() {
        let directory = scratch("gap");
        let path = directory.join("sheet.style");
        let t0 = Instant::now();
        let at = |milliseconds| t0 + ms(milliseconds);

        let mut poller = Poller::default();
        put(&path, "old", 1);
        assert_eq!(poller.settled(&path, at(0)), content("old"));
        std::fs::remove_file(&path).unwrap();
        assert_eq!(poller.settled(&path, at(100)), None);
        put(&path, "new!", 2);
        assert_eq!(poller.settled(&path, at(120)), None);
        assert_eq!(poller.settled(&path, at(120) + SETTLE - ms(1)), None);
        assert_eq!(poller.settled(&path, at(120) + SETTLE), content("new!"));

        // A file that stays away is reported once, after it has stayed away.
        let mut poller = Poller::default();
        put(&path, "old", 1);
        assert_eq!(poller.settled(&path, at(0)), content("old"));
        std::fs::remove_file(&path).unwrap();
        assert_eq!(poller.settled(&path, at(100)), None);
        assert_eq!(poller.settled(&path, at(100) + SETTLE - ms(1)), None);
        let Some(Err(message)) = poller.settled(&path, at(100) + SETTLE) else {
            panic!("a file that stayed away was not reported");
        };
        assert!(message.starts_with(&format!("{}: ", path.display())));
        assert_eq!(poller.settled(&path, at(100) + SETTLE + ms(1000)), None);
        put(&path, "back", 3);
        assert_eq!(poller.settled(&path, at(2000)), None);
        assert_eq!(poller.settled(&path, at(2000) + SETTLE), content("back"));
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_file_that_never_stops_changing_is_read_at_the_limit() {
        let directory = scratch("limit");
        let path = directory.join("log.txt");
        let t0 = Instant::now();
        let at = |milliseconds| t0 + ms(milliseconds);
        let mut poller = Poller::default();
        put(&path, "x", 0);
        assert!(poller.settled(&path, at(0)).is_some());
        // The change is first seen at 30 ms and never stays still for 40 ms.
        for k in 1..=17u64 {
            put(&path, &"x".repeat(k as usize + 1), k);
            assert_eq!(poller.settled(&path, at(30 * k)), None, "look {k}");
        }
        // The limit, not the quiet period, is when it is read next.
        assert_eq!(
            poller.settle_deadline([path.as_path()]),
            Some(at(30) + SETTLE_LIMIT)
        );
        put(&path, &"x".repeat(19), 18);
        assert!(matches!(poller.settled(&path, at(540)), Some(Ok(_))));
        // The next change starts a new wait.
        put(&path, &"x".repeat(20), 19);
        assert_eq!(poller.settled(&path, at(570)), None);
        assert_eq!(
            poller.settle_deadline([path.as_path()]),
            Some(at(570) + SETTLE)
        );
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_change_that_keeps_the_modification_time_but_not_the_size_is_seen() {
        let directory = scratch("size");
        let path = directory.join("asset.txt");
        let mut poller = Poller::default();
        put(&path, "one", 7);
        assert_eq!(poller.changed(&path), content("one"));
        put(&path, "three", 7);
        assert_eq!(poller.changed(&path), content("three"));
        assert_eq!(poller.changed(&path), None);

        let mut poller = Poller::default();
        put(&path, "one", 7);
        assert!(poller.stamp_changed(&path));
        put(&path, "twelve", 7);
        assert!(poller.stamp_changed(&path));
        assert!(!poller.stamp_changed(&path));
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn changed_is_immediate_and_clears_a_pending_change() {
        let directory = scratch("immediate");
        let path = directory.join("sheet.style");
        let other = directory.join("other.style");
        let t0 = Instant::now();
        let at = |milliseconds| t0 + ms(milliseconds);
        let mut poller = Poller::default();
        put(&path, "old", 1);
        assert_eq!(poller.settled(&path, at(0)), content("old"));
        put(&path, "new", 2);
        assert_eq!(poller.settled(&path, at(100)), None);
        assert!(poller.settle_deadline([path.as_path()]).is_some());
        // A file seen for the first time does not wait for one that does.
        put(&other, "first", 1);
        assert_eq!(poller.settled(&other, at(100)), content("first"));
        assert_eq!(poller.changed(&path), content("new"));
        assert_eq!(poller.settle_deadline([path.as_path()]), None);
        assert_eq!(poller.settled(&path, at(100) + ms(1000)), None);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    fn stamp(seconds: u64, len: u64) -> Stamp {
        Some((SystemTime::UNIX_EPOCH + Duration::from_secs(seconds), len))
    }

    #[test]
    fn settle_decides_by_stamp_and_time() {
        let t0 = Instant::now();
        let at = |milliseconds| t0 + ms(milliseconds);
        let (s1, s2, s3) = (stamp(1, 10), stamp(2, 10), stamp(3, 10));
        let waiting = Unsettled {
            stamp: s2,
            first: at(100),
            changed: at(100),
        };
        let table = [
            // Nothing differs from what was handed out, waiting or not.
            (s1, s1, None, at(0), Verdict::Same, None),
            (s1, s1, Some(waiting), at(5), Verdict::Same, None),
            // A first change starts the wait.
            (s1, s2, None, at(100), Verdict::Wait, Some(waiting)),
            // Unchanged for less than SETTLE, and for SETTLE.
            (
                s1,
                s2,
                Some(waiting),
                at(100) + SETTLE - ms(1),
                Verdict::Wait,
                Some(waiting),
            ),
            (s1, s2, Some(waiting), at(100) + SETTLE, Verdict::Read, None),
            // A different stamp restarts the quiet period, not the limit.
            (
                s1,
                s3,
                Some(waiting),
                at(120),
                Verdict::Wait,
                Some(Unsettled {
                    stamp: s3,
                    first: at(100),
                    changed: at(120),
                }),
            ),
            // The limit wins over a different stamp.
            (
                s1,
                s3,
                Some(waiting),
                at(100) + SETTLE_LIMIT,
                Verdict::Read,
                None,
            ),
            (
                s1,
                s3,
                Some(waiting),
                at(100) + SETTLE_LIMIT - ms(1),
                Verdict::Wait,
                Some(Unsettled {
                    stamp: s3,
                    first: at(100),
                    changed: at(100) + SETTLE_LIMIT - ms(1),
                }),
            ),
            // A change that was undone.
            (s1, s1, Some(waiting), at(120), Verdict::Same, None),
            // A file that is gone is read, as an error, once it has stayed gone.
            (
                s1,
                None,
                None,
                at(0),
                Verdict::Wait,
                Some(Unsettled {
                    stamp: None,
                    first: at(0),
                    changed: at(0),
                }),
            ),
            (
                s1,
                None,
                Some(Unsettled {
                    stamp: None,
                    first: at(0),
                    changed: at(0),
                }),
                at(0) + SETTLE,
                Verdict::Read,
                None,
            ),
        ];
        for (index, (handed, current, pending, now, verdict, next)) in table.into_iter().enumerate()
        {
            assert_eq!(
                settle(handed, current, pending, now),
                (verdict, next),
                "row {index}"
            );
        }
    }

    #[test]
    fn the_schedule_looks_often_after_a_change_and_rarely_when_quiet() {
        let t0 = Instant::now();
        let mut schedule = Schedule::new(t0);
        assert_eq!(schedule.files_next(t0 + ms(10), None), t0 + FILE_POLL);
        // Still active at the call, which is what decides the interval.
        let late = t0 + ms(119_950);
        schedule.looked_files(late);
        assert_eq!(schedule.files_next(late, None), late + FILE_POLL);
        let quiet = t0 + Duration::from_secs(121);
        assert_eq!(schedule.files_next(quiet, None), late + SLOW_POLL);
        let change = t0 + Duration::from_secs(200);
        schedule.activity(change);
        assert_eq!(schedule.files_next(change, None), late + FILE_POLL);
        // A file due to be read earlier than the interval wins; a later one does not.
        assert_eq!(
            schedule.files_next(change, Some(late + ms(30))),
            late + ms(30)
        );
        assert_eq!(
            schedule.files_next(change, Some(late + ms(300))),
            late + FILE_POLL
        );
    }

    #[test]
    fn the_next_look_does_not_slide_with_the_clock() {
        let t0 = Instant::now();
        let mut schedule = Schedule::new(t0);
        assert_eq!(
            schedule.files_next(t0 + ms(10), None),
            schedule.files_next(t0 + ms(90), None)
        );
        for settle in [None, Some(t0 + ms(30)), Some(t0 + ms(500))] {
            let next = schedule.files_next(t0 + ms(10), settle);
            assert!(schedule.files_due(next, settle));
            assert!(!schedule.files_due(next - Duration::from_nanos(1), settle));
        }
        // Only a look moves the next one, and it moves it into the future.
        let now = t0 + ms(300);
        schedule.looked_files(now);
        assert!(schedule.files_next(now, None) > now);
        assert!(!schedule.files_due(now, None));
        // The same for the slow tier, which does not depend on activity.
        assert_eq!(schedule.rest_next(), t0 + SLOW_POLL);
        schedule.activity(t0 + ms(400));
        assert_eq!(schedule.rest_next(), t0 + SLOW_POLL);
        assert!(schedule.rest_due(schedule.rest_next()));
        assert!(!schedule.rest_due(schedule.rest_next() - Duration::from_nanos(1)));
        schedule.looked_rest(now);
        assert_eq!(schedule.rest_next(), now + SLOW_POLL);
    }

    #[test]
    fn a_pending_file_only_counts_while_it_is_followed() {
        let directory = scratch("followed");
        let path = directory.join("sheet.style");
        let other = directory.join("other.style");
        let t0 = Instant::now();
        let mut poller = Poller::default();
        put(&path, "old", 1);
        assert!(poller.settled(&path, t0).is_some());
        put(&path, "new", 2);
        assert_eq!(poller.settled(&path, t0 + ms(100)), None);
        assert_eq!(poller.settle_deadline([]), None);
        assert_eq!(poller.settle_deadline([other.as_path()]), None);
        assert_eq!(
            poller.settle_deadline([other.as_path(), path.as_path()]),
            Some(t0 + ms(100) + SETTLE)
        );
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn the_earliest_pending_file_sets_the_deadline() {
        let directory = scratch("earliest");
        let sooner = directory.join("sooner.style");
        let later = directory.join("later.style");
        let t0 = Instant::now();
        let steady = directory.join("steady.style");
        let mut poller = Poller::default();
        put(&steady, "old", 1);
        assert!(poller.settled(&steady, t0).is_some());
        for path in [&sooner, &later] {
            put(path, "old", 1);
            assert!(poller.settled(path, t0).is_some());
            put(path, "new", 2);
        }
        assert_eq!(poller.settled(&sooner, t0 + ms(100)), None);
        assert_eq!(poller.settled(&later, t0 + ms(120)), None);
        let first = Some(t0 + ms(100) + SETTLE);
        assert_eq!(
            poller.settle_deadline([sooner.as_path(), later.as_path()]),
            first
        );
        assert_eq!(
            poller.settle_deadline([later.as_path(), sooner.as_path()]),
            first
        );
        assert_eq!(
            poller.settle_deadline([later.as_path()]),
            Some(t0 + ms(120) + SETTLE)
        );
        // A file that was read and waits for nothing does not hide the others.
        assert_eq!(
            poller.settle_deadline([steady.as_path(), sooner.as_path(), later.as_path()]),
            first
        );
        assert_eq!(
            poller.settle_deadline([steady.as_path(), later.as_path()]),
            Some(t0 + ms(120) + SETTLE)
        );
        assert_eq!(poller.settle_deadline([steady.as_path()]), None);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn only_what_there_is_to_follow_is_scheduled() {
        assert_eq!(tiers(false, 0, 0, false), (false, false));
        assert_eq!(tiers(true, 0, 0, false), (true, false));
        assert_eq!(tiers(false, 1, 0, false), (true, false));
        assert_eq!(tiers(false, 2, 0, false), (true, false));
        assert_eq!(tiers(false, 0, 1, false), (false, true));
        assert_eq!(tiers(false, 0, 3, false), (false, true));
        assert_eq!(tiers(false, 0, 0, true), (false, true));
        assert_eq!(tiers(true, 1, 1, true), (true, true));
    }

    #[test]
    fn the_intervals_are_the_ones_the_documentation_states() {
        // docs/limitations.md, docs/platform.md, docs/styling.md and the
        // changelog quote these numbers.
        assert_eq!(FILE_POLL, ms(100));
        assert_eq!(SLOW_POLL, ms(500));
        assert_eq!(SETTLE, ms(40));
        assert_eq!(SETTLE_LIMIT, ms(500));
        assert_eq!(ACTIVE_FOR, Duration::from_secs(120));
    }

    #[test]
    fn the_runner_may_stop_a_process_that_has_either_path() {
        assert!(!may_stop(None, None));
        assert!(may_stop(Some("state".into()), None));
        assert!(may_stop(None, Some("stop".into())));
        assert!(may_stop(Some("state".into()), Some("stop".into())));
    }

    /// A context whose last declaration asked for these files.
    fn asking_for(paths: &[PathBuf]) -> Context {
        struct Asks(Vec<PathBuf>);
        impl App for Asks {
            type Message = ();
            fn update(&mut self, _: (), _: &mut Effects<'_, ()>) {}
            fn view(&self, ui: &mut Ui<'_, ()>) {
                for path in &self.0 {
                    ui.watched_text(path);
                }
            }
        }
        let mut context = Context::new();
        AppDriver::new(|| true).frame(
            &mut Asks(paths.to_vec()),
            &mut context,
            Size::new(100.0, 100.0),
            &mut MonospaceMeasurer,
        );
        assert_eq!(context.watched_paths(), paths);
        context
    }

    #[test]
    fn a_file_asked_for_late_is_read_at_once_and_looked_at_often() {
        let directory = scratch("late");
        let path = directory.join("data.txt");
        let t0 = Instant::now();
        let late = t0 + Duration::from_secs(300);
        let mut poller = Poller::default();
        let mut schedule = Schedule::new(t0);
        put(&path, "one", 1);
        let mut context = asking_for(std::slice::from_ref(&path));
        assert_eq!(schedule.files_next(late, None), t0 + SLOW_POLL);
        assert!(load_fresh(&mut poller, &mut schedule, &mut context, late));
        // The context has the content: delivering it again changes nothing.
        assert!(context.watched_known(&path));
        assert!(!context.set_watched_file(&path, Ok(b"one".to_vec())));
        assert_eq!(schedule.files_next(late, None), t0 + FILE_POLL);
        // Nothing is asked for the first time now, so nothing is activity.
        let quiet = late + Duration::from_secs(300);
        assert!(!load_fresh(&mut poller, &mut schedule, &mut context, quiet));
        assert_eq!(schedule.files_next(quiet, None), t0 + SLOW_POLL);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_watched_file_that_is_read_keeps_the_look_frequent() {
        let directory = scratch("watched");
        let path = directory.join("data.txt");
        let t0 = Instant::now();
        let mut poller = Poller::default();
        let mut schedule = Schedule::new(t0);
        put(&path, "one", 1);
        let mut context = asking_for(std::slice::from_ref(&path));
        assert!(load_fresh(&mut poller, &mut schedule, &mut context, t0));
        let quiet = t0 + Duration::from_secs(300);
        assert!(!follow(
            None,
            &mut poller,
            &mut schedule,
            &mut context,
            quiet
        ));
        assert_eq!(schedule.files_next(quiet, None), t0 + SLOW_POLL);

        // A change that is still settling is not activity yet; once read it is.
        put(&path, "two", 2);
        let seen = quiet + Duration::from_secs(1);
        assert!(!follow(
            None,
            &mut poller,
            &mut schedule,
            &mut context,
            seen
        ));
        assert_eq!(schedule.files_next(seen, None), t0 + SLOW_POLL);
        let read = seen + SETTLE;
        assert!(follow(None, &mut poller, &mut schedule, &mut context, read));
        assert_eq!(schedule.files_next(read, None), t0 + FILE_POLL);

        // A file that was written but kept its content changes nothing for the
        // window, and still shows that the file is being worked on.
        let quiet = read + Duration::from_secs(300);
        put(&path, "two", 3);
        assert!(!follow(
            None,
            &mut poller,
            &mut schedule,
            &mut context,
            quiet
        ));
        assert_eq!(schedule.files_next(quiet, None), t0 + SLOW_POLL);
        let read = quiet + SETTLE;
        assert!(!follow(
            None,
            &mut poller,
            &mut schedule,
            &mut context,
            read
        ));
        assert_eq!(schedule.files_next(read, None), t0 + FILE_POLL);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_stylesheet_that_is_installed_or_rejected_keeps_the_look_frequent() {
        let directory = scratch("sheet");
        let path = directory.join("app.style");
        let t0 = Instant::now();
        let mut poller = Poller::default();
        let mut schedule = Schedule::new(t0);
        let mut context = Context::new();
        put(&path, "class a { background: #112233 }", 1);
        let sheet = Some(path.as_path());
        let quiet = t0 + Duration::from_secs(300);
        assert!(follow(
            sheet,
            &mut poller,
            &mut schedule,
            &mut context,
            quiet
        ));
        assert!(context.stylesheet().class("a").is_some());
        assert_eq!(schedule.files_next(quiet, None), t0 + FILE_POLL);

        // A sheet that does not parse is reported and the last one stays.
        let quiet = quiet + Duration::from_secs(300);
        put(&path, "theme purple", 2);
        assert!(!follow(
            sheet,
            &mut poller,
            &mut schedule,
            &mut context,
            quiet
        ));
        assert_eq!(schedule.files_next(quiet, None), t0 + SLOW_POLL);
        let rejected = quiet + SETTLE;
        assert!(!follow(
            sheet,
            &mut poller,
            &mut schedule,
            &mut context,
            rejected
        ));
        assert!(context.stylesheet().class("a").is_some());
        assert_eq!(schedule.files_next(rejected, None), t0 + FILE_POLL);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    /// The host's loop on a fake clock for one followed file: the loop wakes
    /// at `files_next`, `files_due` lets it look, and the look reads what the
    /// poller says is settled.
    struct Replay {
        path: PathBuf,
        poller: Poller,
        schedule: Schedule,
        now: Instant,
        looks: u32,
    }

    impl Replay {
        /// The window opened at `opened` and first looked at the file at `start`.
        fn new(path: &Path, opened: Instant, start: Instant) -> Self {
            let mut replay = Self {
                path: path.to_owned(),
                poller: Poller::default(),
                schedule: Schedule::new(opened),
                now: start,
                looks: 0,
            };
            replay.schedule.looked_files(start);
            assert!(replay.poller.settled(path, start).is_some());
            replay
        }

        fn next_look(&self) -> Instant {
            let settle = self.poller.settle_deadline([self.path.as_path()]);
            self.schedule.files_next(self.now, settle)
        }

        /// Waits for the next look and makes it.
        fn look(&mut self) -> Option<Result<Vec<u8>, String>> {
            let next = self.next_look();
            assert!(next > self.now, "the loop would wake at once again");
            self.now = next;
            let settle = self.poller.settle_deadline([self.path.as_path()]);
            assert!(self.schedule.files_due(self.now, settle));
            self.schedule.looked_files(self.now);
            self.looks += 1;
            let read = self.poller.settled(&self.path, self.now);
            if read.is_some() {
                self.schedule.activity(self.now);
            }
            read
        }
    }

    /// How long a change made right after a look waits to be read.
    fn wait_for_a_change(name: &str, quiet: bool) -> Duration {
        let directory = scratch(name);
        let path = directory.join("sheet.style");
        let opened = Instant::now();
        let start = if quiet {
            opened + ACTIVE_FOR + ms(1)
        } else {
            opened
        };
        put(&path, "old", 1);
        let mut replay = Replay::new(&path, opened, start);
        put(&path, "new", 2);
        let mut read = None;
        for _ in 0..10 {
            read = replay.look();
            if read.is_some() {
                break;
            }
        }
        assert_eq!(read, content("new"));
        std::fs::remove_dir_all(&directory).unwrap();
        replay.now - start
    }

    #[test]
    fn a_change_is_read_within_the_time_the_documentation_states() {
        // docs/limitations.md and the changelog: about a tenth of a second
        // while active, and 540 ms for the first change after two quiet minutes.
        assert_eq!(wait_for_a_change("wait-active", false), ms(140));
        assert_eq!(wait_for_a_change("wait-quiet", true), ms(540));
    }

    #[test]
    fn a_file_that_keeps_changing_is_looked_at_at_most_every_settle_period() {
        let directory = scratch("busy");
        let path = directory.join("log.txt");
        let t0 = Instant::now();
        put(&path, "x", 0);
        let mut replay = Replay::new(&path, t0, t0);
        let end = t0 + Duration::from_secs(10);
        let (mut written, mut write_at) = (0, t0 + ms(30));
        while replay.now < end {
            if write_at < replay.next_look() {
                written += 1;
                put(&path, &"x".repeat(written + 1), written as u64);
                write_at += ms(30);
            } else {
                replay.look();
            }
        }
        // The changelog says up to about twenty-five looks a second.
        assert!(
            (200..=250).contains(&replay.looks),
            "{} looks",
            replay.looks
        );
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_look_at_the_deadline_moves_the_deadline_into_the_future() {
        let t0 = Instant::now();
        let (s1, s2, s3) = (stamp(1, 10), stamp(2, 10), stamp(3, 10));
        let mut pendings = vec![None];
        for stamp in [s2, None] {
            for first in [0, 30] {
                for changed in [0, 20, 39, 40, 100, 499, 500] {
                    pendings.push(Some(Unsettled {
                        stamp,
                        first: t0 + ms(first),
                        changed: t0 + ms(first.max(changed)),
                    }));
                }
            }
        }
        for handed in [s1, None] {
            for current in [s1, s2, s3, None] {
                for pending in &pendings {
                    let times = match pending {
                        None => vec![t0],
                        Some(pending) => {
                            let at = settles_at(pending);
                            vec![at, at + ms(1), at + ms(100)]
                        }
                    };
                    for now in times {
                        let (verdict, next) = settle(handed, current, *pending, now);
                        if let Some(next) = next {
                            assert!(
                                settles_at(&next) > now,
                                "{handed:?} {current:?} {pending:?}"
                            );
                        }
                        if verdict != Verdict::Wait {
                            assert_eq!(next, None);
                        }
                    }
                }
            }
        }
    }
}
