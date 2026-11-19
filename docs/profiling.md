# Measuring and profiling

How to find out what a frame costs, whether a change made it cheaper, and where the time goes. [The performance model](performance.md) says what to expect; this is how to check.

Everything here runs headless from `crates/glassmere-bench`, so it needs no window, GPU or display and gives the same answer in CI as on a desk.

## The scenarios

A scenario is a small application and one step of it: a frame with the input that frame would have, a paint, or a batch of text to shape. The same step is timed by the benchmarks and sampled by the profiler.

```sh
cargo run -p glassmere-bench --profile profiling --bin profile -- --list
```

| Scenario | What a step is |
| --- | --- |
| `form/idle` | 1,200 widgets and a frame with no input. |
| `form/redeclare` | The same form declared again and found unchanged: declare and reconcile. |
| `form/memo` | The same, each row behind `Ui::memo`: what skipping a subtree costs. |
| `form/repaint` | Every row a new colour: paint as well. |
| `form/relayout` | Every row a new text: measure and layout as well. |
| `form/one-row` | The text of one row changes, as most frames of an application go. |
| `form/resize` | The window a different width every frame. |
| `form/hover` | The pointer moves to another widget: a hit test and two repaints. |
| `form/scroll` | The wheel scrolls the form: every box moves, nothing is declared. |
| `form/interact` | A checkbox clicked, a field focused, typed in and erased, Tab, a button. |
| `form/relayout-shaped` | `form/relayout` with real text shaping in place of the monospace measurer. |
| `combo/navigate` | A combo box opened, three arrow keys, Enter, and a frame after. |
| `animation/transitions` | 400 cells easing to a colour that changes every twentieth frame. |
| `table/scroll` | A grid of 1,000,000 rows scrolled ten rows. |
| `tree/scroll` | A tree of 100,000 expanded nodes scrolled ten rows. |
| `editor/type` | A character typed into a 40,000-line document, or erased from it. |
| `text/wrap-resize` | 40 wrapped paragraphs in a window dragged narrower and wider, with real shaping. |
| `render-cpu/full` | The CPU renderer painting the whole form at 1200 by 800. |
| `render-cpu/damage` | The same scene with one row damaged. |
| `pipeline/one-row` | One row's text changes and the CPU renderer paints the damage: a message to pixels. |
| `text/shape-cold` | 100 lines of text shaped for the first time. |
| `text/shape-warm` | The same 100 lines asked for again. |

Apart from `form/memo`, the form deliberately does not use `Ui::memo`: it is what the tree costs when a view declares all of it. Text shaping uses the one font the workspace carries and no system fonts, so the shaped scenarios do not depend on what is installed.

A new scenario is an entry in `scenarios()` in `crates/glassmere-bench/src/lib.rs`. Add one for anything you are about to optimize, before you optimize it.

## Benchmarks: did it get faster

```sh
cargo bench -p glassmere-bench                         # every scenario
cargo bench -p glassmere-bench -- form/                # the ones that match
```

Criterion runs each step until the timing is stable and reports the mean with its confidence interval. To compare a change with what was there before it:

```sh
git switch release/v0.1.8
cargo bench -p glassmere-bench -- --save-baseline before
git switch my-change
cargo bench -p glassmere-bench -- --baseline before
```

It then says for each scenario whether the difference is real or within the noise. A laptop on battery, a busy machine and a CI runner all add noise: compare two runs on the same machine in the same state, and distrust a difference of a few percent.

`cargo test` builds the benchmarks and runs each scenario once, so a change that breaks one fails CI. CI does not time them: shared runners are too noisy for the numbers to mean anything.

## Flamegraphs: where the time goes

The `profile` binary runs one scenario for a few seconds. On Linux and macOS it can sample itself and write a flamegraph:

```sh
cargo run -p glassmere-bench --profile profiling --bin profile -- editor/type --flamegraph type.svg
```

Open the SVG in a browser. The width of a box is the share of samples with that function on the stack; click one to zoom into it. Read it from the bottom for the wide boxes, then up into whichever is wider than it should be.

The `profiling` profile is the release build with line tables, so that stacks have names. Always profile with it: a debug build spends its time in different places.

On Windows the sampler is not available, because it reads the stack from a signal handler. Run the same binary without `--flamegraph` under a profiler that samples from outside:

```sh
cargo install samply
cargo build -p glassmere-bench --profile profiling --bin profile
samply record target/profiling/profile.exe editor/type --seconds 10
```

`samply` opens the Firefox Profiler with a flame graph, a call tree and an inverted view (which functions the time is in, rather than under). It works the same on Linux and macOS, and Visual Studio's profiler, the Windows Performance Recorder, `perf` and Instruments work on the binary as well.

To profile an application instead of a scenario, build it with `--profile profiling` and record it the same way.

## Counting instead of timing

A timing moves by a few percent from one run to the next, more on a busy machine, so it cannot show that a change saved two percent. Counting what the processor executed can. `--steps` runs exactly that many steps, which makes a run the same work every time:

```sh
cargo build -p glassmere-bench --profile profiling --bin profile
valgrind --tool=callgrind --callgrind-out=relayout.out \
    target/profiling/profile form/relayout --steps 100
callgrind_annotate relayout.out | head -40
```

Callgrind (Linux and macOS, part of Valgrind) reports instructions executed, in total and for each function, and the totals of two runs of the same build agree to a fraction of a percent. Compare the total before and after a change; `callgrind_annotate --inclusive=yes` shows the cost of a function with everything it calls. It runs the program some fifty times slower, so keep `--steps` small.

`valgrind --tool=dhat` on the same command counts allocations and says where they are made: a step that allocates less is nearly always a step that costs less.

`--folded FILE` writes the sampled stacks themselves, one a line with its count, outermost frame first and each frame as `function@file:line`. That is the form flamegraph tools and [speedscope](https://www.speedscope.app) read, and a few lines of script over it give the time in each function alone, which a flamegraph only shows as the part of a box with nothing above it.

## Checking that nothing else changed

A scenario can describe what a step left: every node with its box, state and semantics, where the focus and the pointer are, the damage and the scene, or the pixels a renderer painted. `--fingerprint` runs a scenario and prints a hash of that after every step:

```sh
target/profiling/profile all --fingerprint > before.txt
# change something that should only make it cheaper
cargo build -p glassmere-bench --profile profiling --bin profile
target/profiling/profile all --fingerprint > after.txt
diff before.txt after.txt
```

No difference means that over those steps of those scenarios the change altered nothing an application, a renderer or a screen reader could see. A difference names the scenario; `--dump FILE` writes what its last step left in full, to compare, and a smaller `--steps` finds the first step that differs. Identities are written as places in the tree, so a change in how many nodes were made before one does not count.

It is a second check beside the tests, not in place of them: it covers what the scenarios do, and only that.

## Inside a running application

- The inspector (F12) shows the time of each phase of each frame and what repainted; [the performance model](performance.md) says what each of its tabs answers.
- `FrameStats`, returned by every frame, has the same numbers for a test or a log: time declaring, in layout, painting and shaping text, elements declared, nodes painted, cache hits.
- `GLASSMERE_STARTUP=1` prints how far into startup each step was reached.
- `tracing` spans named `glassmere.declare`, `glassmere.layout`, `glassmere.paint`, `glassmere.render` and `glassmere.frame` wrap the phases, for any subscriber that records spans.
- `crates/glassmere-allocations` counts allocations per batch with an allocator of its own.

## A way of working

1. Write the scenario, save a baseline and the fingerprints.
2. Take a flamegraph and find the widest box that should not be wide.
3. Change one thing.
4. Count the scenario again, or run its benchmark against the baseline, and the others to see nothing else moved.
5. Compare the fingerprints, and run the tests.
6. Keep the scenario.

The first use of this found that typing in the code editor read the whole document again on every keystroke to find its widest line: 8.5 ms a key in a 40,000-line file, almost all of it under one function in the flamegraph. Keeping the widest line up to date from the lines an edit changes brought it to 37 µs.
