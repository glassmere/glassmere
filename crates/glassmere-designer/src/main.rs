use glassmere::{AppOptions, Size, run};
use glassmere_designer::{Designer, STARTER};
use std::path::PathBuf;

const USAGE: &str = "usage: glassmere-designer <view.glass> [--style <sheet.style>]

Shows the markup file and follows every change saved to it. A file that
does not exist is created with a small starting view. Without --style, a
stylesheet next to the file with the same name (view.style) is used.";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut path: Option<PathBuf> = None;
    let mut style: Option<PathBuf> = None;
    let mut smoke = false;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--style" => style = arguments.next().map(PathBuf::from),
            // Two frames and out, for automated runs.
            "--smoke" => smoke = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => path = Some(argument.into()),
        }
    }
    let Some(path) = path else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    if !path.exists() {
        std::fs::write(&path, STARTER)?;
    }
    // The watcher and the stylesheet's images want a full path.
    let path = std::path::absolute(&path)?;
    let style = style.or_else(|| {
        let beside = path.with_extension("style");
        beside.exists().then_some(beside)
    });
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    run(
        AppOptions {
            title: format!("{name} - Glassmere Designer"),
            size: Size::new(1280.0, 800.0),
            stylesheet: style,
            frame_limit: smoke.then_some(2),
            ..AppOptions::default()
        },
        Designer::open(path)?,
    )
}
