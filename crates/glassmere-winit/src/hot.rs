//! Development restarts that keep state. `glassmere-dev` rebuilds the app when
//! its sources change, asks the running process to stop through a file, and
//! starts the new build; the app saves its state with `App::save_state` and reads
//! it back here. Outside the runner everything in this module is inert.
use std::path::PathBuf;
use std::time::SystemTime;

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

/// Modification-time polling for the files a window depends on.
// Polled with the window's 500 ms tick; move to OS change
// notifications if many files are watched or latency matters.
#[derive(Default)]
pub(crate) struct Poller {
    seen: std::collections::HashMap<PathBuf, Option<SystemTime>>,
}

impl Poller {
    /// Whether the file is new, gone or modified since the last call, without
    /// reading it.
    pub fn stamp_changed(&mut self, path: &std::path::Path) -> bool {
        let stamp = std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok();
        self.seen.insert(path.to_owned(), stamp) != Some(stamp)
    }

    /// The file's content when it is new or changed since the last call, or
    /// the error that prevented reading it (reported once per change).
    pub fn changed(&mut self, path: &std::path::Path) -> Option<Result<Vec<u8>, String>> {
        let modified = std::fs::metadata(path).and_then(|metadata| metadata.modified());
        if !self.stamp_changed(path) {
            return None;
        }
        Some(
            modified
                .and_then(|_| std::fs::read(path))
                .map_err(|error| format!("{}: {error}", path.display())),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
