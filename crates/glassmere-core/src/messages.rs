//! Localized text. The context holds a `Messages` source that apps install
//! (the `glassmere-fluent` crate provides one for Fluent catalogs); `Ui::tr`
//! looks a key up in it, then in the framework's built-in English, and shows
//! the key itself when neither has it.
use std::fmt::Write as _;

/// A value substituted into a message.
#[derive(Clone, Debug, PartialEq)]
pub enum MessageArg {
    Text(String),
    /// Numbers select plural forms and are formatted for the locale.
    Number(f64),
}

impl From<&str> for MessageArg {
    fn from(text: &str) -> Self {
        Self::Text(text.to_owned())
    }
}

impl From<String> for MessageArg {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<&String> for MessageArg {
    fn from(text: &String) -> Self {
        Self::Text(text.clone())
    }
}

macro_rules! number_arg {
    ($($type:ty),*) => {$(
        impl From<$type> for MessageArg {
            fn from(number: $type) -> Self {
                Self::Number(number as f64)
            }
        }
    )*};
}
number_arg!(f64, f32, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

/// A source of localized text for one locale with its fallbacks.
pub trait Messages {
    /// BCP 47 tag of the locale the texts are in, such as `cs-CZ`.
    fn locale(&self) -> &str;
    /// The text for `key` with `args` substituted; `None` when no catalog has
    /// the key, which makes the caller fall back.
    fn text(&self, key: &str, args: &[(&str, MessageArg)]) -> Option<String>;
}

/// The framework's own texts in English, by key. Every key starts with
/// `glassmere-`; a catalog that defines one overrides it. Arguments are written
/// `{ $name }`, as in Fluent.
pub const BUILTIN_MESSAGES: &[(&str, &str)] = &[
    ("glassmere-toast-dismiss", "Dismiss"),
    ("glassmere-palette", "Commands"),
    ("glassmere-palette-query", "Search commands"),
    ("glassmere-palette-empty", "No command matches."),
    ("glassmere-number-decrease", "Decrease"),
    ("glassmere-number-increase", "Increase"),
    ("glassmere-number-invalid", "Enter a number."),
    ("glassmere-grid-move-left", "Move left"),
    ("glassmere-grid-move-right", "Move right"),
    ("glassmere-grid-wider", "Wider"),
    ("glassmere-grid-narrower", "Narrower"),
    ("glassmere-grid-hide", "Hide { $name }"),
    ("glassmere-grid-show-all", "Show all columns"),
    ("glassmere-grid-reset", "Reset columns"),
    ("glassmere-grid-freeze", "Freeze up to { $name }"),
    ("glassmere-grid-unfreeze", "Unfreeze columns"),
    ("glassmere-grid-resize", "Resize { $name }"),
    ("glassmere-grid-filter", "Filter { $name }"),
    ("glassmere-filter-placeholder", "Filter"),
    ("glassmere-property-column", "Property"),
    ("glassmere-value-column", "Value"),
    ("glassmere-property-expand", "Expand { $name }"),
    ("glassmere-property-collapse", "Collapse { $name }"),
    ("glassmere-property-reset", "Reset { $name }"),
    ("glassmere-splitter", "Splitter"),
    ("glassmere-dock-floating", "Floating panel"),
    ("glassmere-dock-move", "Move floating panel"),
    ("glassmere-dock-resize", "Resize floating panel"),
    ("glassmere-tab-close", "Close { $name }"),
    ("glassmere-find-close", "Close the find bar"),
    ("glassmere-minimize", "Minimize"),
    ("glassmere-maximize", "Maximize"),
    ("glassmere-restore", "Restore"),
    ("glassmere-close", "Close"),
];

/// Substitutes `{ $name }` placeholders, the subset of Fluent the built-in
/// texts use. Unknown names stay as written, which shows the mistake.
pub(crate) fn substitute(pattern: &str, args: &[(&str, MessageArg)]) -> String {
    let mut text = String::with_capacity(pattern.len());
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        text.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            rest = &rest[open..];
            break;
        };
        let inside = rest[open + 1..open + close].trim();
        match inside
            .strip_prefix('$')
            .and_then(|name| args.iter().find(|(known, _)| *known == name.trim()))
        {
            Some((_, MessageArg::Text(value))) => text.push_str(value),
            Some((_, MessageArg::Number(value))) => {
                let _ = write!(text, "{value}");
            }
            None => text.push_str(&rest[open..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    text.push_str(rest);
    text
}

impl crate::Context {
    /// Installs the source of localized text, or removes it with `None`. The
    /// next declaration shows the new texts, so this is also how the language
    /// changes while the app runs.
    pub fn set_messages(&mut self, messages: Option<Box<dyn Messages>>) {
        self.messages = messages;
        self.missing_messages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.repaint_all();
    }

    /// Locale of the installed messages; `en` for the built-in texts alone.
    pub fn locale(&self) -> &str {
        self.messages
            .as_ref()
            .map_or("en", |messages| messages.locale())
    }

    /// The text for `key`: from the installed messages, else the framework's
    /// built-in English, else the key itself, which is then also listed by
    /// `missing_messages`.
    pub fn tr(&self, key: &str, args: &[(&str, MessageArg)]) -> String {
        if let Some(text) = self
            .messages
            .as_ref()
            .and_then(|messages| messages.text(key, args))
        {
            return text;
        }
        if let Some(pattern) = builtin(key) {
            return substitute(pattern, args);
        }
        let mut missing = self
            .missing_messages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A linear scan of at most 256 keys; a set if apps miss more.
        if missing.len() < 256 && !missing.iter().any(|known| known == key) {
            missing.push(key.to_owned());
        }
        key.to_owned()
    }

    /// Keys that were asked for and had no text anywhere, in the order they
    /// were first missed, since the messages were last installed.
    pub fn missing_messages(&self) -> Vec<String> {
        self.missing_messages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl<'a, M: 'static> crate::Ui<'a, M> {
    /// Locale of the installed messages; `en` without any.
    pub fn locale(&self) -> &str {
        self.context.locale()
    }

    /// Localized text for `key`; see `Context::tr`.
    pub fn tr(&self, key: &str) -> String {
        self.context.tr(key, &[])
    }

    /// Localized text with arguments: `ui.tr_args("files", &[("count", 3.into())])`.
    pub fn tr_args(&self, key: &str, args: &[(&str, MessageArg)]) -> String {
        self.context.tr(key, args)
    }
}

pub(crate) fn builtin(key: &str) -> Option<&'static str> {
    BUILTIN_MESSAGES
        .iter()
        .find(|(known, _)| *known == key)
        .map(|(_, text)| *text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_texts_substitute_their_arguments_and_have_unique_prefixed_keys() {
        let name = [("name", MessageArg::from("City"))];
        assert_eq!(
            substitute(builtin("glassmere-grid-hide").unwrap(), &name),
            "Hide City"
        );
        assert_eq!(
            substitute(
                "{$count} of {  $total }",
                &[("count", 3.into()), ("total", 2.5.into())]
            ),
            "3 of 2.5"
        );
        assert_eq!(
            substitute("{ $missing } and { broken", &name),
            "{ $missing } and { broken"
        );
        let mut keys: Vec<&str> = BUILTIN_MESSAGES.iter().map(|(key, _)| *key).collect();
        assert!(keys.iter().all(|key| key.starts_with("glassmere-")));
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), BUILTIN_MESSAGES.len());
    }
}
