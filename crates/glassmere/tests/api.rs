//! The public surface, written down.
//!
//! An application's code is written against these signatures, so changing
//! one is a decision rather than an accident. The snapshot is the record:
//! any change to what the crates export shows up as a diff in
//! `tests/snapshots/public-api.snap`, to be accepted deliberately with
//! `UPDATE_SNAPSHOTS=1` and explained in the release notes.
//!
//! What is recorded is what an application can name, found by reading the
//! sources: starting at each crate's root, the test follows `pub mod` and
//! `pub use` (globs and renames included), so an item in a private module
//! counts only when something public re-exports it. For each item it
//! records:
//!
//! - the signature, with generics and bounds;
//! - the type of every public field, and the payload of every enum variant;
//! - the attributes that are part of the contract: `derive`, `repr`,
//!   `non_exhaustive`, `must_use`, `deprecated`, `track_caller`, the proc
//!   macro attributes, `doc(hidden)` and `cfg`/`cfg_attr` (a `cfg` on a
//!   module is inherited by the items in it);
//! - the public methods and associated constants of inherent `impl` blocks,
//!   with the generics of the block when it has any;
//! - the trait `impl` blocks that mention a public item, as their headers;
//! - the items the crates' own macros generate (`setter!`, `from_text!`,
//!   `number_arg!`, `bitflags!`), expanded by the small expanders below, one
//!   per macro.
//!
//! Bodies, documentation and behaviour are not recorded, and neither is
//! anything a macro from outside this workspace generates, other than
//! `bitflags!`.

use glassmere_testing::assert_snapshot;
use proc_macro2::{Delimiter, Group, Spacing, TokenStream, TokenTree};
use quote::{ToTokens, quote};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt::Write as _,
    path::{Path, PathBuf},
};
use syn::{
    Attribute, ImplItem, Item, Type, UseTree, Visibility,
    parse::{ParseStream, Parser as _},
};

const SNAPSHOTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots");

/// The crates an application writes against, in the order it meets them.
const CRATES: [&str; 15] = [
    "glassmere",
    "glassmere-core",
    "glassmere-scene",
    "glassmere-text",
    "glassmere-render-wgpu",
    "glassmere-render-cpu",
    "glassmere-winit",
    "glassmere-testing",
    "glassmere-devtools",
    "glassmere-image",
    "glassmere-win32",
    "glassmere-derive",
    "glassmere-fluent",
    "glassmere-icu",
    "glassmere-syntax",
];

#[test]
fn the_public_surface_is_what_was_recorded() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates directory")
        .to_path_buf();
    let mut text = String::new();
    for name in CRATES {
        let krate = Crate::load(&root.join(name).join("src/lib.rs"));
        let mut found = krate.surface();
        found.sort();
        found.dedup();
        for line in found {
            let _ = writeln!(text, "{name} {line}");
        }
    }
    assert_snapshot(SNAPSHOTS, "public-api", &text);
}

/// A definition: the module it is in and its index among that module's items.
type Def = (usize, usize);

/// What a name can stand for.
#[derive(Clone, Copy)]
enum Target {
    Def(Def),
    Module(usize),
}

struct Module {
    parent: Option<usize>,
    /// The `cfg` attributes of this module and the ones around it.
    gates: Vec<String>,
    /// The items, with the macro invocations among them already expanded.
    items: Vec<Item>,
    children: BTreeMap<String, usize>,
}

struct Crate {
    modules: Vec<Module>,
    /// The arms of the `macro_rules!` definitions seen so far, as matcher and
    /// transcriber.
    macros: HashMap<String, Vec<(TokenStream, TokenStream)>>,
}

/// Where an application can reach things, found while walking from the root.
#[derive(Default)]
struct Surface {
    paths: BTreeMap<Def, BTreeSet<String>>,
    lines: Vec<String>,
    walked: HashSet<(usize, String)>,
}

fn public(visibility: &Visibility) -> bool {
    matches!(visibility, Visibility::Public(_))
}

// Reading the sources.

impl Crate {
    fn load(entry: &Path) -> Self {
        let mut krate = Self {
            modules: Vec::new(),
            macros: HashMap::new(),
        };
        krate.file_module(entry, None, Vec::new());
        krate
    }

    fn file_module(&mut self, path: &Path, parent: Option<usize>, gates: Vec<String>) -> usize {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("{} could not be read: {error}", path.display()));
        let file = syn::parse_file(&source)
            .unwrap_or_else(|error| panic!("{} could not be parsed: {error}", path.display()));
        // `lib.rs` and `mod.rs` keep their modules beside them; any other
        // `x.rs` keeps them in the directory `x/`.
        let beside = path.parent().unwrap_or(Path::new("."));
        let directory = match path.file_stem().and_then(|stem| stem.to_str()) {
            Some("lib" | "mod" | "main") | None => beside.to_path_buf(),
            Some(stem) => beside.join(stem),
        };
        self.module(file.items, parent, gates, &directory)
    }

    fn module(
        &mut self,
        items: Vec<Item>,
        parent: Option<usize>,
        gates: Vec<String>,
        directory: &Path,
    ) -> usize {
        let id = self.modules.len();
        self.modules.push(Module {
            parent,
            gates: gates.clone(),
            items: Vec::new(),
            children: BTreeMap::new(),
        });
        let mut kept = Vec::new();
        for item in items {
            if is_test(item_attributes(&item)) {
                continue;
            }
            match item {
                Item::Mod(declared) => {
                    let name = declared.ident.to_string();
                    let mut inner = gates.clone();
                    inner.extend(gate_attributes(&declared.attrs));
                    let child = match &declared.content {
                        Some((_, inside)) => {
                            self.module(inside.clone(), Some(id), inner, &directory.join(&name))
                        }
                        None => {
                            let beside = directory.join(format!("{name}.rs"));
                            let below = directory.join(&name).join("mod.rs");
                            let file = if beside.exists() { beside } else { below };
                            self.file_module(&file, Some(id), inner)
                        }
                    };
                    self.modules[id].children.insert(name, child);
                    kept.push(Item::Mod(declared));
                }
                Item::Macro(invocation) => kept.extend(self.expand_item(invocation)),
                Item::Impl(block) => kept.push(Item::Impl(self.expand_impl(block))),
                other => kept.push(other),
            }
        }
        self.modules[id].items = kept;
        id
    }

    /// A macro invocation in item position: a definition is remembered, a
    /// known macro is replaced by what it generates.
    fn expand_item(&mut self, invocation: syn::ItemMacro) -> Vec<Item> {
        let name = last_segment(&invocation.mac.path);
        let tokens = invocation.mac.tokens;
        if name == "macro_rules" {
            if let Some(defined) = invocation.ident {
                self.macros.insert(defined.to_string(), macro_arms(tokens));
            }
            return Vec::new();
        }
        match name.as_str() {
            "bitflags" => bitflags(tokens),
            "thread_local" => {
                let public_static = flatten(tokens)
                    .iter()
                    .any(|tree| matches!(tree, TokenTree::Ident(word) if word == "pub"));
                assert!(
                    !public_static,
                    "`thread_local!` declares a public item, which tests/api.rs does not record"
                );
                Vec::new()
            }
            _ => {
                syn::parse2::<syn::File>(self.expand(&name, tokens))
                    .unwrap_or_else(|error| {
                        panic!("the expansion of `{name}!` does not parse: {error}")
                    })
                    .items
            }
        }
    }

    fn expand_impl(&self, mut block: syn::ItemImpl) -> syn::ItemImpl {
        let mut items = Vec::new();
        for inside in block.items {
            match inside {
                ImplItem::Macro(invocation) => {
                    let name = last_segment(&invocation.mac.path);
                    let expanded = self.expand(&name, invocation.mac.tokens);
                    let parse = |input: ParseStream| {
                        let mut parsed = Vec::new();
                        while !input.is_empty() {
                            parsed.push(input.parse::<ImplItem>()?);
                        }
                        Ok(parsed)
                    };
                    items.extend(parse.parse2(expanded).unwrap_or_else(|error| {
                        panic!("the expansion of `{name}!` does not parse: {error}")
                    }));
                }
                other => items.push(other),
            }
        }
        block.items = items;
        block
    }

    /// What one of the workspace's own macros generates for these arguments.
    /// Each has its own way of reading its arguments; the generated code is
    /// the macro's own transcriber, so a change to a macro's output is seen.
    fn expand(&self, name: &str, args: TokenStream) -> TokenStream {
        let arms = || {
            self.macros
                .get(name)
                .unwrap_or_else(|| panic!("`{name}!` is not defined before its use"))
        };
        match name {
            // `setter!(name, field, Type)` and `setter!(name, field, Type, some)`:
            // the arm whose literal tokens match.
            "setter" => {
                let args = split_commas(args);
                arms()
                    .iter()
                    .find_map(|(matcher, body)| {
                        let bindings = bind_list(matcher, &args)?;
                        Some(substitute(body.clone(), &bindings, None))
                    })
                    .expect("an arm of `setter!` matches the invocation")
            }
            // `number_arg!(T, U, ...)`: the transcriber repeated for each type.
            "number_arg" => {
                let types = Binding::Many(split_commas(args));
                let bindings = HashMap::from([("type".to_string(), types)]);
                substitute(arms()[0].1.clone(), &bindings, None)
            }
            // `from_text!(Type)` and `from_text!(Type<M>)`.
            "from_text" => {
                let mut words = flatten(args).into_iter().filter_map(|tree| match tree {
                    TokenTree::Ident(word) => Some(word),
                    _ => None,
                });
                let mut bindings = HashMap::new();
                let word = words.next().expect("`from_text!` names a type");
                bindings.insert("type".to_string(), Binding::One(quote!(#word)));
                if let Some(message) = words.next() {
                    bindings.insert("message".to_string(), Binding::One(quote!(#message)));
                }
                substitute(arms()[0].1.clone(), &bindings, None)
            }
            _ => panic!(
                "`{name}!` generates items, and tests/api.rs has no expander for it; add one"
            ),
        }
    }
}

enum Binding {
    One(TokenStream),
    Many(Vec<TokenStream>),
}

/// The arms of a `macro_rules!` body: `(matcher) => { transcriber };` ...
fn macro_arms(tokens: TokenStream) -> Vec<(TokenStream, TokenStream)> {
    let mut arms = Vec::new();
    let mut trees = tokens.into_iter().peekable();
    while let Some(tree) = trees.next() {
        let TokenTree::Group(matcher) = tree else {
            panic!("unexpected token in a macro_rules body");
        };
        // The `=>`.
        trees.next();
        trees.next();
        let Some(TokenTree::Group(body)) = trees.next() else {
            panic!("a macro_rules arm without a body");
        };
        arms.push((matcher.stream(), body.stream()));
        if matches!(trees.peek(), Some(TokenTree::Punct(p)) if p.as_char() == ';') {
            trees.next();
        }
    }
    arms
}

/// The top-level comma-separated parts of a token stream, angle brackets
/// kept together.
fn split_commas(stream: TokenStream) -> Vec<TokenStream> {
    let mut parts = vec![TokenStream::new()];
    let mut angle = 0_usize;
    for tree in stream {
        if let TokenTree::Punct(p) = &tree {
            match p.as_char() {
                '<' => angle += 1,
                '>' => angle = angle.saturating_sub(1),
                ',' if angle == 0 => {
                    parts.push(TokenStream::new());
                    continue;
                }
                _ => {}
            }
        }
        parts.last_mut().expect("a part").extend([tree]);
    }
    parts.retain(|part| !part.is_empty());
    parts
}

/// Binds the `$name:fragment` parts of a comma-separated matcher to the
/// arguments; `None` when a literal part differs or the counts do not agree.
fn bind_list(matcher: &TokenStream, args: &[TokenStream]) -> Option<HashMap<String, Binding>> {
    let parts = split_commas(matcher.clone());
    if parts.len() != args.len() {
        return None;
    }
    let mut bindings = HashMap::new();
    for (part, arg) in parts.iter().zip(args) {
        let mut trees = part.clone().into_iter();
        match trees.next() {
            Some(TokenTree::Punct(p)) if p.as_char() == '$' => {
                let Some(TokenTree::Ident(name)) = trees.next() else {
                    return None;
                };
                bindings.insert(name.to_string(), Binding::One(arg.clone()));
            }
            _ if part.to_string() == arg.to_string() => {}
            _ => return None,
        }
    }
    Some(bindings)
}

/// A transcriber with its `$name` variables and `$( ... ) sep op`
/// repetitions filled in. `index` is the current round of an enclosing `*`.
fn substitute(
    stream: TokenStream,
    bindings: &HashMap<String, Binding>,
    index: Option<usize>,
) -> TokenStream {
    let mut out = TokenStream::new();
    let mut trees = stream.into_iter().peekable();
    while let Some(tree) = trees.next() {
        match tree {
            TokenTree::Punct(p) if p.as_char() == '$' => match trees.next() {
                Some(TokenTree::Ident(name)) => match bindings.get(&name.to_string()) {
                    Some(Binding::One(tokens)) => out.extend(tokens.clone()),
                    Some(Binding::Many(all)) => {
                        let round = index.expect("a repeated variable outside a repetition");
                        out.extend(all[round].clone());
                    }
                    None => panic!("`${name}` is not bound"),
                },
                Some(TokenTree::Group(repeated)) => {
                    let mut separator = None;
                    let mut operator = trees.next();
                    let is_operator = |tree: &Option<TokenTree>| matches!(tree, Some(TokenTree::Punct(p)) if matches!(p.as_char(), '?' | '*' | '+'));
                    if !is_operator(&operator) {
                        separator = operator.take();
                        operator = trees.next();
                    }
                    let Some(TokenTree::Punct(operator)) = operator else {
                        panic!("a repetition without an operator");
                    };
                    let names = variables(repeated.stream());
                    if operator.as_char() == '?' {
                        let present = !names.is_empty()
                            && names
                                .iter()
                                .all(|name| matches!(bindings.get(name), Some(Binding::One(_))));
                        if present {
                            out.extend(substitute(repeated.stream(), bindings, index));
                        }
                    } else {
                        let rounds = names
                            .iter()
                            .filter_map(|name| match bindings.get(name) {
                                Some(Binding::Many(all)) => Some(all.len()),
                                _ => None,
                            })
                            .max()
                            .expect("a repetition over a repeated variable");
                        for round in 0..rounds {
                            if round > 0 {
                                out.extend(separator.clone());
                            }
                            out.extend(substitute(repeated.stream(), bindings, Some(round)));
                        }
                    }
                }
                _ => panic!("a stray `$` in a macro transcriber"),
            },
            TokenTree::Group(group) => {
                let inner = substitute(group.stream(), bindings, index);
                out.extend([TokenTree::Group(Group::new(group.delimiter(), inner))]);
            }
            other => out.extend([other]),
        }
    }
    out
}

/// The `$name` variables mentioned in a stream.
fn variables(stream: TokenStream) -> Vec<String> {
    let trees = flatten(stream);
    let mut names = Vec::new();
    for pair in trees.windows(2) {
        if let (TokenTree::Punct(p), TokenTree::Ident(name)) = (&pair[0], &pair[1])
            && p.as_char() == '$'
        {
            names.push(name.to_string());
        }
    }
    names
}

/// `bitflags! { pub struct Name: u8 { const A = 1; ... } }` as the struct,
/// the associated constants, the methods and the trait impls the macro
/// generates for it: the API of the `bitflags` 2.x line, with `const`
/// qualifiers and bodies left out.
fn bitflags(tokens: TokenStream) -> Vec<Item> {
    let parse = |input: ParseStream| {
        let mut generated = String::new();
        while !input.is_empty() {
            let attributes = input.call(Attribute::parse_outer)?;
            let visibility: Visibility = input.parse()?;
            input.parse::<syn::Token![struct]>()?;
            let name: syn::Ident = input.parse()?;
            input.parse::<syn::Token![:]>()?;
            let bits: Type = input.parse()?;
            let body;
            syn::braced!(body in input);
            let mut flags = Vec::new();
            while !body.is_empty() {
                body.call(Attribute::parse_outer)?;
                body.parse::<syn::Token![const]>()?;
                flags.push(body.parse::<syn::Ident>()?);
                body.parse::<syn::Token![=]>()?;
                body.parse::<syn::Expr>()?;
                body.parse::<syn::Token![;]>()?;
            }
            let bits = quote!(#bits).to_string();
            let _ = writeln!(
                generated,
                "{} {} struct {name}({bits});",
                quote!(#(#attributes)*),
                quote!(#visibility)
            );
            let _ = writeln!(generated, "impl {name} {{");
            for flag in &flags {
                let _ = writeln!(generated, "pub const {flag}: {name} = {name}(0);");
            }
            for method in [
                "fn empty() -> Self",
                "fn all() -> Self",
                "fn bits(&self) -> BITS",
                "fn from_bits(bits: BITS) -> Option<Self>",
                "fn from_bits_truncate(bits: BITS) -> Self",
                "fn from_bits_retain(bits: BITS) -> Self",
                "fn from_name(name: &str) -> Option<Self>",
                "fn is_empty(&self) -> bool",
                "fn is_all(&self) -> bool",
                "fn intersects(&self, other: Self) -> bool",
                "fn contains(&self, other: Self) -> bool",
                "fn insert(&mut self, other: Self)",
                "fn remove(&mut self, other: Self)",
                "fn toggle(&mut self, other: Self)",
                "fn set(&mut self, other: Self, value: bool)",
                "fn intersection(self, other: Self) -> Self",
                "fn union(self, other: Self) -> Self",
                "fn difference(self, other: Self) -> Self",
                "fn symmetric_difference(self, other: Self) -> Self",
                "fn complement(self) -> Self",
                "fn iter(&self) -> bitflags::iter::Iter<Self>",
                "fn iter_names(&self) -> bitflags::iter::IterNames<Self>",
            ] {
                let _ = writeln!(
                    generated,
                    "pub {} {{ loop {{}} }}",
                    method.replace("BITS", &bits)
                );
            }
            let _ = writeln!(generated, "}}");
            for implemented in [
                "::core::fmt::Binary",
                "::core::fmt::LowerHex",
                "::core::fmt::Octal",
                "::core::fmt::UpperHex",
                "::core::ops::BitAnd",
                "::core::ops::BitAndAssign",
                "::core::ops::BitOr",
                "::core::ops::BitOrAssign",
                "::core::ops::BitXor",
                "::core::ops::BitXorAssign",
                "::core::ops::Not",
                "::core::ops::Sub",
                "::core::ops::SubAssign",
                "::core::iter::IntoIterator",
                "bitflags::Flags",
            ] {
                let _ = writeln!(generated, "impl {implemented} for {name} {{}}");
            }
            for implemented in ["::core::iter::Extend", "::core::iter::FromIterator"] {
                let _ = writeln!(generated, "impl {implemented}<{name}> for {name} {{}}");
            }
        }
        Ok(generated)
    };
    let generated = parse
        .parse2(tokens)
        .unwrap_or_else(|error| panic!("a `bitflags!` invocation did not parse: {error}"));
    syn::parse_str::<syn::File>(&generated)
        .unwrap_or_else(|error| panic!("the expansion of `bitflags!` does not parse: {error}"))
        .items
}

// Walking from the root.

impl Crate {
    /// Every line of the crate's public surface, unsorted.
    fn surface(&self) -> Vec<String> {
        let mut surface = Surface::default();
        self.expose(0, "", &mut surface);
        let primary: BTreeMap<Def, String> = surface
            .paths
            .iter()
            .map(|(def, paths)| {
                let shortest = paths
                    .iter()
                    .min_by_key(|path| (path.matches("::").count(), (*path).clone()))
                    .expect("a path");
                (*def, shortest.clone())
            })
            .collect();
        let mut lines = std::mem::take(&mut surface.lines);
        for (def, paths) in &surface.paths {
            let name = &primary[def];
            self.describe(*def, name, &mut lines);
            for other in paths.iter().filter(|path| *path != name) {
                lines.push(format!("alias {other} = {name}"));
            }
        }
        self.describe_impls(&primary, &mut lines);
        lines
    }

    /// The public names of a module, as seen under `prefix`.
    fn expose(&self, module: usize, prefix: &str, surface: &mut Surface) {
        if !surface.walked.insert((module, prefix.to_string())) {
            return;
        }
        let at = |name: &str| {
            if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}::{name}")
            }
        };
        let gates = &self.modules[module].gates;
        for (index, item) in self.modules[module].items.iter().enumerate() {
            match item {
                Item::Use(declared) if public(&declared.vis) => {
                    for leaf in leaves(&declared.tree) {
                        let exposed = at(leaf.name.as_deref().unwrap_or("*"));
                        surface.lines.push(format!(
                            "use {exposed} = {}{}",
                            leaf.source,
                            attributes(&declared.attrs, gates)
                        ));
                        for target in self.find(module, &leaf.path) {
                            match (target, &leaf.name) {
                                (Target::Def(def), Some(_)) => {
                                    surface
                                        .paths
                                        .entry(def)
                                        .or_default()
                                        .insert(exposed.clone());
                                }
                                (Target::Module(inner), Some(_)) => {
                                    self.expose(inner, &exposed, surface);
                                }
                                (Target::Module(inner), None) => {
                                    self.expose(inner, prefix, surface);
                                }
                                (Target::Def(_), None) => {}
                            }
                        }
                    }
                }
                Item::Mod(declared) if public(&declared.vis) => {
                    let name = declared.ident.to_string();
                    let path = at(&name);
                    surface
                        .lines
                        .push(format!("mod {path}{}", attributes(&declared.attrs, gates)));
                    let child = self.modules[module].children[&name];
                    self.expose(child, &path, surface);
                }
                other => {
                    if let Some((name, visibility)) = defined(other)
                        && public(visibility)
                        && name != "_"
                    {
                        surface
                            .paths
                            .entry((module, index))
                            .or_default()
                            .insert(at(&name));
                    }
                }
            }
        }
    }

    /// What a path written in `module` stands for. A path into another crate
    /// stands for nothing here.
    fn find(&self, module: usize, path: &[String]) -> Vec<Target> {
        self.resolve(module, path, &mut Vec::new())
    }

    fn resolve(
        &self,
        module: usize,
        path: &[String],
        active: &mut Vec<(usize, String)>,
    ) -> Vec<Target> {
        let Some((first, rest)) = path.split_first() else {
            return Vec::new();
        };
        let mut current = match first.as_str() {
            "crate" => vec![Target::Module(0)],
            "self" => vec![Target::Module(module)],
            "super" => self.modules[module]
                .parent
                .map(Target::Module)
                .into_iter()
                .collect(),
            name => self.lookup(module, name, active),
        };
        for segment in rest {
            let mut next = Vec::new();
            for target in &current {
                if let Target::Module(inner) = target {
                    if segment == "super" {
                        next.extend(self.modules[*inner].parent.map(Target::Module));
                    } else {
                        next.extend(self.lookup(*inner, segment, active));
                    }
                }
            }
            current = next;
        }
        current
    }

    /// A name declared or imported in a module, whatever its visibility.
    fn lookup(&self, module: usize, name: &str, active: &mut Vec<(usize, String)>) -> Vec<Target> {
        let key = (module, name.to_string());
        if active.contains(&key) {
            return Vec::new();
        }
        active.push(key);
        let mut found = Vec::new();
        if let Some(child) = self.modules[module].children.get(name) {
            found.push(Target::Module(*child));
        }
        for (index, item) in self.modules[module].items.iter().enumerate() {
            if let Item::Use(declared) = item {
                for leaf in leaves(&declared.tree) {
                    match &leaf.name {
                        Some(exposed) if exposed == name => {
                            found.extend(self.resolve(module, &leaf.path, active));
                        }
                        None => {
                            for target in self.resolve(module, &leaf.path, active) {
                                if let Target::Module(inner) = target {
                                    found.extend(self.lookup(inner, name, active));
                                }
                            }
                        }
                        Some(_) => {}
                    }
                }
            } else if defined(item).is_some_and(|(declared, _)| declared == name) {
                found.push(Target::Def((module, index)));
            }
        }
        active.pop();
        found
    }
}

/// One name a `use` brings in: the path it comes from, the name it gives
/// (`None` for a glob) and the source as written.
struct Leaf {
    path: Vec<String>,
    name: Option<String>,
    source: String,
}

fn leaves(tree: &UseTree) -> Vec<Leaf> {
    fn walk(tree: &UseTree, prefix: &mut Vec<String>, out: &mut Vec<Leaf>) {
        let mut leaf = |path: Vec<String>, name: Option<String>| {
            let mut source = path.join("::");
            if name.is_none() {
                source.push_str("::*");
            }
            out.push(Leaf { path, name, source });
        };
        match tree {
            UseTree::Path(inner) => {
                prefix.push(inner.ident.to_string());
                walk(&inner.tree, prefix, out);
                prefix.pop();
            }
            UseTree::Name(name) => {
                let mut path = prefix.clone();
                if name.ident != "self" {
                    path.push(name.ident.to_string());
                }
                let exposed = path.last().cloned();
                leaf(path, exposed);
            }
            UseTree::Rename(rename) => {
                let mut path = prefix.clone();
                if rename.ident != "self" {
                    path.push(rename.ident.to_string());
                }
                leaf(path, Some(rename.rename.to_string()));
            }
            UseTree::Glob(_) => leaf(prefix.clone(), None),
            UseTree::Group(group) => {
                for inner in &group.items {
                    walk(inner, prefix, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(tree, &mut Vec::new(), &mut out);
    out
}

/// The name and visibility of an item that declares one.
fn defined(item: &Item) -> Option<(String, &Visibility)> {
    match item {
        Item::Struct(declared) => Some((declared.ident.to_string(), &declared.vis)),
        Item::Enum(declared) => Some((declared.ident.to_string(), &declared.vis)),
        Item::Union(declared) => Some((declared.ident.to_string(), &declared.vis)),
        Item::Trait(declared) => Some((declared.ident.to_string(), &declared.vis)),
        Item::Type(declared) => Some((declared.ident.to_string(), &declared.vis)),
        Item::Const(declared) => Some((declared.ident.to_string(), &declared.vis)),
        Item::Static(declared) => Some((declared.ident.to_string(), &declared.vis)),
        Item::Fn(declared) => Some((declared.sig.ident.to_string(), &declared.vis)),
        _ => None,
    }
}

fn item_attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(inner) => &inner.attrs,
        Item::Enum(inner) => &inner.attrs,
        Item::Fn(inner) => &inner.attrs,
        Item::Impl(inner) => &inner.attrs,
        Item::Macro(inner) => &inner.attrs,
        Item::Mod(inner) => &inner.attrs,
        Item::Static(inner) => &inner.attrs,
        Item::Struct(inner) => &inner.attrs,
        Item::Trait(inner) => &inner.attrs,
        Item::Type(inner) => &inner.attrs,
        Item::Union(inner) => &inner.attrs,
        Item::Use(inner) => &inner.attrs,
        _ => &[],
    }
}

fn last_segment(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_default()
}

// Writing the lines.

impl Crate {
    /// One definition, under the name an application reaches it by.
    fn describe(&self, def: Def, name: &str, out: &mut Vec<String>) {
        let module = &self.modules[def.0];
        let gates = &module.gates;
        match &module.items[def.1] {
            Item::Struct(declared) => {
                let mut private = false;
                let mut fields = Vec::new();
                for (position, field) in declared.fields.iter().enumerate() {
                    if !public(&field.vis) {
                        private = true;
                        continue;
                    }
                    let label = field
                        .ident
                        .as_ref()
                        .map_or_else(|| position.to_string(), ToString::to_string);
                    fields.push(format!(
                        "  field {name}::{label}: {}{}",
                        one_line(&field.ty),
                        attributes(&field.attrs, &[])
                    ));
                }
                out.push(format!(
                    "struct {name}{}{}{}",
                    generics(&declared.generics),
                    attributes(&declared.attrs, gates),
                    if private { " (private fields)" } else { "" }
                ));
                out.extend(fields);
            }
            Item::Enum(declared) => {
                out.push(format!(
                    "enum {name}{}{}",
                    generics(&declared.generics),
                    attributes(&declared.attrs, gates)
                ));
                for variant in &declared.variants {
                    let mut fields = variant.fields.clone();
                    for field in &mut fields {
                        field.attrs.clear();
                    }
                    let payload = match &fields {
                        syn::Fields::Unit => String::new(),
                        syn::Fields::Unnamed(fields) => one_line(fields),
                        syn::Fields::Named(fields) => format!(" {}", one_line(fields)),
                    };
                    let discriminant = variant
                        .discriminant
                        .as_ref()
                        .map(|(_, value)| format!(" = {}", one_line(value)))
                        .unwrap_or_default();
                    out.push(format!(
                        "  variant {name}::{}{payload}{discriminant}{}",
                        variant.ident,
                        attributes(&variant.attrs, &[])
                    ));
                }
            }
            Item::Trait(declared) => {
                let supertraits = if declared.supertraits.is_empty() {
                    String::new()
                } else {
                    format!(": {}", one_line(&declared.supertraits))
                };
                out.push(format!(
                    "{}trait {name}{}{supertraits}{}",
                    if declared.unsafety.is_some() {
                        "unsafe "
                    } else {
                        ""
                    },
                    generics(&declared.generics),
                    attributes(&declared.attrs, gates)
                ));
                for inside in &declared.items {
                    match inside {
                        syn::TraitItem::Fn(method) => out.push(format!(
                            "  {}{}{}",
                            signature(&method.sig, &format!("{name}::{}", method.sig.ident)),
                            if method.default.is_some() {
                                " (provided)"
                            } else {
                                ""
                            },
                            attributes(&method.attrs, &[])
                        )),
                        syn::TraitItem::Const(constant) => out.push(format!(
                            "  const {name}::{}: {}{}",
                            constant.ident,
                            one_line(&constant.ty),
                            attributes(&constant.attrs, &[])
                        )),
                        syn::TraitItem::Type(associated) => {
                            let bounds = if associated.bounds.is_empty() {
                                String::new()
                            } else {
                                format!(": {}", one_line(&associated.bounds))
                            };
                            out.push(format!(
                                "  type {name}::{}{bounds}{}",
                                associated.ident,
                                attributes(&associated.attrs, &[])
                            ));
                        }
                        _ => {}
                    }
                }
            }
            Item::Fn(declared) => out.push(format!(
                "{}{}",
                signature(&declared.sig, name),
                attributes(&declared.attrs, gates)
            )),
            Item::Const(declared) => out.push(format!(
                "const {name}: {}{}",
                one_line(&declared.ty),
                attributes(&declared.attrs, gates)
            )),
            Item::Static(declared) => out.push(format!(
                "static {}{name}: {}{}",
                if matches!(declared.mutability, syn::StaticMutability::Mut(_)) {
                    "mut "
                } else {
                    ""
                },
                one_line(&declared.ty),
                attributes(&declared.attrs, gates)
            )),
            Item::Type(declared) => out.push(format!(
                "type {name}{} = {}{}",
                generics(&declared.generics),
                one_line(&declared.ty),
                attributes(&declared.attrs, gates)
            )),
            Item::Union(declared) => out.push(format!(
                "union {name}{}{}",
                generics(&declared.generics),
                attributes(&declared.attrs, gates)
            )),
            _ => {}
        }
    }

    /// The inherent methods and constants of public types, and the trait
    /// implementations that mention a public item.
    fn describe_impls(&self, primary: &BTreeMap<Def, String>, out: &mut Vec<String>) {
        for (module, declared) in self.modules.iter().enumerate() {
            for item in &declared.items {
                let Item::Impl(block) = item else {
                    continue;
                };
                if block.trait_.is_some() {
                    if self.mentions_public(module, block, primary) {
                        out.push(format!(
                            "{}{}",
                            impl_header(block),
                            attributes(&block.attrs, &declared.gates)
                        ));
                    }
                    continue;
                }
                let mut gates = declared.gates.clone();
                gates.extend(gate_attributes(&block.attrs));
                let mut owners = BTreeSet::new();
                if let Type::Path(path) = &*block.self_ty
                    && path.qself.is_none()
                {
                    let segments: Vec<String> = path
                        .path
                        .segments
                        .iter()
                        .map(|segment| segment.ident.to_string())
                        .collect();
                    for target in self.find(module, &segments) {
                        if let Target::Def(def) = target
                            && let Some(name) = primary.get(&def)
                        {
                            owners.insert(name.clone());
                        }
                    }
                }
                // The bounds the methods are available under, when there are any.
                let bounds = if block.generics.params.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", impl_header(block))
                };
                for owner in owners {
                    for inside in &block.items {
                        match inside {
                            ImplItem::Fn(method) if public(&method.vis) => out.push(format!(
                                "  {}{}{bounds}",
                                signature(&method.sig, &format!("{owner}::{}", method.sig.ident)),
                                attributes(&method.attrs, &gates)
                            )),
                            ImplItem::Const(constant) if public(&constant.vis) => {
                                out.push(format!(
                                    "  const {owner}::{}: {}{}{bounds}",
                                    constant.ident,
                                    one_line(&constant.ty),
                                    attributes(&constant.attrs, &gates)
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    /// Whether the header of an `impl` names an item an application can reach.
    fn mentions_public(
        &self,
        module: usize,
        block: &syn::ItemImpl,
        primary: &BTreeMap<Def, String>,
    ) -> bool {
        let mut header = TokenStream::new();
        block.generics.to_tokens(&mut header);
        if let Some((_, path, _)) = &block.trait_ {
            path.to_tokens(&mut header);
        }
        block.self_ty.to_tokens(&mut header);
        block.generics.where_clause.to_tokens(&mut header);
        let trees = flatten(header);
        let mut position = 0;
        while position < trees.len() {
            let TokenTree::Ident(first) = &trees[position] else {
                position += 1;
                continue;
            };
            // A path: `a::b::C`.
            let mut path = vec![first.to_string()];
            position += 1;
            while let [
                TokenTree::Punct(a),
                TokenTree::Punct(b),
                TokenTree::Ident(next),
                ..,
            ] = &trees[position..]
                && a.as_char() == ':'
                && b.as_char() == ':'
            {
                path.push(next.to_string());
                position += 3;
            }
            if self
                .find(module, &path)
                .iter()
                .any(|target| matches!(target, Target::Def(def) if primary.contains_key(def)))
            {
                return true;
            }
        }
        false
    }
}

fn impl_header(block: &syn::ItemImpl) -> String {
    let (generics, _, where_clause) = block.generics.split_for_impl();
    let unsafety = block.unsafety;
    let self_ty = &block.self_ty;
    let target = match &block.trait_ {
        Some((negative, path, _)) => quote!(#negative #path for #self_ty),
        None => quote!(#self_ty),
    };
    one_line(&quote!(#unsafety impl #generics #target #where_clause))
}

/// A function's signature, bounds included, with its name replaced by `name`.
fn signature(signature: &syn::Signature, name: &str) -> String {
    one_line(signature).replacen(&format!("fn {}", signature.ident), &format!("fn {name}"), 1)
}

fn generics(generics: &syn::Generics) -> String {
    let mut text = one_line(generics);
    if let Some(clause) = &generics.where_clause {
        text.push(' ');
        text.push_str(&one_line(clause));
    }
    text
}

// Attributes.

fn is_test(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && attribute
                .meta
                .require_list()
                .is_ok_and(|list| list.tokens.to_string() == "test")
    })
}

/// The `cfg` attributes among these, as written.
fn gate_attributes(attributes: &[Attribute]) -> Vec<String> {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("cfg"))
        .map(one_line)
        .collect()
}

/// The attributes that belong to the contract, as a suffix of a line: the
/// inherited `cfg` gates and the item's own, in a fixed order.
fn attributes(attributes: &[Attribute], inherited: &[String]) -> String {
    let mut found: Vec<(u8, String)> = inherited.iter().map(|gate| (0, gate.clone())).collect();
    let mut derived = Vec::new();
    for attribute in attributes {
        let rank = match last_segment(attribute.path()).as_str() {
            "cfg" => 0,
            "cfg_attr" => 1,
            "derive" => {
                let _ = attribute.parse_nested_meta(|meta| {
                    derived.push(one_line(&meta.path));
                    Ok(())
                });
                continue;
            }
            "repr" => 3,
            "non_exhaustive" => 4,
            "must_use" => 5,
            "deprecated" => 6,
            "track_caller" => 7,
            "proc_macro" | "proc_macro_derive" | "proc_macro_attribute" => 8,
            "doc"
                if attribute
                    .meta
                    .require_list()
                    .is_ok_and(|list| list.tokens.to_string() == "hidden") =>
            {
                9
            }
            _ => continue,
        };
        found.push((rank, one_line(attribute)));
    }
    if !derived.is_empty() {
        derived.sort();
        found.push((2, format!("#[derive({})]", derived.join(", "))));
    }
    found.sort();
    found.dedup();
    found
        .iter()
        .fold(String::new(), |mut text, (_, attribute)| {
            text.push(' ');
            text.push_str(attribute);
            text
        })
}

// Tokens as text.

/// The leaves of a token stream, groups opened.
fn flatten(stream: TokenStream) -> Vec<TokenTree> {
    let mut out = Vec::new();
    for tree in stream {
        match tree {
            TokenTree::Group(group) => out.extend(flatten(group.stream())),
            other => out.push(other),
        }
    }
    out
}

enum Piece {
    Word(String),
    Op(String),
    Open(char),
    Close(char),
}

/// Operators made of several characters that stay together; any other run of
/// punctuation is read one character at a time (`>>` closes two generics).
const OPERATORS: [&str; 12] = [
    "::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "..", "..=", "...",
];

fn pieces(stream: TokenStream, out: &mut Vec<Piece>) {
    let mut trees = stream.into_iter().peekable();
    while let Some(tree) = trees.next() {
        match tree {
            TokenTree::Group(group) => {
                let (open, close) = match group.delimiter() {
                    Delimiter::Parenthesis => ('(', ')'),
                    Delimiter::Bracket => ('[', ']'),
                    Delimiter::Brace => ('{', '}'),
                    Delimiter::None => {
                        pieces(group.stream(), out);
                        continue;
                    }
                };
                out.push(Piece::Open(open));
                pieces(group.stream(), out);
                out.push(Piece::Close(close));
            }
            TokenTree::Ident(word) => out.push(Piece::Word(word.to_string())),
            TokenTree::Literal(literal) => out.push(Piece::Word(literal.to_string())),
            TokenTree::Punct(punct) => {
                if punct.as_char() == '\'' {
                    if let Some(TokenTree::Ident(word)) = trees.next() {
                        out.push(Piece::Word(format!("'{word}")));
                    }
                    continue;
                }
                let mut op = punct.as_char().to_string();
                let mut joint = punct.spacing() == Spacing::Joint;
                while joint {
                    let Some(TokenTree::Punct(next)) = trees.peek() else {
                        break;
                    };
                    let candidate = format!("{op}{}", next.as_char());
                    if !OPERATORS.iter().any(|known| known.starts_with(&candidate)) {
                        break;
                    }
                    joint = next.spacing() == Spacing::Joint;
                    op = candidate;
                    trees.next();
                }
                out.push(Piece::Op(op));
            }
        }
    }
}

/// Whether a space goes between two neighbouring pieces.
fn gap(before: &Piece, after: &Piece) -> bool {
    const KEYWORDS: [&str; 12] = [
        "dyn", "impl", "mut", "as", "in", "where", "for", "const", "unsafe", "ref", "move", "else",
    ];
    const TIGHT_BEFORE: [&str; 10] = [",", ";", "::", ":", ">", "<", "?", ".", "..", "..="];
    const TIGHT_AFTER: [&str; 11] = ["::", "<", "&", "&&", "#", "?", "!", ".", "..", "*", "-"];
    match (before, after) {
        (Piece::Open('{'), Piece::Close('}')) => false,
        (Piece::Open('{'), _) | (_, Piece::Close('}')) => true,
        (Piece::Open(_), _) | (_, Piece::Close(_)) => false,
        (Piece::Word(word), Piece::Op(op)) if op == "::" && KEYWORDS.contains(&word.as_str()) => {
            true
        }
        (_, Piece::Op(op)) if TIGHT_BEFORE.contains(&op.as_str()) => false,
        (Piece::Op(op), _) if TIGHT_AFTER.contains(&op.as_str()) => false,
        (Piece::Word(word), Piece::Open('(')) => KEYWORDS.contains(&word.as_str()),
        (Piece::Op(op), Piece::Open('(')) if op == ">" => false,
        _ => true,
    }
}

/// Tokens on one line, spaced the way they are written in a signature.
fn one_line(tokens: &impl ToTokens) -> String {
    let mut all = Vec::new();
    pieces(tokens.to_token_stream(), &mut all);
    let mut out = String::new();
    for (position, piece) in all.iter().enumerate() {
        // A trailing comma depends on how the source was wrapped.
        let trailing = matches!(piece, Piece::Op(op) if op == ",")
            && matches!(all.get(position + 1), Some(Piece::Close(_)) | None);
        if trailing {
            continue;
        }
        if position > 0 && gap(&all[position - 1], piece) {
            out.push(' ');
        }
        match piece {
            Piece::Word(text) | Piece::Op(text) => out.push_str(text),
            Piece::Open(c) | Piece::Close(c) => out.push(*c),
        }
    }
    out
}
