# What the API promises

An application is written against these crates' signatures. This says which of them are meant to be relied on, what may change, and what keeps a change from happening by accident.

## The surface

`crates/glassmere/tests/snapshots/public-api.snap` is the record of what an application can name in the crates it depends on. A test reads the sources, follows `pub mod` and `pub use` from each crate's root (an item in a private module counts only when something public re-exports it), and compares the result with that file. For each reachable item it records the signature with its generics and bounds, the type of every public field, the payload of every enum variant, the public methods and constants of its `impl` blocks, and the headers of the trait implementations that name it (`From`, `Default`, `Display` and the like). It also records the attributes that are part of the contract: `derive`, `repr`, `non_exhaustive`, `must_use`, `deprecated`, `track_caller`, and `cfg` gates such as `#[cfg(feature = "desktop")]`. Items the workspace's own macros generate, such as the `Style` setters and the `Dirty` flags, are expanded and recorded like any other. A rename, a removal, a changed type or an added parameter cannot slip through unnoticed. Accepting a change is deliberate:

```sh
UPDATE_SNAPSHOTS=1 cargo test -p glassmere --test api
```

The diff is then part of the change, to be read and explained. Anything that appears there is public; anything else is not, whatever its visibility says.

## Tiers

| Tier | What it covers | What may happen to it |
| --- | --- | --- |
| Application surface | `glassmere`: `Ui`, `Style`, `AppOptions`, the controls, the models, `Command`, `Clip`, `Tasks`, `Store`, `CustomWidget` and the types they take. | Additive changes at any time. A breaking one needs a minor version and an entry in the release notes, after a release that deprecates it. |
| Host surface | `Context`, `Event`, `FrameStats`, the renderers, `glassmere-winit`. What a host or a test harness calls, not an application. | The same promise, but it moves more often: this is where new platform work lands. |
| Internals | Anything not in the snapshot. Module paths, field order, private types. | Change without notice. An application that reaches one is not supported. |

## Versioning

The workspace is at `0.1`. Before `1.0` a breaking change raises the minor version, which is what Cargo treats as incompatible for a `0.x` crate, so `glassmere = "0.1"` will not pick one up. After `1.0` the usual semantic versioning applies.

Features are additive: turning one on adds items and never changes what the others do. The default set can gain a feature in a minor release; taking one out of the defaults is a breaking change.

## Deprecation

Nothing is removed without a release that still has it, marked `#[deprecated]` with the replacement named in the message. A deprecated item keeps working until the next minor version at the earliest, and the release notes say what to write instead.

That promise starts with `0.2`; nothing in `0.1` is deprecated.

## What the compiler already checks

Some mistakes never reach the API at all:

- `tr!` checks a message key and its arguments against the catalog while the crate compiles.
- `style_sheet!` parses a stylesheet with the framework's own parser while the crate compiles, so an unknown property is a build error that names the line.
- `#[derive(Properties)]` checks that every field has an editor for its type.
- Styles, lengths, alignments, colours and models are typed, so a property takes what it means and nothing else.

What is left is checked when a view is declared, and says so plainly: duplicate keys among siblings or dialogs, two enabled commands with one shortcut in a scope, a slider whose range is empty, grid columns without a width, list options out of range, a popover with nothing to anchor to, a property default of the wrong type, an unbalanced clip in a custom widget. A debug build panics with the message, because a view that declares them is wrong every frame rather than only sometimes. A release build leaves the offending widget out, keeps the rest of the view, and reports a `DiagnosticKind::Misuse` diagnostic, even with the other checks switched off. A memo that writes state panics in debug and drops the write in release; a memo derived from itself panics in both, since there is no value to fall back on. Model identities that are not unique are an error from the model's constructor.
