//! Reads a markup file into its tree.
use crate::ast::{
    Component, Element, Expr, For, If, Node, Operator, Piece, Property, PropertyKind, Text,
};
use crate::eval::Scope;
use crate::{MarkupError, Value, Values};
use glassmere_core::Style;
use std::collections::BTreeMap;
use std::ops::Range;

/// What an element may be given besides style properties.
pub(crate) struct Kind {
    pub name: &'static str,
    pub text: bool,
    pub children: bool,
    pub values: &'static [&'static str],
    pub events: &'static [&'static str],
    pub bind: bool,
}

const fn kind(
    name: &'static str,
    text: bool,
    children: bool,
    values: &'static [&'static str],
    events: &'static [&'static str],
    bind: bool,
) -> Kind {
    Kind {
        name,
        text,
        children,
        values,
        events,
        bind,
    }
}

pub(crate) const KINDS: &[Kind] = &[
    kind("column", false, true, &[], &[], false),
    kind("row", false, true, &[], &[], false),
    kind("box", false, true, &[], &[], false),
    kind("grid", false, true, &[], &[], false),
    kind("scroll", false, true, &[], &[], false),
    kind("spacer", false, false, &[], &[], false),
    kind("label", true, false, &["selectable"], &[], false),
    kind("button", true, false, &[], &["on-click"], false),
    kind("link", true, false, &[], &["on-click"], false),
    kind("checkbox", true, false, &["checked"], &["on-toggle"], true),
    kind("switch", true, false, &["checked"], &["on-toggle"], true),
    kind(
        "radio",
        true,
        false,
        &["selected", "option"],
        &["on-select"],
        true,
    ),
    kind(
        "slider",
        true,
        false,
        &["value", "min", "max", "step"],
        &["on-change"],
        true,
    ),
    kind(
        "input",
        true,
        false,
        &["value", "placeholder", "password"],
        &["on-change", "on-submit"],
        true,
    ),
    kind(
        "number",
        true,
        false,
        &["value", "min", "max", "step", "decimals", "suffix"],
        &["on-change"],
        true,
    ),
    kind("progress", true, false, &["value"], &[], false),
    kind("spinner", true, false, &[], &[], false),
    kind(
        "combo",
        true,
        false,
        &["items", "selected"],
        &["on-choose"],
        true,
    ),
    kind("image", true, false, &["src", "fit"], &[], false),
    kind("tabs", false, true, &["selected"], &["on-select"], true),
    kind("tab", true, true, &[], &[], false),
    kind("collapsible", true, true, &["open"], &["on-toggle"], true),
];

/// What every element takes.
const COMMON: [&str; 3] = ["key", "enabled", "tooltip"];

pub(crate) struct Parsed {
    pub nodes: Vec<Node>,
    pub components: BTreeMap<String, Component>,
    pub defaults: Values,
    pub elements: usize,
}

struct Parser<'s> {
    src: &'s str,
    pos: usize,
    end: usize,
    ids: usize,
    /// The elements whose bodies are open, innermost last.
    parents: Vec<String>,
    /// Where each component is used, and with which properties.
    uses: Vec<Use>,
}

/// A component's name, where it is used, and the properties it is given
/// there with where each is.
type Use = (String, usize, Vec<(String, usize)>);

type Res<T> = Result<T, MarkupError>;

pub(crate) fn parse(src: &str) -> Res<Parsed> {
    let mut parser = Parser {
        src,
        // Editors on Windows put a byte-order mark first.
        pos: if src.starts_with('\u{feff}') {
            '\u{feff}'.len_utf8()
        } else {
            0
        },
        end: src.len(),
        ids: 0,
        parents: Vec::new(),
        uses: Vec::new(),
    };
    let mut parsed = Parsed {
        nodes: Vec::new(),
        components: BTreeMap::new(),
        defaults: Values::new(),
        elements: 0,
    };
    loop {
        parser.skip();
        if parser.pos >= parser.end {
            break;
        }
        let start = parser.pos;
        match parser.word() {
            "data" if parser.opens_after("data") => {
                parser.pos += "data".len();
                parser.data(&mut parsed.defaults)?;
            }
            "component" => {
                parser.pos += "component".len();
                parser.skip_inline();
                let name = parser.identifier("a component name")?;
                if !name.starts_with(char::is_uppercase) {
                    return Err(parser.error(start, "a component's name starts with a capital"));
                }
                let mut parameters = Vec::new();
                parser.skip_inline();
                if parser.eat('(') {
                    loop {
                        parser.skip_with(",");
                        if parser.eat(')') {
                            break;
                        }
                        parameters.push(parser.identifier("a parameter name")?);
                    }
                }
                parser.skip_inline();
                parser.expect('{')?;
                let body = parser.nodes_only("a component")?;
                if parsed
                    .components
                    .insert(name.clone(), Component { parameters, body })
                    .is_some()
                {
                    return Err(parser.error(start, format!("component `{name}` is defined twice")));
                }
            }
            _ => parsed.nodes.push(parser.node()?),
        }
    }
    for (name, at, properties) in &parser.uses {
        let Some(component) = parsed.components.get(name) else {
            return Err(parser.error(*at, format!("unknown component `{name}`")));
        };
        for (property, at) in properties {
            // An empty name is the text after the component's name, which
            // is its first parameter.
            if property.is_empty() {
                if component.parameters.is_empty() {
                    return Err(parser.error(*at, format!("component `{name}` takes no text")));
                }
                continue;
            }
            if !component.parameters.contains(property) {
                return Err(parser.error(
                    *at,
                    format!("component `{name}` has no parameter `{property}`"),
                ));
            }
        }
    }
    parsed.elements = parser.ids;
    Ok(parsed)
}

fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

impl<'s> Parser<'s> {
    fn error(&self, at: usize, message: impl Into<String>) -> MarkupError {
        let (line, column) = self.place(at);
        MarkupError {
            line,
            column,
            message: message.into(),
        }
    }

    fn place(&self, at: usize) -> (usize, usize) {
        let before = &self.src[..at.min(self.src.len())];
        let line = before.matches('\n').count() + 1;
        let column = before
            .rsplit('\n')
            .next()
            .map_or(0, |tail| tail.chars().count())
            + 1;
        (line, column)
    }

    fn rest(&self) -> &'s str {
        &self.src[self.pos.min(self.end)..self.end]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, c: char) -> Res<()> {
        if self.eat(c) {
            return Ok(());
        }
        Err(self.error(self.pos, format!("expected `{c}`{}", self.found())))
    }

    fn found(&self) -> String {
        match self.peek() {
            Some('\n') | None => ", found the end of the line".into(),
            Some(c) => format!(", found `{c}`"),
        }
    }

    fn skip_inline(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.pos += 1;
        }
    }

    /// Past white space, comments, and any of `separators`.
    fn skip_with(&mut self, separators: &str) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() || separators.contains(c) => self.pos += c.len_utf8(),
                Some('/') if self.rest().starts_with("//") => {
                    self.pos += self.rest().find('\n').unwrap_or(self.rest().len());
                }
                _ => break,
            }
        }
    }

    fn skip(&mut self) {
        self.skip_with(";");
    }

    /// The letters ahead, without taking them.
    fn word(&self) -> &'s str {
        let rest = self.rest();
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        &rest[..end]
    }

    fn opens_after(&self, word: &str) -> bool {
        self.rest()[word.len()..].trim_start().starts_with('{')
    }

    /// A name of the expression language: letters, digits and `_`.
    fn identifier(&mut self, what: &str) -> Res<String> {
        let word = self.word();
        if !word.starts_with(is_name_start) {
            return Err(self.error(self.pos, format!("expected {what}{}", self.found())));
        }
        self.pos += word.len();
        Ok(word.to_owned())
    }

    /// The name of an element or a property: `text-input`, `label.title`,
    /// `hover.background`.
    fn name(&mut self) -> Res<&'s str> {
        let rest = self.rest();
        if !rest.starts_with(is_name_start) {
            return Err(self.error(
                self.pos,
                format!("expected an element or a property{}", self.found()),
            ));
        }
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.')))
            .unwrap_or(rest.len());
        let name = rest[..end].trim_end_matches('.');
        self.pos += name.len();
        Ok(name)
    }

    /// Where a value ends: at the end of its line, at `;`, or at the `}`
    /// that closes what it is in. Brackets and quotes keep it going.
    fn value(&mut self, stop_at_comma: bool) -> Res<Range<usize>> {
        let start = self.pos;
        let mut depth = 0_usize;
        let bytes = self.src.as_bytes();
        while self.pos < self.end {
            match bytes[self.pos] {
                b'"' => self.pos = self.string_end(self.pos)?,
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' => depth = depth.saturating_sub(1),
                b'}' if depth == 0 => break,
                b'}' => depth -= 1,
                b'\n' | b';' if depth == 0 => break,
                b',' if depth == 0 && stop_at_comma => break,
                b'/' if self.rest().starts_with("//") => {
                    if depth == 0 {
                        break;
                    }
                    self.pos += self.rest().find('\n').unwrap_or(self.rest().len());
                    continue;
                }
                _ => {}
            }
            self.pos += 1;
        }
        let end = start + self.src[start..self.pos].trim_end().len();
        if end == start {
            return Err(self.error(start, "expected a value"));
        }
        Ok(start..end)
    }

    /// The index of the quote that closes the text opening at `from`.
    fn string_end(&self, from: usize) -> Res<usize> {
        let bytes = self.src.as_bytes();
        let mut at = from + 1;
        while at < self.end {
            match bytes[at] {
                b'\\' => at += 1,
                b'"' => return Ok(at),
                b'\n' => break,
                b'{' if bytes.get(at + 1) == Some(&b'{') => at += 1,
                b'{' => {
                    let mut depth = 1;
                    at += 1;
                    while at < self.end && depth > 0 {
                        match bytes[at] {
                            b'"' => at = self.string_end(at)?,
                            b'{' => depth += 1,
                            b'}' => depth -= 1,
                            b'\n' => return Err(self.error(from, "this text has no closing quote")),
                            _ => {}
                        }
                        at += 1;
                    }
                    continue;
                }
                _ => {}
            }
            at += 1;
        }
        Err(self.error(from, "this text has no closing quote"))
    }

    fn sub(&self, range: Range<usize>) -> Parser<'s> {
        Parser {
            src: self.src,
            pos: range.start,
            end: range.end,
            ids: 0,
            parents: Vec::new(),
            uses: Vec::new(),
        }
    }

    /// The whole of `range` as one expression.
    fn expression_in(&self, range: Range<usize>) -> Res<Expr> {
        let mut sub = self.sub(range);
        let expr = sub.expression()?;
        sub.skip();
        if sub.pos < sub.end {
            return Err(sub.error(sub.pos, format!("unexpected `{}`", sub.rest())));
        }
        Ok(expr)
    }

    /// Text with `{expression}` in it. `quoted` text also has `\` escapes.
    fn template(&self, range: Range<usize>, quoted: bool) -> Res<Vec<Piece>> {
        let mut pieces = Vec::new();
        let mut text = String::new();
        let source = &self.src[range.clone()];
        let mut chars = source.char_indices().peekable();
        while let Some((index, c)) = chars.next() {
            let next = chars.peek().map(|(_, c)| *c);
            match c {
                '\\' if quoted => match chars.next() {
                    Some((_, 'n')) => text.push('\n'),
                    Some((_, 't')) => text.push('\t'),
                    Some((_, other)) => text.push(other),
                    None => {}
                },
                '{' if next == Some('{') => {
                    chars.next();
                    text.push('{');
                }
                '}' if next == Some('}') => {
                    chars.next();
                    text.push('}');
                }
                '{' => {
                    let start = range.start + index + 1;
                    let mut depth = 1;
                    let mut end = None;
                    while let Some((index, c)) = chars.next() {
                        match c {
                            '"' => {
                                let close =
                                    self.sub(range.clone()).string_end(range.start + index)?;
                                while chars
                                    .next_if(|(index, _)| range.start + index <= close)
                                    .is_some()
                                {}
                            }
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = Some(range.start + index);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    let Some(end) = end else {
                        return Err(self.error(
                            start - 1,
                            "this `{` has no closing `}`; write `{{` for a brace",
                        ));
                    };
                    if !text.is_empty() {
                        pieces.push(Piece::Text(std::mem::take(&mut text)));
                    }
                    pieces.push(Piece::Expr(self.expression_in(start..end)?));
                }
                '}' => {
                    return Err(self.error(range.start + index, "write `}}` for a closing brace"));
                }
                other => text.push(other),
            }
        }
        if !text.is_empty() {
            pieces.push(Piece::Text(text));
        }
        Ok(pieces)
    }

    fn text(&mut self) -> Res<Text> {
        let start = self.pos;
        let close = self.string_end(start)?;
        self.pos = close + 1;
        Ok(Text {
            raw: self.src[start + 1..close].to_owned(),
            span: start..close + 1,
            pieces: self.template(start + 1..close, true)?,
        })
    }

    fn node(&mut self) -> Res<Node> {
        let start = self.pos;
        let name = self.name()?;
        match name {
            "if" => Ok(Node::If(self.condition(start)?)),
            "for" => self.repeat(start).map(Node::For),
            _ => self.element(start, name).map(Node::Element),
        }
    }

    /// The expression between here and the `{` that opens a body.
    fn header(&mut self, what: &str) -> Res<Range<usize>> {
        self.skip_inline();
        let start = self.pos;
        let bytes = self.src.as_bytes();
        let mut depth = 0_usize;
        while self.pos < self.end {
            match bytes[self.pos] {
                b'"' => self.pos = self.string_end(self.pos)?,
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth = depth.saturating_sub(1),
                b'{' if depth == 0 => break,
                b'\n' => break,
                _ => {}
            }
            self.pos += 1;
        }
        if self.peek() != Some('{') {
            return Err(self.error(start, format!("expected `{{` after {what}")));
        }
        let end = start + self.src[start..self.pos].trim_end().len();
        self.pos += 1;
        Ok(start..end)
    }

    fn condition(&mut self, start: usize) -> Res<If> {
        let (line, _) = self.place(start);
        let id = self.next_id();
        let header = self.header("the condition")?;
        let test = self.expression_in(header.clone())?;
        let then = self.nodes_only("`if`")?;
        let mut otherwise = Vec::new();
        let after = self.pos;
        self.skip();
        if self.word() == "else" {
            self.pos += "else".len();
            self.skip_inline();
            if self.word() == "if" {
                let start = self.pos;
                self.pos += "if".len();
                otherwise.push(Node::If(self.condition(start)?));
            } else {
                self.expect('{')?;
                otherwise = self.nodes_only("`else`")?;
            }
        } else {
            self.pos = after;
        }
        Ok(If {
            condition: self.src[header].to_owned(),
            then,
            otherwise,
            span: start..self.pos,
            line,
            id,
            test,
        })
    }

    fn repeat(&mut self, start: usize) -> Res<For> {
        let (line, _) = self.place(start);
        let id = self.next_id();
        self.skip_inline();
        let item = self.identifier("a name for each item")?;
        self.skip_inline();
        let index = if self.eat(',') {
            self.skip_inline();
            Some(self.identifier("a name for the index")?)
        } else {
            None
        };
        self.skip_inline();
        if self.word() != "in" {
            return Err(self.error(self.pos, "expected `in`, as in `for item in items`"));
        }
        self.pos += "in".len();
        let header = self.header("the list")?;
        let items = self.expression_in(header.clone())?;
        let body = self.nodes_only("`for`")?;
        Ok(For {
            item,
            index,
            list: self.src[header].to_owned(),
            body,
            span: start..self.pos,
            line,
            id,
            items,
        })
    }

    fn next_id(&mut self) -> usize {
        self.ids += 1;
        self.ids - 1
    }

    /// A body that holds elements and no properties, after its `{`.
    fn nodes_only(&mut self, what: &str) -> Res<Vec<Node>> {
        let (nodes, properties, _) = self.body()?;
        match properties.first() {
            Some((name, at, _)) => Err(self.error(
                *at,
                format!("`{name}` is a property, and {what} has none; put it in an element"),
            )),
            None => Ok(nodes),
        }
    }

    /// Elements and `name: value` lines up to the closing `}`.
    #[allow(clippy::type_complexity)]
    fn body(&mut self) -> Res<(Vec<Node>, Vec<(&'s str, usize, Range<usize>)>, Range<usize>)> {
        let open = self.pos;
        let mut nodes = Vec::new();
        let mut properties = Vec::new();
        loop {
            self.skip();
            if self.pos >= self.end {
                return Err(self.error(open - 1, "this `{` is never closed"));
            }
            if self.eat('}') {
                return Ok((nodes, properties, open..self.pos - 1));
            }
            let start = self.pos;
            let name = self.name()?;
            self.skip_inline();
            if self.peek() == Some(':') {
                self.pos += 1;
                self.skip_inline();
                properties.push((name, start, self.value(false)?));
            } else {
                self.pos = start;
                nodes.push(self.node()?);
            }
        }
    }

    fn element(&mut self, start: usize, name: &'s str) -> Res<Element> {
        let (line, _) = self.place(start);
        let (kind_name, class) = match name.split_once('.') {
            Some((kind, class)) => (kind, Some(class.to_owned())),
            None => (name, None),
        };
        let component = kind_name.starts_with(char::is_uppercase);
        let kind = KINDS.iter().find(|kind| kind.name == kind_name);
        if kind.is_none() && !component {
            let known: Vec<_> = KINDS.iter().map(|kind| kind.name).collect();
            return Err(self.error(
                start,
                format!(
                    "unknown element `{kind_name}`; there are {}",
                    known.join(", ")
                ),
            ));
        }
        if kind_name == "tab" && self.parents.last().map(String::as_str) != Some("tabs") {
            return Err(self.error(start, "a `tab` goes directly inside `tabs`"));
        }
        let id = self.next_id();
        self.skip_inline();
        let text = if self.peek() == Some('"') {
            if kind.is_some_and(|kind| !kind.text) {
                return Err(self.error(self.pos, format!("`{kind_name}` takes no text")));
            }
            Some(self.text()?)
        } else {
            None
        };
        self.skip_inline();
        let (children, raw, body) = if self.eat('{') {
            self.parents.push(kind_name.to_owned());
            let (children, raw, body) = self.body()?;
            self.parents.pop();
            (children, raw, Some(body))
        } else {
            (Vec::new(), Vec::new(), None)
        };
        if let Some(child) = children.first()
            && !kind.is_none_or(|kind| kind.children)
        {
            let at = match child {
                Node::Element(element) => element.span.start,
                Node::If(node) => node.span.start,
                Node::For(node) => node.span.start,
            };
            return Err(self.error(at, format!("`{kind_name}` holds no elements")));
        }
        if kind_name == "tabs"
            && let Some(at) = children.iter().find_map(|child| match child {
                Node::Element(element) if element.kind == "tab" => None,
                Node::Element(element) => Some(element.span.start),
                Node::If(node) => Some(node.span.start),
                Node::For(node) => Some(node.span.start),
            })
        {
            return Err(self.error(at, "`tabs` holds `tab` elements only"));
        }
        let mut fixed = Box::new(if kind_name == "button" {
            Style::button()
        } else {
            Style::default()
        });
        let mut styled = false;
        let mut properties = Vec::new();
        let mut used = Vec::new();
        for (name, at, value_span) in raw {
            let value = &self.src[value_span.clone()];
            let property_kind = if component && name != "key" {
                used.push((name.to_owned(), at));
                PropertyKind::Value(self.expression_in(value_span.clone())?)
            } else if COMMON.contains(&name) || kind.is_some_and(|kind| kind.values.contains(&name))
            {
                PropertyKind::Value(self.expression_in(value_span.clone())?)
            } else if name == "bind" {
                if !kind.is_some_and(|kind| kind.bind) {
                    return Err(self.error(at, format!("`{kind_name}` has no value to bind")));
                }
                let mut sub = self.sub(value_span.clone());
                let mut path = vec![sub.identifier("a name to bind to")?];
                while sub.eat('.') {
                    let rest = sub.rest();
                    let end = rest
                        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .unwrap_or(rest.len());
                    path.push(rest[..end].to_owned());
                    sub.pos += end;
                }
                if sub.pos < sub.end {
                    return Err(sub.error(sub.pos, "`bind` takes a name such as `user.name`"));
                }
                PropertyKind::Bind(path)
            } else if name.starts_with("on-") {
                let events = kind.map_or(&[][..], |kind| kind.events);
                if !events.contains(&name) {
                    return Err(self.error(
                        at,
                        if events.is_empty() {
                            format!("`{kind_name}` sends no events")
                        } else {
                            format!(
                                "`{kind_name}` has no `{name}`; it has {}",
                                events.join(", ")
                            )
                        },
                    ));
                }
                self.sub(value_span.clone()).event()?
            } else if value.contains('{') {
                PropertyKind::DynamicStyle(self.template(value_span.clone(), false)?)
            } else {
                fixed
                    .set_property(name, value)
                    .map_err(|message| self.error(value_span.start, message))?;
                styled = true;
                PropertyKind::Style
            };
            styled |= matches!(property_kind, PropertyKind::DynamicStyle(_));
            properties.push(Property {
                name: name.to_owned(),
                value: value.to_owned(),
                span: at..value_span.end,
                value_span,
                line: self.place(at).0,
                kind: property_kind,
            });
        }
        if component {
            if text.is_some() {
                used.push((String::new(), start));
            }
            self.uses.push((kind_name.to_owned(), start, used));
        }
        Ok(Element {
            id,
            kind: kind_name.to_owned(),
            class,
            text,
            properties,
            children,
            span: start..self.pos,
            body,
            line,
            fixed,
            styled,
        })
    }

    /// `name` or `name(argument, ...)`.
    fn event(&mut self) -> Res<PropertyKind> {
        let rest = self.rest();
        if !rest.starts_with(is_name_start) {
            return Err(self.error(self.pos, "expected the name of an event"));
        }
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.')))
            .unwrap_or(rest.len());
        let name = rest[..end].to_owned();
        self.pos += end;
        let mut args = Vec::new();
        if self.eat('(') {
            loop {
                self.skip_with(",");
                if self.eat(')') {
                    break;
                }
                if self.pos >= self.end {
                    return Err(self.error(self.pos, "expected `)`"));
                }
                args.push(self.expression()?);
            }
        }
        self.skip();
        if self.pos < self.end {
            return Err(self.error(self.pos, format!("unexpected `{}`", self.rest())));
        }
        Ok(PropertyKind::Event { name, args })
    }

    fn data(&mut self, defaults: &mut Values) -> Res<()> {
        self.skip_inline();
        self.expect('{')?;
        let open = self.pos - 1;
        loop {
            self.skip_with(";,");
            if self.eat('}') {
                return Ok(());
            }
            if self.pos >= self.end {
                return Err(self.error(open, "this `{` is never closed"));
            }
            let name = self.identifier("a name")?;
            self.skip_inline();
            self.expect(':')?;
            self.skip_inline();
            let span = self.value(true)?;
            let value = Scope::constant().eval(&self.expression_in(span)?);
            defaults.set(&name, value);
        }
    }

    fn expression(&mut self) -> Res<Expr> {
        let test = self.binary(1)?;
        self.skip();
        if !self.eat('?') {
            return Ok(test);
        }
        let then = self.expression()?;
        self.skip();
        self.expect(':')?;
        let otherwise = self.expression()?;
        Ok(Expr::Choice(
            Box::new(test),
            Box::new(then),
            Box::new(otherwise),
        ))
    }

    fn operator(&self) -> Option<(Operator, usize, u8)> {
        const OPERATORS: [(&str, Operator, u8); 13] = [
            ("||", Operator::Or, 1),
            ("&&", Operator::And, 2),
            ("==", Operator::Equal, 3),
            ("!=", Operator::NotEqual, 3),
            ("<=", Operator::LessEqual, 3),
            (">=", Operator::GreaterEqual, 3),
            ("<", Operator::Less, 3),
            (">", Operator::Greater, 3),
            ("+", Operator::Add, 4),
            ("-", Operator::Subtract, 4),
            ("*", Operator::Multiply, 5),
            ("/", Operator::Divide, 5),
            ("%", Operator::Remainder, 5),
        ];
        let rest = self.rest();
        OPERATORS
            .iter()
            .find(|(text, ..)| rest.starts_with(text))
            .map(|(text, operator, level)| (*operator, text.len(), *level))
    }

    fn binary(&mut self, level: u8) -> Res<Expr> {
        let mut left = self.unary()?;
        loop {
            self.skip();
            let Some((operator, length, found)) = self.operator() else {
                break;
            };
            if found < level {
                break;
            }
            self.pos += length;
            let right = self.binary(found + 1)?;
            left = Expr::Binary(operator, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Res<Expr> {
        self.skip();
        if self.eat('!') {
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        if self.eat('-') {
            return Ok(Expr::Negate(Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Res<Expr> {
        let start = self.pos;
        match self.peek() {
            Some('"') => {
                let close = self.string_end(start)?;
                self.pos = close + 1;
                let mut pieces = self.template(start + 1..close, true)?;
                Ok(match pieces.len() {
                    0 => Expr::Literal(Value::Text(String::new())),
                    1 if matches!(pieces[0], Piece::Text(_)) => {
                        let Some(Piece::Text(text)) = pieces.pop() else {
                            unreachable!()
                        };
                        Expr::Literal(Value::Text(text))
                    }
                    _ => Expr::Template(pieces),
                })
            }
            Some('(') => {
                self.pos += 1;
                let inner = self.expression()?;
                self.skip();
                self.expect(')')?;
                Ok(inner)
            }
            Some('[') => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_with(",");
                    if self.eat(']') {
                        break;
                    }
                    if self.pos >= self.end {
                        return Err(self.error(start, "this `[` is never closed"));
                    }
                    items.push(self.expression()?);
                }
                Ok(Expr::List(items))
            }
            Some('{') => {
                self.pos += 1;
                let mut fields = Vec::new();
                loop {
                    self.skip_with(",");
                    if self.eat('}') {
                        break;
                    }
                    if self.pos >= self.end {
                        return Err(self.error(start, "this `{` is never closed"));
                    }
                    let name = self.identifier("a field name")?;
                    self.skip();
                    self.expect(':')?;
                    fields.push((name, self.expression()?));
                }
                Ok(Expr::Record(fields))
            }
            Some(c) if c.is_ascii_digit() => {
                let rest = self.rest();
                let mut end = rest
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(rest.len());
                if rest[end..].starts_with('.')
                    && rest[end + 1..].starts_with(|c: char| c.is_ascii_digit())
                {
                    end += 1;
                    end += rest[end..]
                        .find(|c: char| !c.is_ascii_digit())
                        .unwrap_or(rest.len() - end);
                }
                self.pos += end;
                let number = rest[..end]
                    .parse()
                    .map_err(|_| self.error(start, "this is not a number"))?;
                Ok(Expr::Literal(Value::Number(number)))
            }
            Some(c) if is_name_start(c) => {
                let word = self.identifier("a name")?;
                match word.as_str() {
                    "true" => return Ok(Expr::Literal(Value::Bool(true))),
                    "false" => return Ok(Expr::Literal(Value::Bool(false))),
                    "none" => return Ok(Expr::Literal(Value::None)),
                    "len" if self.peek() == Some('(') => {
                        self.pos += 1;
                        let inner = self.expression()?;
                        self.skip();
                        self.expect(')')?;
                        return Ok(Expr::Length(Box::new(inner)));
                    }
                    _ => {}
                }
                if self.peek() == Some('(') {
                    return Err(
                        self.error(start, format!("unknown function `{word}`; there is `len`"))
                    );
                }
                let mut path = vec![word];
                while self.peek() == Some('.') {
                    let rest = &self.rest()[1..];
                    let end = rest
                        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .unwrap_or(rest.len());
                    if end == 0 {
                        break;
                    }
                    path.push(rest[..end].to_owned());
                    self.pos += end + 1;
                }
                Ok(Expr::Path(path))
            }
            _ => Err(self.error(start, format!("expected a value{}", self.found()))),
        }
    }
}
