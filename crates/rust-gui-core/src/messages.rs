//! Localized text. The context holds a `Messages` source that apps install
//! (the `rust-gui-fluent` crate provides one for Fluent catalogs); `Ui::tr`
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
/// `rust-gui-`; a catalog that defines one overrides it. Arguments are written
/// `{ $name }`, as in Fluent.
pub const BUILTIN_MESSAGES: &[(&str, &str)] = &[
    ("rust-gui-toast-dismiss", "Dismiss"),
    ("rust-gui-palette", "Commands"),
    ("rust-gui-palette-query", "Search commands"),
    ("rust-gui-palette-empty", "No command matches."),
    ("rust-gui-number-decrease", "Decrease"),
    ("rust-gui-number-increase", "Increase"),
    ("rust-gui-number-invalid", "Enter a number."),
    ("rust-gui-grid-move-left", "Move left"),
    ("rust-gui-grid-move-right", "Move right"),
    ("rust-gui-grid-wider", "Wider"),
    ("rust-gui-grid-narrower", "Narrower"),
    ("rust-gui-grid-hide", "Hide { $name }"),
    ("rust-gui-grid-show-all", "Show all columns"),
    ("rust-gui-grid-reset", "Reset columns"),
    ("rust-gui-grid-freeze", "Freeze up to { $name }"),
    ("rust-gui-grid-unfreeze", "Unfreeze columns"),
    ("rust-gui-grid-resize", "Resize { $name }"),
    ("rust-gui-grid-filter", "Filter { $name }"),
    ("rust-gui-filter-placeholder", "Filter"),
    ("rust-gui-property-column", "Property"),
    ("rust-gui-value-column", "Value"),
    ("rust-gui-property-expand", "Expand { $name }"),
    ("rust-gui-property-collapse", "Collapse { $name }"),
    ("rust-gui-property-reset", "Reset { $name }"),
    ("rust-gui-splitter", "Splitter"),
    ("rust-gui-dock-floating", "Floating panel"),
    ("rust-gui-dock-move", "Move floating panel"),
    ("rust-gui-dock-resize", "Resize floating panel"),
    ("rust-gui-tab-close", "Close { $name }"),
    ("rust-gui-find-close", "Close the find bar"),
    ("rust-gui-minimize", "Minimize"),
    ("rust-gui-maximize", "Maximize"),
    ("rust-gui-restore", "Restore"),
    ("rust-gui-close", "Close"),
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
        // ponytail: a linear scan of at most 256 keys; a set if apps miss more.
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
            substitute(builtin("rust-gui-grid-hide").unwrap(), &name),
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
        assert!(keys.iter().all(|key| key.starts_with("rust-gui-")));
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), BUILTIN_MESSAGES.len());
    }
}
