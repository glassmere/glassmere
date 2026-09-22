# Milestone 21: background work

Implemented on 2026-09-22. The framework had a bounded mailbox and a wake handle, which is enough to hear from a worker but not to manage one. A user interface needs three things a channel does not give: a job that can be asked to stop, a newer job that supersedes the one before it, and the promise that a superseded job's results never reach the view. Without the third, search as you type is wrong: the answer to the query before last can arrive after the answer to the current one and overwrite it.

No executor is started and none is forced. A job is a thread, or a future on whatever runtime the application already runs.

```rust
let mut tasks = wake.tasks::<&str, Found>(256);
// Every keystroke supersedes the search the last one started.
tasks.spawn("search", move |out| {
    for record in records {
        if out.cancelled() {
            return;
        }
        let _ = out.send_blocking(Found::Hit(record));
    }
});
for (key, message) in tasks.drain(128) { .. }
```

| Piece | Contract |
| --- | --- |
| `Cancel` | Asks a job to stop, cooperatively: it sets a flag and the job decides how often to look. Cloneable and cheap to read. |
| `Tasks::<K, T>::start(key)` | The sending end for a new job, cancelling and forgetting whatever ran under that key. Hand it to a thread or to a future. `capacity` is shared by all jobs. |
| `Tasks::spawn(key, job)` | The same, on a thread of its own, for work that blocks rather than awaits. Returns the job's `Cancel`. |
| `Tasks::drain(limit)` | Up to `limit` results, each with the key of the job that sent it, in the order sent. Results from a cancelled or superseded job are dropped here rather than handed over, including ones sent before it was superseded. A partial drain asks for another UI turn, so a long stream does not stall the view. |
| `Tasks::cancel`, `cancel_all` | Stops listening at once, so a view stops caring before the job has noticed. |
| `Tasks::running(key)`, `running_keys()` | What is still going. A job counts as ended as soon as it drops its sender, which also wakes the window, so an indicator turns itself off with nothing sent. |
| `TaskSender::send` | Without waiting. A full mailbox hands the value back and the job decides what to do with it. |
| `TaskSender::send_blocking` | Waits for room, which is what a job streaming many results wants; the alternative is a retry loop in every application. Never on the UI thread, which is the thread that makes room. |
| Streams | Many sends per job is the whole of it. A job sending progress and results is two variants of one message type. |
| Async commands | A command handler calls `tasks.spawn`; there is no separate async command type, because a command that starts a job is a command that calls one function. |

`Mailbox`, `MessageSender` and the new `message_channel` moved from `rust-gui-winit` to `rust-gui-core`, so background work can be tested without a window and `Tasks` can be built on them. `WakeHandle::channel` is unchanged and `WakeHandle::tasks` is its companion; the paths through the facade are the same as before.

## Validation

Six tests in `rust-gui-core`:

- a superseded job cannot reach the view, including with a result already sent and waiting;
- cancelling stops the job, drops what it had sent, and is visible to the job at its next look;
- one job streams five results, a drain of two takes the first two and asks for another turn, and the rest arrive in order;
- a full mailbox hands the value back, and a worker using blocking sends through a mailbox of one gets all four results across to a view draining one at a time;
- a thread reports that it has ended, wakes the window by ending, and is forgotten once its results are drained;
- a cancelled thread stops at its next look.

The `tasks` example searches two hundred thousand records in a real window, streaming progress and hits, with a stop button and a superseding search per keystroke. Its smoke run prints what the window shows:

```
SEARCH Searching · 95% · 140 hits
SEARCH 140 hits
```

The last line is the job ending on its own and the view noticing, which is the wake on drop. The workspace suite is 291 tests; formatting and clippy with all features are clean.

## Remaining limits

- Cancellation is cooperative. A job that never looks at `cancelled()` runs to the end, and its results are dropped rather than never produced.
- A cancelled job still holds room in the shared mailbox until its results are drained and discarded, so a badly behaved job can crowd out a well behaved one until then.
- There is no `Future` to await a job from inside another one. The view drains; jobs do not wait on each other through this.
- `Tasks` is not a work queue: starting under a key that is busy replaces that job rather than queueing behind it.
