//! Changes to a markup file's text, placed by the spans of its parsed tree.
//!
//! Each returns the whole new text. The caller parses it again before
//! keeping it, so a change that breaks the file is never written.
use glassmere::markup::Element;

/// The white space the element's line starts with.
fn indent<'a>(source: &'a str, element: &Element) -> &'a str {
    let line = source[..element.span.start]
        .rfind('\n')
        .map_or(0, |at| at + 1);
    let text = &source[line..element.span.start];
    &text[..text.len() - text.trim_start().len()]
}

fn replace(source: &str, range: std::ops::Range<usize>, with: &str) -> String {
    format!("{}{with}{}", &source[..range.start], &source[range.end..])
}

/// Removes `range`, and its whole line when nothing else is on it.
fn remove(source: &str, mut range: std::ops::Range<usize>) -> String {
    let after = &source[range.end..];
    let trailing = after.len() - after.trim_start_matches([' ', '\t']).len();
    if after[trailing..].starts_with(';') {
        range.end += trailing + 1;
    }
    let line_start = source[..range.start].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[range.end..]
        .find('\n')
        .map_or(source.len(), |at| range.end + at);
    if source[line_start..range.start].trim().is_empty()
        && source[range.end..line_end].trim().is_empty()
    {
        range = line_start..(line_end + 1).min(source.len());
    } else {
        let after = &source[range.end..];
        range.end += after.len() - after.trim_start_matches([' ', '\t']).len();
    }
    replace(source, range, "")
}

/// Sets the value of a property the element has, or gives it the property.
pub fn set_property(source: &str, element: &Element, name: &str, value: &str) -> String {
    if let Some(property) = element
        .properties
        .iter()
        .find(|property| property.name == name)
    {
        return replace(source, property.value_span.clone(), value);
    }
    let Some(body) = &element.body else {
        return replace(
            source,
            element.span.end..element.span.end,
            &format!(" {{ {name}: {value} }}"),
        );
    };
    if source[body.clone()].contains('\n') {
        let inner = format!("\n{}    {name}: {value}", indent(source, element));
        replace(source, body.start..body.start, &inner)
    } else {
        replace(
            source,
            body.start..body.start,
            &format!(" {name}: {value};"),
        )
    }
}

pub fn remove_property(source: &str, element: &Element, name: &str) -> String {
    match element
        .properties
        .iter()
        .find(|property| property.name == name)
    {
        Some(property) => remove(source, property.span.clone()),
        None => source.to_owned(),
    }
}

/// Sets the text after the element's name, as the user sees it: quotes and
/// backslashes are escaped, `{name}` stays a value.
pub fn set_text(source: &str, element: &Element, text: &str) -> String {
    let quoted = format!(
        "\"{}\"",
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    );
    match &element.text {
        Some(text) => replace(source, text.span.clone(), &quoted),
        None => {
            let at = header_end(element);
            replace(source, at..at, &format!(" {quoted}"))
        }
    }
}

/// Where the element's name and class end.
fn header_end(element: &Element) -> usize {
    element.span.start
        + element.kind.len()
        + element.class.as_ref().map_or(0, |class| class.len() + 1)
}

/// Sets the class after the element's name. An empty one removes it.
pub fn set_class(source: &str, element: &Element, class: &str) -> String {
    let start = element.span.start + element.kind.len();
    let class = class.trim().trim_start_matches('.');
    let with = if class.is_empty() {
        String::new()
    } else {
        format!(".{class}")
    };
    replace(source, start..header_end(element), &with)
}

pub fn remove_element(source: &str, element: &Element) -> String {
    remove(source, element.span.clone())
}

/// Puts `snippet` in the element as its last child.
pub fn add_child(source: &str, element: &Element, snippet: &str) -> String {
    let outer = indent(source, element);
    let Some(body) = &element.body else {
        return replace(
            source,
            element.span.end..element.span.end,
            &format!(" {{\n{outer}    {snippet}\n{outer}}}"),
        );
    };
    let content = &source[body.clone()];
    let end = body.start + content.trim_end().len();
    // A body on one line gets its closing brace on a line of its own.
    let closing = if content.contains('\n') {
        source[end..body.end].to_owned()
    } else {
        format!("\n{outer}")
    };
    replace(
        source,
        end..body.end,
        &format!("\n{outer}    {snippet}{closing}"),
    )
}

/// Puts `snippet` after the element, as its next sibling.
pub fn add_after(source: &str, element: &Element, snippet: &str) -> String {
    let at = element.span.end;
    replace(
        source,
        at..at,
        &format!("\n{}{snippet}", indent(source, element)),
    )
}

/// Swaps the element with the sibling written before or after it.
pub fn swap(source: &str, first: &Element, second: &Element) -> String {
    let (first, second) = if first.span.start < second.span.start {
        (first, second)
    } else {
        (second, first)
    };
    format!(
        "{}{}{}{}{}",
        &source[..first.span.start],
        &source[second.span.clone()],
        &source[first.span.end..second.span.start],
        &source[first.span.clone()],
        &source[second.span.end..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use glassmere::markup::{Markup, Node};

    const SOURCE: &str = "column {\n    padding: 12\n    label.title \"Hi\"\n    row { gap: 8; button \"Go\" { on-click: go } }\n    spacer\n}\n";

    /// The element whose source starts with `start`.
    fn find(source: &str, start: &str) -> Element {
        fn walk<'a>(nodes: &'a [Node], out: &mut Vec<&'a Element>) {
            for node in nodes {
                if let Node::Element(element) = node {
                    out.push(element);
                    walk(&element.children, out);
                }
            }
        }
        let markup = Markup::parse(source).unwrap();
        let mut all = Vec::new();
        walk(markup.nodes(), &mut all);
        all.into_iter()
            .find(|element| source[element.span.clone()].starts_with(start))
            .unwrap()
            .clone()
    }

    /// Every edit has to leave a file that still parses.
    fn checked(text: String) -> String {
        Markup::parse(&text).unwrap_or_else(|error| panic!("{error}\n{text}"));
        text
    }

    #[test]
    fn properties_are_set_added_and_removed() {
        let column = find(SOURCE, "column");
        let text = checked(set_property(SOURCE, &column, "padding", "20"));
        assert!(text.starts_with("column {\n    padding: 20\n"));
        let text = checked(set_property(SOURCE, &column, "gap", "4"));
        assert!(text.starts_with("column {\n    gap: 4\n    padding: 12\n"));
        let text = checked(remove_property(SOURCE, &column, "padding"));
        assert!(text.starts_with("column {\n    label.title"));

        let row = find(SOURCE, "row");
        let text = checked(set_property(SOURCE, &row, "padding", "2"));
        assert!(text.contains("row { padding: 2; gap: 8; button"));
        let text = checked(remove_property(SOURCE, &row, "gap"));
        assert!(text.contains("row { button \"Go\""));

        let spacer = find(SOURCE, "spacer");
        let text = checked(set_property(SOURCE, &spacer, "grow", "2"));
        assert!(text.contains("    spacer { grow: 2 }\n"));
    }

    #[test]
    fn text_and_class_are_set() {
        let label = find(SOURCE, "label");
        let text = checked(set_text(SOURCE, &label, "Say \"hi\" to {name}"));
        assert!(text.contains(r#"label.title "Say \"hi\" to {name}""#));
        let text = checked(set_class(SOURCE, &label, "muted"));
        assert!(text.contains("label.muted \"Hi\""));
        let text = checked(set_class(SOURCE, &label, ""));
        assert!(text.contains("    label \"Hi\""));
        let column = find(SOURCE, "column");
        let text = checked(set_class(SOURCE, &column, "page"));
        assert!(text.starts_with("column.page {"));
    }

    #[test]
    fn elements_are_added_removed_and_swapped() {
        let column = find(SOURCE, "column");
        let text = checked(add_child(SOURCE, &column, "label \"New\""));
        assert!(
            text.ends_with("    spacer\n    label \"New\"\n}\n"),
            "{text}"
        );

        let row = find(SOURCE, "row");
        let text = checked(add_child(SOURCE, &row, "label \"New\""));
        assert!(
            text.contains("{ on-click: go }\n        label \"New\"\n    }\n"),
            "{text}"
        );

        let bare = "column {\n    box\n}\n";
        let text = checked(add_child(bare, &find(bare, "box"), "label \"x\""));
        assert!(
            text.contains("    box {\n        label \"x\"\n    }\n"),
            "{text}"
        );
        let spacer = find(SOURCE, "spacer");

        let label = find(SOURCE, "label");
        let text = checked(remove_element(SOURCE, &label));
        assert!(text.starts_with("column {\n    padding: 12\n    row {"));
        let text = checked(add_after(SOURCE, &label, "label \"Next\""));
        assert!(text.contains("    label.title \"Hi\"\n    label \"Next\"\n    row"));

        let text = checked(swap(SOURCE, &spacer, &label));
        assert!(text.contains("    spacer\n    row {"), "{text}");
        assert!(text.ends_with("    label.title \"Hi\"\n}\n"), "{text}");
    }
}
