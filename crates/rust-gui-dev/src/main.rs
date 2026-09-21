//! `rust-gui-dev [--watch PATH]... [--poll MS] [cargo build args]... [-- app args...]`
//!
//! Builds the app, runs it, and when a watched `.rs` or `.toml` file changes
//! rebuilds it. After a successful build the running app is asked to stop
//! (it saves its state through `Ui::hot_state`), and the new build starts
//! where the old one was. A failed build leaves the old app running.
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime};

/// Same names as `rust_gui::hot::{STATE_ENV, STOP_ENV}`.
const STATE_ENV: &str = "RUST_GUI_HOT_STATE";
const STOP_ENV: &str = "RUST_GUI_HOT_STOP";
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq)]
struct Options {
    watch: Vec<PathBuf>,
    poll: Duration,
    cargo: Vec<String>,
    app: Vec<String>,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        watch: Vec::new(),
        poll: Duration::from_millis(500),
        cargo: Vec::new(),
        app: Vec::new(),
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--" => {
                options.app.extend(args);
                break;
            }
            "--watch" => options
                .watch
                .push(args.next().ok_or("--watch needs a path")?.into()),
            "--poll" => {
                let value = args.next().ok_or("--poll needs milliseconds")?;
                let millis: u64 = value
                    .parse()
                    .map_err(|_| format!("invalid --poll value `{value}`"))?;
                options.poll = Duration::from_millis(millis.max(50));
            }
            _ => options.cargo.push(arg),
        }
    }
    if options.watch.is_empty() {
        options.watch.push(".".into());
    }
    Ok(options)
}

/// The last executable named in cargo's JSON build messages.
// ponytail: string search instead of a JSON parser; cargo writes one message
// per line with the field spelled exactly like this.
fn executable_from(lines: impl IntoIterator<Item = String>) -> Option<PathBuf> {
    const FIELD: &str = "\"executable\":\"";
    let mut found = None;
    for line in lines {
        let Some(start) = line.find(FIELD) else {
            continue;
        };
        let mut path = String::new();
        let mut chars = line[start + FIELD.len()..].chars();
        while let Some(ch) = chars.next() {
            match ch {
                '"' => break,
                '\\' => path.extend(chars.next()),
                other => path.push(other),
            }
        }
        found = Some(PathBuf::from(path));
    }
    found
}

/// Newest modification time and count of the sources under the watch roots.
fn snapshot(roots: &[PathBuf]) -> (Option<SystemTime>, usize) {
    fn visit(path: &Path, newest: &mut Option<SystemTime>, count: &mut usize) {
        let Ok(metadata) = std::fs::metadata(path) else {
            return;
        };
        if metadata.is_dir() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if name == "target" || (name.starts_with('.') && name.len() > 1) {
                return;
            }
            for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
                visit(&entry.path(), newest, count);
            }
        } else if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("rs" | "toml")
        ) {
            *count += 1;
            if let Ok(modified) = metadata.modified() {
                *newest = Some(newest.map_or(modified, |old| old.max(modified)));
            }
        }
    }
    let (mut newest, mut count) = (None, 0);
    for root in roots {
        visit(root, &mut newest, &mut count);
    }
    (newest, count)
}

fn build(cargo: &[String]) -> Option<PathBuf> {
    let mut child = Command::new("cargo")
        .arg("build")
        .arg("--message-format=json-render-diagnostics")
        .args(cargo)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| eprintln!("rust-gui-dev: cannot run cargo: {error}"))
        .ok()?;
    let lines = BufReader::new(child.stdout.take()?)
        .lines()
        .map_while(Result::ok);
    let executable = executable_from(lines);
    let status = child.wait().ok()?;
    if !status.success() {
        return None;
    }
    if executable.is_none() {
        eprintln!("rust-gui-dev: the build produced no executable; pass --bin or --example");
    }
    executable
}

fn start(executable: &Path, options: &Options, state: &Path, stop: &Path) -> Option<Child> {
    Command::new(executable)
        .args(&options.app)
        .env(STATE_ENV, state)
        .env(STOP_ENV, stop)
        .spawn()
        .map_err(|error| {
            eprintln!(
                "rust-gui-dev: cannot start {}: {error}",
                executable.display()
            )
        })
        .ok()
}

/// Asks the app to save and exit, and kills it if it does not.
fn stop(child: &mut Child, stop_file: &Path) {
    let _ = std::fs::write(stop_file, "stop");
    let deadline = Instant::now() + STOP_TIMEOUT;
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            let _ = std::fs::remove_file(stop_file);
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    eprintln!("rust-gui-dev: the app did not stop in time; its state may be stale");
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(stop_file);
}

fn main() -> ExitCode {
    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("rust-gui-dev: {error}");
            return ExitCode::from(2);
        }
    };
    let directory = std::env::temp_dir().join(format!("rust-gui-dev-{}", std::process::id()));
    if let Err(error) = std::fs::create_dir_all(&directory) {
        eprintln!("rust-gui-dev: {}: {error}", directory.display());
        return ExitCode::FAILURE;
    }
    let (state_file, stop_file) = (directory.join("state"), directory.join("stop"));
    let mut seen = snapshot(&options.watch);
    let mut child: Option<Child> = build(&options.cargo)
        .and_then(|executable| start(&executable, &options, &state_file, &stop_file));
    if child.is_none() {
        eprintln!("rust-gui-dev: waiting for a change");
    }
    let code = loop {
        std::thread::sleep(options.poll);
        if let Some(running) = &mut child
            && let Ok(Some(status)) = running.try_wait()
        {
            // Closing the app ends the session.
            break status.code().unwrap_or(1);
        }
        let now = snapshot(&options.watch);
        if now == seen {
            continue;
        }
        seen = now;
        eprintln!("rust-gui-dev: change detected, rebuilding");
        let Some(executable) = build(&options.cargo) else {
            eprintln!("rust-gui-dev: build failed; the running app is unchanged");
            continue;
        };
        if let Some(running) = &mut child {
            stop(running, &stop_file);
        }
        child = start(&executable, &options, &state_file, &stop_file);
    };
    let _ = std::fs::remove_dir_all(&directory);
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn arguments_split_into_runner_cargo_and_app_parts() {
        let options = parse_args(strings(&[
            "--watch",
            "src",
            "-p",
            "app",
            "--poll",
            "10",
            "--example",
            "demo",
            "--",
            "--smoke",
            "--watch",
        ]))
        .unwrap();
        assert_eq!(
            options,
            Options {
                watch: vec!["src".into()],
                poll: Duration::from_millis(50),
                cargo: strings(&["-p", "app", "--example", "demo"]),
                app: strings(&["--smoke", "--watch"]),
            }
        );
        assert_eq!(
            parse_args(strings(&[])).unwrap().watch,
            [PathBuf::from(".")]
        );
        assert!(parse_args(strings(&["--poll", "soon"])).is_err());
        assert!(parse_args(strings(&["--watch"])).is_err());
    }

    #[test]
    fn the_last_executable_of_the_build_messages_is_used() {
        let lines = strings(&[
            r#"{"reason":"compiler-artifact","target":{"name":"dep"},"executable":null}"#,
            r#"{"reason":"compiler-artifact","executable":"D:\\dev\\target\\debug\\examples\\old.exe","fresh":true}"#,
            r#"{"reason":"compiler-artifact","executable":"D:\\dev\\my \"app\"\\demo.exe","fresh":false}"#,
            r#"{"reason":"build-finished","success":true}"#,
        ]);
        assert_eq!(
            executable_from(lines),
            Some(PathBuf::from(r#"D:\dev\my "app"\demo.exe"#))
        );
        assert_eq!(executable_from(strings(&["{}"])), None);
    }

    #[test]
    fn snapshots_see_source_changes_and_ignore_build_output() {
        let root = std::env::temp_dir().join(format!("rust-gui-dev-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(root.join("notes.md"), "ignored").unwrap();
        std::fs::write(root.join("target/built.rs"), "ignored").unwrap();
        std::fs::write(root.join(".git/config.toml"), "ignored").unwrap();
        let roots = [root.clone()];
        let first = snapshot(&roots);
        assert_eq!(first.1, 2);
        assert_eq!(snapshot(&roots), first);
        std::fs::write(root.join("src/extra.rs"), "").unwrap();
        let second = snapshot(&roots);
        assert_eq!(second.1, 3);
        // An edit moves the newest modification time.
        let later = SystemTime::now() + Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(root.join("src/main.rs"))
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_ne!(snapshot(&roots), second);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
