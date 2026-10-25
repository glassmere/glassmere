//! Optional reactive state: values that know what was derived from them.
//!
//! Ordinary Rust state stays the default; nothing in the framework asks for a
//! store. This is for a value derived from several others, expensive to
//! produce and read in several places, where a hand-written cache and its
//! invalidation rules would rot.
//!
//! A [`Signal`] holds a value. A [`Memo`] derives one and records what it read
//! while doing so. Changing a signal marks its dependents, transitively, and a
//! memo recomputes when next read, not when the change happened. The recorded
//! reads are also the dependency graph the inspector shows.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// The view declaring on this thread, if one is: whether a refused write
/// panics, and the messages of the writes it refused.
struct Declaration {
    panics: bool,
    refused: Vec<String>,
}

thread_local! {
    static DECLARATION: RefCell<Option<Declaration>> = const { RefCell::new(None) };
}

/// Runs a view's declaration with writes to signals refused, and returns
/// what they tried to write, for the context to report as misuses.
pub(crate) fn declaring<R>(panics: bool, declare: impl FnOnce() -> R) -> (R, Vec<String>) {
    /// Puts back the declaration that was running before this one, even when
    /// the view panics.
    struct Restore(Option<Declaration>);
    impl Drop for Restore {
        fn drop(&mut self) {
            DECLARATION.with(|declaration| *declaration.borrow_mut() = self.0.take());
        }
    }
    let outer = DECLARATION.with(|declaration| {
        declaration.replace(Some(Declaration {
            panics,
            refused: Vec::new(),
        }))
    });
    let restore = Restore(outer);
    let value = declare();
    let refused = DECLARATION.with(|declaration| {
        declaration
            .borrow_mut()
            .as_mut()
            .map(|declaration| std::mem::take(&mut declaration.refused))
            .unwrap_or_default()
    });
    drop(restore);
    (value, refused)
}

/// What a value in a [`Store`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StateKind {
    /// Held, and changed from outside.
    Signal,
    /// Derived, and recomputed when what it read has changed.
    Memo,
}

/// One value in the graph, as [`Store::graph`] reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct GraphNode {
    pub name: String,
    pub kind: StateKind,
    /// Bumps whenever the value changes, which for a memo means whenever it
    /// recomputes.
    pub version: u64,
    /// The store version at that change, so the newest is the largest.
    pub changed_at: u64,
    /// A memo whose inputs have moved and which has not been read since.
    pub dirty: bool,
    /// What it read when it last computed, as positions in this graph.
    pub inputs: Vec<usize>,
}

struct Slot {
    name: String,
    kind: StateKind,
    version: u64,
    changed_at: u64,
    dirty: bool,
    computing: bool,
    inputs: Vec<usize>,
    dependents: Vec<usize>,
}

#[derive(Default)]
struct Inner {
    slots: RefCell<Vec<Slot>>,
    /// The memos computing right now, innermost last, each collecting what
    /// it reads.
    tracking: RefCell<Vec<(usize, Vec<usize>)>>,
    version: Cell<u64>,
}

impl Inner {
    fn add(&self, name: String, kind: StateKind) -> usize {
        let mut slots = self.slots.borrow_mut();
        slots.push(Slot {
            name,
            kind,
            version: 0,
            changed_at: self.version.get(),
            // A memo has not computed yet, so the first read must.
            dirty: kind == StateKind::Memo,
            computing: false,
            inputs: Vec::new(),
            dependents: Vec::new(),
        });
        slots.len() - 1
    }

    /// Notes that the memo now computing read this value.
    fn record_read(&self, id: usize) {
        if let Some((_, reads)) = self.tracking.borrow_mut().last_mut()
            && !reads.contains(&id)
        {
            reads.push(id);
        }
    }

    /// A value changed: everything derived from it is out of date, however
    /// far down the chain.
    fn changed(&self, id: usize) {
        let version = self.version.get() + 1;
        self.version.set(version);
        let mut stack = {
            let mut slots = self.slots.borrow_mut();
            let slot = &mut slots[id];
            slot.version += 1;
            slot.changed_at = version;
            slot.dependents.clone()
        };
        while let Some(next) = stack.pop() {
            let mut slots = self.slots.borrow_mut();
            let slot = &mut slots[next];
            if slot.dirty {
                continue;
            }
            slot.dirty = true;
            stack.extend(slot.dependents.iter().copied());
        }
    }

    /// A memo that writes would make what others read depend on when they
    /// read it, so it is a mistake rather than a policy: a debug build
    /// panics, and a release build drops the write and logs it. A view that
    /// writes a signal is the same mistake, reported through its declaration.
    /// Returns whether the write may go ahead.
    fn may_write(&self, id: usize) -> bool {
        let computing = self.tracking.borrow().last().map(|(id, _)| *id);
        if let Some(computing) = computing {
            let name = self.slots.borrow()[computing].name.clone();
            let message =
                format!("`{name}` changed state while it was computing; a memo may only read");
            if cfg!(debug_assertions) {
                panic!("{message}");
            }
            tracing::error!(target: "glassmere::diagnostics", "{message}");
            return false;
        }
        self.declaration_allows(id)
    }

    /// Inside a declaration a write is refused: a debug build panics, and a
    /// release build drops it and hands the message to the declaration, which
    /// reports it as a misuse.
    fn declaration_allows(&self, id: usize) -> bool {
        DECLARATION.with(|declaration| {
            let mut declaration = declaration.borrow_mut();
            let Some(Declaration { panics, refused }) = declaration.as_mut() else {
                return true;
            };
            let name = self.slots.borrow()[id].name.clone();
            let message = format!(
                "`{name}` was written while a view was declaring; a view reads state and `update` writes it"
            );
            assert!(!*panics, "{message}");
            refused.push(message);
            false
        })
    }

    /// A cycle has no value to fall back on, so it panics in every build.
    fn begin(&self, id: usize) {
        {
            let mut slots = self.slots.borrow_mut();
            assert!(
                !slots[id].computing,
                "reactive cycle: `{}` is derived from itself",
                slots[id].name
            );
            slots[id].computing = true;
            // The reads are recorded again, so the old edges go.
            for input in std::mem::take(&mut slots[id].inputs) {
                slots[input].dependents.retain(|dependent| *dependent != id);
            }
        }
        self.tracking.borrow_mut().push((id, Vec::new()));
    }

    fn end(&self, id: usize) {
        let (_, inputs) = self.tracking.borrow_mut().pop().expect("a tracking frame");
        let mut slots = self.slots.borrow_mut();
        for input in &inputs {
            slots[*input].dependents.push(id);
        }
        let version = self.version.get();
        let slot = &mut slots[id];
        slot.inputs = inputs;
        slot.version += 1;
        slot.changed_at = version;
        slot.dirty = false;
        slot.computing = false;
    }

    fn dirty(&self, id: usize) -> bool {
        self.slots.borrow()[id].dirty
    }

    fn version_of(&self, id: usize) -> u64 {
        self.slots.borrow()[id].version
    }
}

/// Holds reactive values and the edges between them. Cloning shares it.
#[derive(Clone, Default)]
pub struct Store(Rc<Inner>);

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("version", &self.version())
            .finish_non_exhaustive()
    }
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    /// A held value. The name is what the inspector shows.
    pub fn signal<T>(&self, name: impl Into<String>, value: T) -> Signal<T> {
        Signal {
            id: self.0.add(name.into(), StateKind::Signal),
            store: self.0.clone(),
            value: Rc::new(RefCell::new(value)),
        }
    }

    /// A derived value. `compute` reads signals and other memos; what it
    /// reads is recorded as it runs, so a branch that reads nothing this
    /// time depends on nothing this time.
    pub fn memo<T>(
        &self,
        name: impl Into<String>,
        compute: impl FnMut() -> T + 'static,
    ) -> Memo<T> {
        Memo {
            id: self.0.add(name.into(), StateKind::Memo),
            store: self.0.clone(),
            state: Rc::new(RefCell::new(MemoState {
                value: None,
                compute: Box::new(compute),
            })),
        }
    }

    /// Bumps on every change to a signal, so a view can ask in one
    /// comparison whether anything at all has moved. It also passes straight
    /// to `Ui::memo` as the state of a subtree.
    pub fn version(&self) -> u64 {
        self.0.version.get()
    }

    /// Every value, in the order they were created, with what each one read
    /// when it last computed.
    pub fn graph(&self) -> Vec<GraphNode> {
        self.0
            .slots
            .borrow()
            .iter()
            .map(|slot| GraphNode {
                name: slot.name.clone(),
                kind: slot.kind,
                version: slot.version,
                changed_at: slot.changed_at,
                dirty: slot.dirty,
                inputs: slot.inputs.clone(),
            })
            .collect()
    }
}

/// A held value. Cloning it gives another handle on the same value.
pub struct Signal<T> {
    id: usize,
    store: Rc<Inner>,
    value: Rc<RefCell<T>>,
}

impl<T> std::fmt::Debug for Signal<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signal")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            store: self.store.clone(),
            value: self.value.clone(),
        }
    }
}

impl<T> Signal<T> {
    /// Reads without cloning, and records the read.
    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        self.store.record_read(self.id);
        read(&self.value.borrow())
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.with(T::clone)
    }

    /// Writing the value it already holds changes nothing and marks nothing,
    /// so a view that writes what it read every frame costs nothing.
    pub fn set(&self, value: T)
    where
        T: PartialEq,
    {
        if !self.store.may_write(self.id) || *self.value.borrow() == value {
            return;
        }
        *self.value.borrow_mut() = value;
        self.store.changed(self.id);
    }

    /// Changes in place, for a value too large to rebuild or not worth
    /// comparing. Always counts as a change.
    pub fn update(&self, change: impl FnOnce(&mut T)) {
        if !self.store.may_write(self.id) {
            return;
        }
        change(&mut self.value.borrow_mut());
        self.store.changed(self.id);
    }

    /// Bumps on every change. Hashable state for `Ui::memo`.
    pub fn version(&self) -> u64 {
        self.store.version_of(self.id)
    }
}

struct MemoState<T> {
    value: Option<T>,
    compute: Box<dyn FnMut() -> T>,
}

/// A derived value, recomputed when read after something it read has
/// changed. Cloning it gives another handle on the same value.
pub struct Memo<T> {
    id: usize,
    store: Rc<Inner>,
    state: Rc<RefCell<MemoState<T>>>,
}

impl<T> std::fmt::Debug for Memo<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Memo")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl<T> Clone for Memo<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            store: self.store.clone(),
            state: self.state.clone(),
        }
    }
}

impl<T> Memo<T> {
    /// Reads without cloning, computing first if it is out of date.
    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        self.refresh();
        self.store.record_read(self.id);
        read(
            self.state
                .borrow()
                .value
                .as_ref()
                .expect("a memo holds a value once it has computed"),
        )
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.with(T::clone)
    }

    /// Bumps whenever it recomputes, which is what makes it hashable state
    /// for `Ui::memo`. Computes first, so it answers about the value now.
    pub fn version(&self) -> u64 {
        self.refresh();
        self.store.version_of(self.id)
    }

    fn refresh(&self) {
        if !self.store.dirty(self.id) {
            return;
        }
        self.store.begin(self.id);
        let value = (self.state.borrow_mut().compute)();
        self.state.borrow_mut().value = Some(value);
        self.store.end(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store with a signal, a memo over it and a count of computations.
    fn counted() -> (Store, Signal<u32>, Memo<u32>, Rc<Cell<u32>>) {
        let store = Store::new();
        let signal = store.signal("count", 1u32);
        let runs = Rc::new(Cell::new(0));
        let (read, counter) = (signal.clone(), runs.clone());
        let memo = store.memo("doubled", move || {
            counter.set(counter.get() + 1);
            read.get() * 2
        });
        (store, signal, memo, runs)
    }

    #[test]
    fn a_memo_computes_once_until_what_it_read_changes() {
        let (_, signal, memo, runs) = counted();
        assert_eq!(runs.get(), 0, "nothing computes before it is read");
        assert_eq!(memo.get(), 2);
        assert_eq!(memo.get(), 2);
        assert_eq!(runs.get(), 1);
        signal.set(5);
        assert_eq!(runs.get(), 1, "a change marks; the read computes");
        assert_eq!(memo.get(), 10);
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn writing_the_same_value_changes_nothing() {
        let (store, signal, memo, runs) = counted();
        assert_eq!(memo.get(), 2);
        let version = store.version();
        signal.set(1);
        assert_eq!(store.version(), version);
        assert_eq!(memo.get(), 2);
        assert_eq!(runs.get(), 1);
        signal.update(|count| *count += 0);
        assert_ne!(store.version(), version, "an update always counts");
        assert_eq!(memo.get(), 2);
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn a_change_reaches_the_far_end_of_a_chain_and_a_shared_input_computes_once() {
        let store = Store::new();
        let first = store.signal("first", 2u32);
        let second = store.signal("second", 3u32);
        let runs = Rc::new(Cell::new(0));
        let (a, b, counter) = (first.clone(), second.clone(), runs.clone());
        let sum = store.memo("sum", move || {
            counter.set(counter.get() + 1);
            a.get() + b.get()
        });
        let (left, right) = (sum.clone(), sum.clone());
        let label = store.memo("label", move || format!("{}", left.get()));
        let doubled = store.memo("doubled", move || right.get() * 2);
        assert_eq!(label.get(), "5");
        assert_eq!(doubled.get(), 10);
        assert_eq!(
            runs.get(),
            1,
            "both read the same memo, which computed once"
        );
        first.set(10);
        assert_eq!(label.get(), "13");
        assert_eq!(doubled.get(), 26);
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn a_branch_that_was_not_taken_is_not_depended_on() {
        let store = Store::new();
        let use_left = store.signal("use_left", true);
        let left = store.signal("left", 1u32);
        let right = store.signal("right", 100u32);
        let (switch, a, b) = (use_left.clone(), left.clone(), right.clone());
        let runs = Rc::new(Cell::new(0));
        let counter = runs.clone();
        let chosen = store.memo("chosen", move || {
            counter.set(counter.get() + 1);
            if switch.get() { a.get() } else { b.get() }
        });
        assert_eq!(chosen.get(), 1);
        // The other side was not read, so changing it means nothing.
        right.set(200);
        assert_eq!(chosen.get(), 1);
        assert_eq!(runs.get(), 1);
        use_left.set(false);
        assert_eq!(chosen.get(), 200);
        // Now it is the left side that means nothing.
        left.set(50);
        assert_eq!(chosen.get(), 200);
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn the_graph_names_the_values_their_edges_and_what_moved_last() {
        let store = Store::new();
        let width = store.signal("width", 4u32);
        let height = store.signal("height", 5u32);
        let (a, b) = (width.clone(), height.clone());
        let area = store.memo("area", move || a.get() * b.get());
        let read = area.clone();
        let caption = store.memo("caption", move || format!("{} sq", read.get()));
        assert_eq!(caption.get(), "20 sq");
        let graph = store.graph();
        let names: Vec<&str> = graph.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["width", "height", "area", "caption"]);
        assert_eq!(graph[2].kind, StateKind::Memo);
        assert_eq!(graph[2].inputs, [0, 1], "area read both signals");
        assert_eq!(graph[3].inputs, [2], "the caption read the area");
        assert!(graph.iter().all(|node| !node.dirty));
        height.set(6);
        let graph = store.graph();
        assert_eq!(graph[1].changed_at, store.version());
        assert!(graph[2].dirty && graph[3].dirty, "both are out of date");
        assert_eq!(caption.get(), "24 sq");
        assert!(store.graph().iter().all(|node| !node.dirty));
    }

    #[test]
    #[should_panic(expected = "reactive cycle: `loop` is derived from itself")]
    fn a_memo_that_reads_itself_says_so() {
        let store = Store::new();
        let hold: Rc<RefCell<Option<Memo<u32>>>> = Rc::new(RefCell::new(None));
        let read = hold.clone();
        let looping = store.memo("loop", move || {
            read.borrow()
                .as_ref()
                .map_or(0, |memo: &Memo<u32>| memo.get())
                + 1
        });
        *hold.borrow_mut() = Some(looping.clone());
        looping.get();
    }

    #[test]
    #[should_panic(expected = "`writer` changed state while it was computing")]
    fn a_memo_that_writes_says_so() {
        let store = Store::new();
        let signal = store.signal("count", 1u32);
        let write = signal.clone();
        let writer = store.memo("writer", move || {
            write.set(2);
            0u32
        });
        writer.get();
    }
}
