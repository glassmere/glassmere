# Milestone 27: what holds still

Implemented on 2026-09-22. Three coverage rows were about keeping promises rather than adding capability: the same behaviour on three platforms, mistakes caught before an application runs, and an API an application can be written against.

## The three platforms

The continuous build already ran formatting, the whole test suite with every feature, and clippy on Windows, Linux and macOS. Two things were missing and are now there:

- The Linux job installs what the desktop integration of [milestone 26](milestone-26.md) needs: GTK and the app indicator for a tray icon, and D-Bus for notifications. Without them the Linux build would have started failing the moment that feature existed.
- Every platform now opens a real window and runs a frame in it, not only builds one. Linux has no display in the build, so Xvfb provides one and the CPU renderer paints into it, which exercises the whole host path: window, surface, input plumbing, presentation.

**Not verified here:** the build itself. This machine is Windows, and the Linux and macOS jobs have not been run since these changes; what they do is written down, not observed. Windows remains the platform checked by hand, with the interactive scripts.

## Mistakes the compiler catches

```rust
let sheet: StyleSheet = style_sheet!("styles/app.style");
```

`style_sheet!` reads the file while the crate compiles and parses it with the framework's own parser, so a stylesheet that would have failed at start fails the build instead, naming the line:

```
error: examples/theming.style:33: unknown property `colour`
  --> crates\seaglass\tests\compile_time.rs:13:29
   |
13 |     let sheet: StyleSheet = style_sheet!("examples/theming.style");
```

It expands to the same parse, over text included from that path, so editing the file rebuilds the crate and the result cannot fail at start. There is no second parser to drift: the macro crate uses the real one.

That joins what was already checked while compiling: `tr!` for a message key and its arguments, `#[derive(Properties)]` for a field's editor, and the typed styles, lengths, alignments and models. What is left is checked at declaration and says so plainly, which [the stability note](api-stability.md) lists.

## The API, written down

`crates/seaglass/tests/snapshots/public-api.snap` records every public item and every public method of the ten crates an application depends on, with its signature: 1,800 lines read out of the sources themselves. A test compares the sources with that file, so a rename, a removal or an added parameter cannot pass unnoticed; accepting one is a deliberate `UPDATE_SNAPSHOTS=1` and a diff to read.

[The stability note](api-stability.md) says what the tiers are, what a version number means before and after 1.0, and how something is deprecated before it is taken away.

## Validation

The public surface snapshot is itself the test. The compile-time stylesheet has one that reads a class out of a sheet parsed while compiling, and the error above was produced by adding a bad property to a real file and building. The workspace suite is 320 tests; formatting and clippy with all features are clean.

## Remaining limits

- The surface snapshot is read from the sources, so it does not know about `cfg`: an item behind a feature is recorded once, whatever that feature is.
- It records signatures, not semantics. A function that keeps its shape and changes its meaning passes.
- `style_sheet!` checks the file it is given, not the one an application loads at start: `AppOptions::stylesheet` still reads a path, because that is what makes it reload while the window is open.
- The version is 0.1. The promise is that changes are deliberate and written down, not that nothing will change.
