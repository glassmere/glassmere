//! Runs one scenario so that it can be looked at: for a while, for a
//! profiler; a fixed number of steps, for a tool that counts; or with a
//! fingerprint of what every step left, to check that a change made a step
//! cheaper and nothing else.
//!
//! ```sh
//! profile --list
//! profile form/relayout                           # five seconds of it
//! profile form/relayout --flamegraph relayout.svg # sampled (Linux, macOS)
//! profile form/relayout --folded relayout.folded  # the stacks themselves
//! profile form/relayout --steps 200               # exactly 200 steps
//! profile form/relayout --fingerprint             # what 64 steps left
//! profile all --fingerprint                       # for every scenario
//! ```
//!
//! Run it through `cargo run -p glassmere-bench --profile profiling --bin
//! profile --`. See `docs/profiling.md`.
use glassmere_bench::{Fingerprint, Scenario, scenarios};
use std::process::ExitCode;
use std::time::{Duration, Instant};

struct Options {
    scenario: String,
    seconds: f64,
    /// Exactly this many steps, in place of running for a time.
    steps: Option<u64>,
    flamegraph: Option<String>,
    folded: Option<String>,
    fingerprint: bool,
    /// Where to write what the last step left, in full.
    dump: Option<String>,
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: profile <scenario|all> [--seconds N | --steps N] [--flamegraph FILE.svg] \
         [--folded FILE] [--fingerprint] [--dump FILE]"
    );
    eprintln!("       profile --list");
    ExitCode::from(2)
}

fn parse(mut arguments: impl Iterator<Item = String>) -> Option<Options> {
    let mut options = Options {
        scenario: String::new(),
        seconds: 5.0,
        steps: None,
        flamegraph: None,
        folded: None,
        fingerprint: false,
        dump: None,
    };
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--seconds" => options.seconds = arguments.next()?.parse().ok()?,
            "--steps" => options.steps = Some(arguments.next()?.parse().ok()?),
            "--flamegraph" => options.flamegraph = Some(arguments.next()?),
            "--folded" => options.folded = Some(arguments.next()?),
            "--fingerprint" => options.fingerprint = true,
            "--dump" => options.dump = Some(arguments.next()?),
            name if !name.starts_with('-') && options.scenario.is_empty() => {
                options.scenario = name.to_owned();
            }
            _ => return None,
        }
    }
    (!options.scenario.is_empty() && options.seconds > 0.0).then_some(options)
}

/// Runs `step` for about `seconds`, or exactly `steps` times, and returns
/// how many steps that was and how long they took.
fn run(step: &mut dyn FnMut(), seconds: f64, steps: Option<u64>) -> (u64, Duration) {
    let started = Instant::now();
    if let Some(steps) = steps {
        for _ in 0..steps {
            step();
        }
        return (steps, started.elapsed());
    }
    let limit = Duration::from_secs_f64(seconds);
    let mut done = 0;
    while started.elapsed() < limit {
        // The clock is read once per batch, so that it is not what is measured.
        for _ in 0..16 {
            step();
        }
        done += 16;
    }
    (done, started.elapsed())
}

#[cfg(unix)]
fn sampled(
    step: &mut dyn FnMut(),
    options: &Options,
) -> Result<(u64, Duration), Box<dyn std::error::Error>> {
    use std::io::Write as _;
    // A prime rate, so that the samples do not fall in step with the frames.
    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(997)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()?;
    let measured = run(step, options.seconds, options.steps);
    let report = guard.report().build()?;
    if let Some(path) = &options.flamegraph {
        report.flamegraph(std::fs::File::create(path)?)?;
    }
    if let Some(path) = &options.folded {
        // One line a stack, outermost frame first, then how many samples
        // had it: the form flamegraph tools and speedscope read. A frame is
        // `function@file:line` where the line is known.
        let mut lines: Vec<String> = report
            .data
            .iter()
            .map(|(stack, samples)| {
                let frames: Vec<String> = stack
                    .frames
                    .iter()
                    .rev()
                    .flat_map(|frame| frame.iter().rev())
                    .map(|symbol| {
                        let name = symbol.name().replace(';', ":");
                        match (&symbol.filename, symbol.lineno) {
                            (Some(file), Some(line)) => {
                                let file = file.to_string_lossy();
                                let short = file.rsplit("/crates/").next().unwrap_or(&file);
                                format!("{name}@{short}:{line}")
                            }
                            _ => name,
                        }
                    })
                    .collect();
                format!("{} {samples}", frames.join(";"))
            })
            .collect();
        lines.sort();
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        for line in lines {
            writeln!(file, "{line}")?;
        }
    }
    Ok(measured)
}

#[cfg(not(unix))]
fn sampled(
    _: &mut dyn FnMut(),
    _: &Options,
) -> Result<(u64, Duration), Box<dyn std::error::Error>> {
    Err(
        "--flamegraph and --folded sample from inside the process, which needs Linux or \
         macOS; on Windows run without them under an outside profiler (see \
         docs/profiling.md)"
            .into(),
    )
}

/// The fingerprint of what each of `steps` steps of a scenario left, and
/// the full text of what the last one did.
fn fingerprint(scenario: &Scenario, steps: u64) -> (u64, String) {
    let mut step = (scenario.build)();
    let mut hash = Fingerprint::default();
    let mut last = step.describe();
    hash.add(&last);
    for _ in 0..steps {
        step.step();
        last = step.describe();
        hash.add(&last);
    }
    (hash.value(), last)
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let all = scenarios();
    if arguments.iter().any(|argument| argument == "--list") {
        for scenario in &all {
            println!("{:<24} {}", scenario.name, scenario.about);
        }
        return ExitCode::SUCCESS;
    }
    let Some(options) = parse(arguments.into_iter()) else {
        return usage();
    };
    let chosen: Vec<&Scenario> = all
        .iter()
        .filter(|scenario| options.scenario == "all" || scenario.name == options.scenario)
        .collect();
    if chosen.is_empty() {
        eprintln!(
            "no scenario named `{}`; --list shows them",
            options.scenario
        );
        return ExitCode::from(2);
    }
    if options.fingerprint || options.dump.is_some() {
        let steps = options.steps.unwrap_or(64);
        for scenario in &chosen {
            let (hash, last) = fingerprint(scenario, steps);
            println!("{:<24} {steps} steps {hash:016x}", scenario.name);
            if let Some(path) = &options.dump
                && let Err(error) = std::fs::write(path, last)
            {
                eprintln!("{path}: {error}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }
    if chosen.len() != 1 {
        eprintln!("`all` is for --fingerprint; name one scenario to run it");
        return ExitCode::from(2);
    }
    let scenario = chosen[0];
    let mut built = (scenario.build)();
    let mut step = || built.step();
    // Warm caches and pools before anything is counted.
    for _ in 0..32 {
        step();
    }
    let measured = if options.flamegraph.is_some() || options.folded.is_some() {
        sampled(&mut step, &options)
    } else {
        Ok(run(&mut step, options.seconds, options.steps))
    };
    match measured {
        Ok((steps, elapsed)) => {
            let each = elapsed.as_secs_f64() * 1_000_000.0 / steps as f64;
            println!(
                "{}: {steps} steps in {:.2} s, {each:.1} µs each",
                scenario.name,
                elapsed.as_secs_f64()
            );
            for path in [&options.flamegraph, &options.folded].into_iter().flatten() {
                println!("written to {path}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
