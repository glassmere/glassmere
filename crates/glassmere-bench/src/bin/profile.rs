//! Runs one scenario for a while, so that a profiler has something to look
//! at, and on Linux and macOS samples it itself and writes a flamegraph.
//!
//! ```sh
//! cargo run -p glassmere-bench --profile profiling --bin profile -- --list
//! cargo run -p glassmere-bench --profile profiling --bin profile -- form/relayout
//! cargo run -p glassmere-bench --profile profiling --bin profile -- form/relayout --flamegraph relayout.svg
//! ```
//!
//! Without `--flamegraph` it only runs and reports how long a step took,
//! which is what an outside profiler (samply, perf, Instruments, the
//! Windows Performance Recorder) needs it to do. See `docs/profiling.md`.
use glassmere_bench::scenarios;
use std::process::ExitCode;
use std::time::{Duration, Instant};

struct Options {
    scenario: String,
    seconds: f64,
    flamegraph: Option<String>,
}

fn usage() -> ExitCode {
    eprintln!("usage: profile <scenario> [--seconds N] [--flamegraph FILE.svg]");
    eprintln!("       profile --list");
    ExitCode::from(2)
}

fn parse(mut arguments: impl Iterator<Item = String>) -> Option<Options> {
    let mut options = Options {
        scenario: String::new(),
        seconds: 5.0,
        flamegraph: None,
    };
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--seconds" => options.seconds = arguments.next()?.parse().ok()?,
            "--flamegraph" => options.flamegraph = Some(arguments.next()?),
            name if !name.starts_with('-') && options.scenario.is_empty() => {
                options.scenario = name.to_owned();
            }
            _ => return None,
        }
    }
    (!options.scenario.is_empty() && options.seconds > 0.0).then_some(options)
}

/// Runs `step` for about `seconds` and returns how many steps that was and
/// how long they took.
fn run(step: &mut dyn FnMut(), seconds: f64) -> (u64, Duration) {
    let limit = Duration::from_secs_f64(seconds);
    let started = Instant::now();
    let mut steps = 0;
    while started.elapsed() < limit {
        // The clock is read once per batch, so that it is not what is measured.
        for _ in 0..16 {
            step();
        }
        steps += 16;
    }
    (steps, started.elapsed())
}

#[cfg(unix)]
fn sampled(
    step: &mut dyn FnMut(),
    seconds: f64,
    path: &str,
) -> Result<(u64, Duration), Box<dyn std::error::Error>> {
    // A prime rate, so that the samples do not fall in step with the frames.
    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(997)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()?;
    let measured = run(step, seconds);
    let report = guard.report().build()?;
    report.flamegraph(std::fs::File::create(path)?)?;
    Ok(measured)
}

#[cfg(not(unix))]
fn sampled(
    _: &mut dyn FnMut(),
    _: f64,
    _: &str,
) -> Result<(u64, Duration), Box<dyn std::error::Error>> {
    Err(
        "--flamegraph samples from inside the process, which needs Linux or macOS; \
         on Windows run without it under an outside profiler (see docs/profiling.md)"
            .into(),
    )
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let all = scenarios();
    if arguments.iter().any(|argument| argument == "--list") {
        for scenario in &all {
            println!("{:<22} {}", scenario.name, scenario.about);
        }
        return ExitCode::SUCCESS;
    }
    let Some(options) = parse(arguments.into_iter()) else {
        return usage();
    };
    let Some(scenario) = all
        .iter()
        .find(|scenario| scenario.name == options.scenario)
    else {
        eprintln!(
            "no scenario named `{}`; --list shows them",
            options.scenario
        );
        return ExitCode::from(2);
    };
    let mut step = (scenario.build)();
    // Warm caches and pools before anything is counted.
    for _ in 0..32 {
        step();
    }
    let measured = match &options.flamegraph {
        Some(path) => sampled(&mut step, options.seconds, path),
        None => Ok(run(&mut step, options.seconds)),
    };
    match measured {
        Ok((steps, elapsed)) => {
            let each = elapsed.as_secs_f64() * 1_000_000.0 / steps as f64;
            println!(
                "{}: {steps} steps in {:.2} s, {each:.1} µs each",
                scenario.name,
                elapsed.as_secs_f64()
            );
            if let Some(path) = &options.flamegraph {
                println!("flamegraph written to {path}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
