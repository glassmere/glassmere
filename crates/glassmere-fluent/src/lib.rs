//! Fluent catalogs as a `glassmere_core::Messages` source.
//!
//! ```ignore
//! let mut messages = FluentMessages::new("cs-CZ", "en")?;   // tries cs-CZ, cs, en
//! messages.load_dir("locales")?;                           // locales/cs.ftl, locales/en/*.ftl, ...
//! context.set_messages(Some(Box::new(messages)));
//! ui.label(ui.tr_args("files-deleted", &[("count", 3.into())]));
//! ```
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use glassmere_core::{MessageArg, Messages};
use std::path::{Path, PathBuf};
use unic_langid::LanguageIdentifier;

/// A catalog that could not be read or parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FluentError {
    pub locale: String,
    pub file: Option<PathBuf>,
    /// Line of the first problem, from one; zero when it is not about a line.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for FluentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.file {
            Some(file) => write!(f, "{}:{}: {}", file.display(), self.line, self.message),
            None => write!(f, "{} line {}: {}", self.locale, self.line, self.message),
        }
    }
}

impl std::error::Error for FluentError {}

/// Translations of the framework's own texts that ship with this crate, by
/// language. English is built into the core.
pub const FRAMEWORK_CATALOGS: &[(&str, &str)] = &[
    ("cs", include_str!("../locales/cs.ftl")),
    ("de", include_str!("../locales/de.ftl")),
];

/// Messages for one locale with its fallbacks. A key is looked up in the
/// locale itself, then in its bare language, then in the fallback locale.
pub struct FluentMessages {
    locale: String,
    chain: Vec<(String, FluentBundle<FluentResource>)>,
}

impl FluentMessages {
    /// `locale` and `fallback` are BCP 47 tags such as `cs-CZ` and `en`.
    pub fn new(locale: &str, fallback: &str) -> Result<Self, FluentError> {
        let mut tags: Vec<String> = Vec::new();
        for tag in [
            locale,
            locale.split(['-', '_']).next().unwrap_or(locale),
            fallback,
        ] {
            let tag = tag.replace('_', "-");
            if !tags.iter().any(|known| known.eq_ignore_ascii_case(&tag)) {
                tags.push(tag);
            }
        }
        let chain = tags
            .into_iter()
            .map(|tag| {
                let language: LanguageIdentifier = tag.parse().map_err(|_| FluentError {
                    locale: tag.clone(),
                    file: None,
                    line: 0,
                    message: format!("`{tag}` is not a language tag"),
                })?;
                let mut bundle = FluentBundle::new_concurrent(vec![language]);
                // Fluent wraps arguments in directional isolates by default.
                // They are right for mixed-direction text and show as boxes
                // in fonts that lack them; see `isolating`.
                bundle.set_use_isolating(false);
                Ok((tag, bundle))
            })
            .collect::<Result<_, FluentError>>()?;
        let mut messages = Self {
            locale: locale.replace('_', "-"),
            chain,
        };
        // The framework's own controls speak the language too. These go in
        // first, so anything the app adds for the same key wins.
        for (language, source) in FRAMEWORK_CATALOGS {
            messages.add(language, source)?;
        }
        Ok(messages)
    }

    /// Wraps substituted arguments in Unicode directional isolates, which
    /// keeps a left-to-right name intact inside right-to-left text.
    pub fn isolating(mut self, isolating: bool) -> Self {
        for (_, bundle) in &mut self.chain {
            bundle.set_use_isolating(isolating);
        }
        self
    }

    /// The locales that are tried, in order.
    pub fn chain(&self) -> impl Iterator<Item = &str> {
        self.chain.iter().map(|(tag, _)| tag.as_str())
    }

    /// Adds a catalog for one locale of the chain; later catalogs override
    /// earlier ones key by key. Returns false, and adds nothing, for a locale
    /// that is not in the chain, so a loader can offer every file it has.
    pub fn add(&mut self, locale: &str, source: &str) -> Result<bool, FluentError> {
        self.add_from(locale, source, None)
    }

    fn add_from(
        &mut self,
        locale: &str,
        source: &str,
        file: Option<&Path>,
    ) -> Result<bool, FluentError> {
        let locale = locale.replace('_', "-");
        let Some((tag, bundle)) = self
            .chain
            .iter_mut()
            .find(|(tag, _)| tag.eq_ignore_ascii_case(&locale))
        else {
            return Ok(false);
        };
        let resource = FluentResource::try_new(source.to_owned()).map_err(|(_, errors)| {
            let first = &errors[0];
            FluentError {
                locale: tag.clone(),
                file: file.map(Path::to_owned),
                // Where the broken entry begins, not where the parser gave up.
                line: {
                    let at = first
                        .slice
                        .as_ref()
                        .map_or(first.pos.start, |slice| slice.start);
                    source[..at.min(source.len())].matches('\n').count() + 1
                },
                message: first.to_string(),
            }
        })?;
        bundle.add_resource_overriding(resource);
        Ok(true)
    }

    /// Loads, for every locale of the chain, `<dir>/<locale>.ftl` and every
    /// `.ftl` file in `<dir>/<locale>/`, in name order. Missing files and
    /// folders are fine; returns how many files were loaded.
    pub fn load_dir(&mut self, dir: impl AsRef<Path>) -> Result<usize, FluentError> {
        let dir = dir.as_ref();
        let mut loaded = 0;
        for tag in self.chain().map(str::to_owned).collect::<Vec<_>>() {
            let mut files = vec![dir.join(format!("{tag}.ftl"))];
            if let Ok(entries) = std::fs::read_dir(dir.join(&tag)) {
                let mut inside: Vec<PathBuf> = entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().is_some_and(|extension| extension == "ftl"))
                    .collect();
                inside.sort();
                files.extend(inside);
            }
            for file in files {
                let Ok(source) = std::fs::read_to_string(&file) else {
                    continue;
                };
                loaded += usize::from(self.add_from(&tag, &source, Some(&file))?);
            }
        }
        Ok(loaded)
    }
}

impl Messages for FluentMessages {
    fn locale(&self) -> &str {
        &self.locale
    }

    fn text(&self, key: &str, args: &[(&str, MessageArg)]) -> Option<String> {
        let (bundle, pattern) = self
            .chain
            .iter()
            .find_map(|(_, bundle)| Some((bundle, bundle.get_message(key)?.value()?)))?;
        let mut fluent = FluentArgs::new();
        for (name, value) in args {
            fluent.set(
                *name,
                match value {
                    MessageArg::Text(text) => FluentValue::from(text.as_str()),
                    MessageArg::Number(number) => FluentValue::from(*number),
                },
            );
        }
        // Formatting problems (an argument the app did not pass) leave their
        // mark in the text, which is where they are looked for.
        let mut errors = Vec::new();
        Some(
            bundle
                .format_pattern(pattern, Some(&fluent), &mut errors)
                .into_owned(),
        )
    }
}
