//! `style_sheet!`: a stylesheet parsed while the crate compiles.

use proc_macro2::TokenStream as Tokens;
use quote::quote;
use syn::{Error, LitStr};

/// Reads the file and parses it with the framework's own parser, so an
/// unknown property or a broken rule is a compile error rather than a
/// surprise at start.
pub(crate) fn expand(path: LitStr) -> Result<Tokens, Error> {
    let relative = path.value();
    let root = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| Error::new(path.span(), "CARGO_MANIFEST_DIR is not set"))?;
    let full = std::path::Path::new(&root).join(&relative);
    let text = std::fs::read_to_string(&full).map_err(|error| {
        Error::new(
            path.span(),
            format!("{} could not be read: {error}", full.display()),
        )
    })?;
    if let Err(problem) = glassmere_core::StyleSheet::parse(&text) {
        return Err(Error::new(
            path.span(),
            format!("{relative}:{}: {}", problem.line, problem.message),
        ));
    }
    let full = full.to_string_lossy().replace('\\', "/");
    // `include_str!` again, so that editing the file rebuilds the crate.
    Ok(quote! {
        ::glassmere::StyleSheet::parse(include_str!(#full))
            .expect("the stylesheet was parsed while compiling")
    })
}

#[cfg(test)]
mod tests {
    use super::expand;
    use proc_macro2::Span;
    use syn::LitStr;

    /// Expands `style_sheet!` on a file holding `text`: the compile error it
    /// would be, as text, or nothing when the sheet is accepted.
    fn expand_text(name: &str, text: &str) -> Result<(), String> {
        let path = std::env::temp_dir().join(format!(
            "glassmere-derive-{}-{name}.style",
            std::process::id()
        ));
        std::fs::write(&path, text).unwrap();
        let literal = LitStr::new(&path.to_string_lossy(), Span::call_site());
        let result = expand(literal);
        std::fs::remove_file(&path).ok();
        result.map(|_| ()).map_err(|error| error.to_string())
    }

    #[test]
    fn variables_and_nested_rules_are_accepted() {
        let text = "--brand: #1a9c8f\ntoken accent var(--brand)\n\
                    class card {\n  background: var(--brand, #000)\n  &:hover { background: #22b0a0 }\n  \
                    &.selected { &:pressed { opacity: 0.5 } }\n}";
        assert_eq!(expand_text("accepted", text), Ok(()));
    }

    #[test]
    fn a_misused_variable_or_nested_rule_is_an_error_naming_the_line() {
        let error = expand_text(
            "unknown",
            "class card {\n  padding: 4\n  radius: var(--r)\n}",
        )
        .unwrap_err();
        assert!(
            error.ends_with(":3: unknown variable `--r`; variables must be declared before use"),
            "{error}"
        );
        let error = expand_text("cycle", "\n--a: var(--a)").unwrap_err();
        assert!(
            error.ends_with(":2: variable `--a` refers to itself"),
            "{error}"
        );
        let error =
            expand_text("selector", "class card {\n  .title { padding: 4 }\n}").unwrap_err();
        assert!(
            error.contains(":2: nested rule `.title` is not supported"),
            "{error}"
        );
        let error = expand_text(
            "unclosed",
            "class card {\n  &:hover {\n    opacity: 0.5\n  }",
        )
        .unwrap_err();
        assert!(
            error.ends_with(":4: class `card` is missing `}`"),
            "{error}"
        );
    }
}
