//! Views written in a markup file instead of Rust, read while the app runs.
//!
//! ```text
//! // counter.glass
//! data { count: 3, name: "Ada" }            // values to preview with
//!
//! column { padding: 24; gap: 12
//!     label.title "Hello, {name}"
//!     label "Clicked {count} times"
//!     row { gap: 8
//!         button "Decrease" { on-click: decrease; enabled: count > 0 }
//!         button "Increase" { on-click: increase; background: #16695f }
//!     }
//!     input "Name" { bind: name; placeholder: "Who are you?" }
//!     if count >= 10 { label "That is a lot." } else { label "Keep going." }
//!     for item, index in items { label "{index + 1}. {item.title}" }
//! }
//! ```
//!
//! An element is its kind, an optional `.class` from the stylesheet, optional
//! text, and optional braces holding `name: value` properties and child
//! elements. Properties end at the end of the line or at `;`. A property the
//! element does not know is a style property, in the stylesheet's own
//! syntax (`padding: 12`, `hover.background: #22b0a0`).
//!
//! | Element | Text | Properties | Events |
//! |---|---|---|---|
//! | `column` `row` `box` `grid` `scroll` | | | |
//! | `spacer` | | | |
//! | `label` | shown | `selectable` | |
//! | `button` `link` | shown | | `on-click` |
//! | `checkbox` `switch` | label | `checked`, `bind` | `on-toggle` |
//! | `radio` | label | `selected`, `option`, `bind` | `on-select` |
//! | `slider` | name | `value`, `min`, `max`, `step`, `bind` | `on-change` |
//! | `input` | name | `value`, `placeholder`, `password`, `bind` | `on-change`, `on-submit` |
//! | `number` | name | `value`, `min`, `max`, `step`, `decimals`, `suffix`, `bind` | `on-change` |
//! | `progress` | name | `value` (0 to 1; none for indeterminate) | |
//! | `spinner` | name | | |
//! | `combo` | name | `items`, `selected`, `bind` | `on-choose` |
//! | `image` | description | `src`, `fit` (`contain`, `cover`, `fill`, `none`) | |
//! | `tabs` holding `tab "Title" { }` | | `selected`, `bind` | `on-select` |
//! | `collapsible` | heading | `open`, `bind` | `on-toggle` |
//!
//! Every element also takes `enabled`, `tooltip` and `key` (a name that
//! follows the element when a loop reorders its items).
//!
//! Values are expressions over the application's [`Values`]: names and
//! `a.b` paths, numbers, `"text with {names}"`, `true`, `false`, `none`,
//! `[lists]`, `{records: 1}`, `! - * / % + - == != < <= > >= && ||`,
//! `condition ? this : that` and `len(list)`.
//!
//! `on-click: name` sends a [`MarkupEvent`] called `name`, and
//! `on-click: remove(item.id)` one with an argument. `bind: path` shows the
//! value at `path` and sends an event [`Values::apply`] stores back there.
//!
//! `component Card(title, body) { ... }` at the top level defines an element
//! of its own, used as `Card { title: "Hello"; body: text }` or, for the
//! first parameter, `Card "Hello"`.
//!
//! ```ignore
//! let view = Markup::parse(include_str!("counter.glass"))?;
//! // In `App::view`:
//! view.show(ui, &self.values, Msg::View);
//! // In `App::update`:
//! Msg::View(event) => if !self.values.apply(&event) {
//!     match event.name.as_str() { "increase" => ..., _ => {} }
//! }
//! ```
//!
//! [`LiveMarkup`] does the same from a file it reads again whenever the file
//! changes, and `glassmere-designer` previews and edits one without an
//! application around it.
mod ast;
mod eval;
mod live;
mod parse;
mod show;
mod value;

pub use ast::{Element, For, If, Node, Property, Text};
pub use live::LiveMarkup;
pub use value::{MarkupEvent, Value, Values};

use ast::{Component, Expr, Piece, PropertyKind};
use glassmere_core::{Context, NodeId, Ui};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;

/// Where a markup file went wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkupError {
    /// From 1.
    pub line: usize,
    /// From 1, in characters.
    pub column: usize,
    pub message: String,
}

impl std::fmt::Display for MarkupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}
impl std::error::Error for MarkupError {}

/// What [`Markup::show_with`] does besides showing.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ShowOptions {
    /// The element to draw a ring around, by [`Element::id`].
    pub selected: Option<usize>,
    /// Where the `src` of an `image` is relative to.
    pub base_dir: Option<PathBuf>,
}

impl ShowOptions {
    pub fn selected(mut self, element: Option<usize>) -> Self {
        self.selected = element;
        self
    }

    pub fn base_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(directory.into());
        self
    }
}

/// A parsed markup file.
#[derive(Clone, Debug)]
pub struct Markup {
    nodes: Vec<Node>,
    components: BTreeMap<String, Component>,
    defaults: Values,
    elements: usize,
}

impl Markup {
    pub fn parse(source: &str) -> Result<Self, MarkupError> {
        let parsed = parse::parse(source)?;
        Ok(Self {
            nodes: parsed.nodes,
            components: parsed.components,
            defaults: parsed.defaults,
            elements: parsed.elements,
        })
    }

    /// Declares the view. Every widget's message is `on` of what it sent.
    #[track_caller]
    pub fn show<M: 'static>(
        &self,
        ui: &mut Ui<'_, M>,
        values: &Values,
        on: impl Fn(MarkupEvent) -> M + 'static,
    ) {
        self.show_with(ui, values, &ShowOptions::default(), on);
    }

    #[track_caller]
    pub fn show_with<M: 'static>(
        &self,
        ui: &mut Ui<'_, M>,
        values: &Values,
        options: &ShowOptions,
        on: impl Fn(MarkupEvent) -> M + 'static,
    ) {
        show::Shower {
            markup: self,
            scope: eval::Scope::new(values),
            on: Rc::new(on),
            options,
            depth: 0,
        }
        .nodes(ui, &self.nodes, "");
    }

    /// The values of the file's `data` block: what it is previewed with, and
    /// a starting point for the application's own.
    pub fn defaults(&self) -> &Values {
        &self.defaults
    }

    /// The top-level elements, conditions and loops. Components are not
    /// among them.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The element with this [`Element::id`], wherever it is, components
    /// included.
    pub fn element(&self, id: usize) -> Option<&Element> {
        fn find(nodes: &[Node], id: usize) -> Option<&Element> {
            nodes.iter().find_map(|node| match node {
                Node::Element(element) if element.id == id => Some(element),
                Node::Element(element) => find(&element.children, id),
                Node::If(node) => find(&node.then, id).or_else(|| find(&node.otherwise, id)),
                Node::For(node) => find(&node.body, id),
            })
        }
        if id >= self.elements {
            return None;
        }
        find(&self.nodes, id).or_else(|| {
            self.components
                .values()
                .find_map(|component| find(&component.body, id))
        })
    }

    /// The element shown at `position`, going by the tree of the last frame:
    /// `Markup::pick(ui.context(), point)`.
    pub fn pick(context: &Context, position: glassmere_scene::Point) -> Option<usize> {
        let mut at: Option<NodeId> = context.hit_test(position);
        while let Some(id) = at {
            let key = context.details(id)?.key;
            if let Some(start) = key.rfind("gm") {
                let digits: String = key[start + 2..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                if let Ok(element) = digits.parse() {
                    return Some(element);
                }
            }
            at = context.node(id)?.parent;
        }
        None
    }

    /// The names the file reads from [`Values`]: the first part of every
    /// path that is not a loop variable or a component's parameter.
    pub fn references(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        references(&self.nodes, &mut Vec::new(), &mut names);
        for component in self.components.values() {
            references(
                &component.body,
                &mut component.parameters.clone(),
                &mut names,
            );
        }
        names
    }
}

fn references(nodes: &[Node], locals: &mut Vec<String>, names: &mut BTreeSet<String>) {
    fn pieces(pieces: &[Piece], locals: &[String], names: &mut BTreeSet<String>) {
        for piece in pieces {
            if let Piece::Expr(expr) = piece {
                expression(expr, locals, names);
            }
        }
    }
    fn expression(expr: &Expr, locals: &[String], names: &mut BTreeSet<String>) {
        match expr {
            Expr::Literal(_) => {}
            Expr::Path(path) => {
                if !locals.contains(&path[0]) {
                    names.insert(path[0].clone());
                }
            }
            Expr::Template(parts) => pieces(parts, locals, names),
            Expr::List(items) => items
                .iter()
                .for_each(|item| expression(item, locals, names)),
            Expr::Record(fields) => fields
                .iter()
                .for_each(|(_, value)| expression(value, locals, names)),
            Expr::Not(inner) | Expr::Negate(inner) | Expr::Length(inner) => {
                expression(inner, locals, names);
            }
            Expr::Binary(_, left, right) => {
                expression(left, locals, names);
                expression(right, locals, names);
            }
            Expr::Choice(test, then, otherwise) => {
                expression(test, locals, names);
                expression(then, locals, names);
                expression(otherwise, locals, names);
            }
        }
    }
    for node in nodes {
        match node {
            Node::Element(element) => {
                if let Some(text) = &element.text {
                    pieces(&text.pieces, locals, names);
                }
                for property in &element.properties {
                    match &property.kind {
                        PropertyKind::Value(expr) => expression(expr, locals, names),
                        PropertyKind::Event { args, .. } => {
                            args.iter().for_each(|arg| expression(arg, locals, names));
                        }
                        PropertyKind::Bind(path) => {
                            if !locals.contains(&path[0]) {
                                names.insert(path[0].clone());
                            }
                        }
                        PropertyKind::DynamicStyle(parts) => pieces(parts, locals, names),
                        PropertyKind::Style => {}
                    }
                }
                references(&element.children, locals, names);
            }
            Node::If(node) => {
                expression(&node.test, locals, names);
                references(&node.then, locals, names);
                references(&node.otherwise, locals, names);
            }
            Node::For(node) => {
                expression(&node.items, locals, names);
                let outer = locals.len();
                locals.push(node.item.clone());
                locals.extend(node.index.clone());
                references(&node.body, locals, names);
                locals.truncate(outer);
            }
        }
    }
}
