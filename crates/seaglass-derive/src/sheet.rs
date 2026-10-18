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
    if let Err(problem) = seaglass_core::StyleSheet::parse(&text) {
        return Err(Error::new(
            path.span(),
            format!("{relative}:{}: {}", problem.line, problem.message),
        ));
    }
    let full = full.to_string_lossy().replace('\\', "/");
    // `include_str!` again, so that editing the file rebuilds the crate.
    Ok(quote! {
        ::seaglass::StyleSheet::parse(include_str!(#full))
            .expect("the stylesheet was parsed while compiling")
    })
}
