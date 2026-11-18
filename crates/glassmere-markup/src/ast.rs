use crate::Value;
use glassmere_core::Style;
use std::ops::Range;

/// One thing in a view: a widget, or a condition or loop around some.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Node {
    Element(Element),
    If(If),
    For(For),
}

/// A widget, or the use of a component.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Element {
    /// Its number in the file, in the order elements are written. A change
    /// to the file renumbers them.
    pub id: usize,
    /// `button`, `column`, or the name of a component.
    pub kind: String,
    /// The stylesheet class after the dot in `label.title`.
    pub class: Option<String>,
    pub text: Option<Text>,
    pub properties: Vec<Property>,
    pub children: Vec<Node>,
    /// The whole element in the source, in bytes.
    pub span: Range<usize>,
    /// The inside of its braces, when it has them.
    pub body: Option<Range<usize>>,
    /// The line it starts on, from 1.
    pub line: usize,
    /// The style properties whose values are fixed, already applied.
    pub(crate) fixed: Box<Style>,
    pub(crate) styled: bool,
}

/// The text written after an element's name.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Text {
    /// As written between the quotes, escapes and `{name}` included.
    pub raw: String,
    /// The quoted text in the source, quotes included.
    pub span: Range<usize>,
    pub(crate) pieces: Vec<Piece>,
}

/// One `name: value` line of an element.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Property {
    pub name: String,
    /// The value as written.
    pub value: String,
    /// Name and value in the source.
    pub span: Range<usize>,
    pub value_span: Range<usize>,
    pub line: usize,
    pub(crate) kind: PropertyKind,
}

#[derive(Clone, Debug)]
pub(crate) enum PropertyKind {
    Value(Expr),
    Event {
        name: String,
        args: Vec<Expr>,
    },
    Bind(Vec<String>),
    /// Applied when the file was read.
    Style,
    /// A style value with `{name}` in it, applied each time the view is shown.
    DynamicStyle(Vec<Piece>),
}

/// `if condition { } else { }`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct If {
    /// The condition as written.
    pub condition: String,
    pub then: Vec<Node>,
    pub otherwise: Vec<Node>,
    pub span: Range<usize>,
    pub line: usize,
    pub(crate) id: usize,
    pub(crate) test: Expr,
}

/// `for item in items { }` or `for item, index in items { }`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct For {
    pub item: String,
    pub index: Option<String>,
    /// The list as written.
    pub list: String,
    pub body: Vec<Node>,
    pub span: Range<usize>,
    pub line: usize,
    pub(crate) id: usize,
    pub(crate) items: Expr,
}

#[derive(Clone, Debug)]
pub(crate) struct Component {
    pub parameters: Vec<String>,
    pub body: Vec<Node>,
}

#[derive(Clone, Debug)]
pub(crate) enum Piece {
    Text(String),
    Expr(Expr),
}

#[derive(Clone, Debug)]
pub(crate) enum Expr {
    Literal(Value),
    Path(Vec<String>),
    Template(Vec<Piece>),
    List(Vec<Expr>),
    Record(Vec<(String, Expr)>),
    Not(Box<Expr>),
    Negate(Box<Expr>),
    Length(Box<Expr>),
    Binary(Operator, Box<Expr>, Box<Expr>),
    Choice(Box<Expr>, Box<Expr>, Box<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operator {
    Or,
    And,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}
