//! `tr!(ui, "key", name = value, ...)`: a message lookup whose key and
//! argument names are checked against the default-locale catalog at compile
//! time.
use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use syn::parse::{Parse, ParseStream};
use syn::{Error, Expr, Ident, LitStr, Token};

pub struct Call {
    ui: Expr,
    key: LitStr,
    args: Vec<(Ident, Expr)>,
}

impl Parse for Call {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let ui = input.parse()?;
        input.parse::<Token![,]>()?;
        let key = input.parse()?;
        let mut args = Vec::new();
        while input.parse::<Option<Token![,]>>()?.is_some() && !input.is_empty() {
            let name: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            args.push((name, input.parse()?));
        }
        Ok(Self { ui, key, args })
    }
}

/// The catalog named by `[package.metadata.seaglass] messages = "..."` in the
/// manifest of the crate being compiled, relative to that manifest.
fn catalog() -> Option<PathBuf> {
    let directory = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?);
    let manifest = std::fs::read_to_string(directory.join("Cargo.toml")).ok()?;
    // A line scan instead of a TOML parser; the table has one key.
    let mut inside = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == "[package.metadata.seaglass]";
        } else if inside
            && let Some(value) = line.strip_prefix("messages")
            && let Some(path) = value.trim_start().strip_prefix('=')
        {
            return Some(directory.join(path.trim().trim_matches('"')));
        }
    }
    None
}

/// Variables a message uses, and the messages it refers to.
#[derive(Default)]
struct Uses<'a> {
    variables: Vec<&'a str>,
    messages: Vec<&'a str>,
}

impl<'a> Uses<'a> {
    fn inline(&mut self, expression: &InlineExpression<&'a str>) {
        match expression {
            InlineExpression::VariableReference { id } => self.variables.push(id.name),
            InlineExpression::MessageReference { id, .. } => self.messages.push(id.name),
            InlineExpression::FunctionReference { arguments, .. } => {
                for argument in &arguments.positional {
                    self.inline(argument);
                }
                for argument in &arguments.named {
                    self.inline(&argument.value);
                }
            }
            InlineExpression::TermReference {
                arguments: Some(arguments),
                ..
            } => {
                for argument in &arguments.named {
                    self.inline(&argument.value);
                }
            }
            InlineExpression::Placeable { expression } => self.expression(expression),
            _ => {}
        }
    }

    fn expression(&mut self, expression: &Expression<&'a str>) {
        match expression {
            Expression::Inline(expression) => self.inline(expression),
            Expression::Select { selector, variants } => {
                self.inline(selector);
                for variant in variants {
                    self.pattern(&variant.value);
                }
            }
        }
    }

    fn pattern(&mut self, pattern: &Pattern<&'a str>) {
        for element in &pattern.elements {
            if let PatternElement::Placeable { expression } = element {
                self.expression(expression);
            }
        }
    }
}

/// Every message of a catalog with the variables it needs, including those of
/// the messages it refers to.
fn variables(source: &str) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let resource =
        fluent_syntax::parser::parse(source).map_err(|(_, errors)| errors[0].to_string())?;
    let mut direct = BTreeMap::new();
    for entry in &resource.body {
        if let Entry::Message(message) = entry {
            let mut uses = Uses::default();
            if let Some(value) = &message.value {
                uses.pattern(value);
            }
            direct.insert(message.id.name, uses);
        }
    }
    Ok(direct
        .keys()
        .map(|key| {
            let (mut all, mut pending, mut seen) = (BTreeSet::new(), vec![*key], BTreeSet::new());
            while let Some(next) = pending.pop() {
                if let (true, Some(uses)) = (seen.insert(next), direct.get(next)) {
                    all.extend(uses.variables.iter().map(|name| (*name).to_owned()));
                    pending.extend(&uses.messages);
                }
            }
            ((*key).to_owned(), all)
        })
        .collect())
}

pub fn expand(call: Call) -> syn::Result<TokenStream> {
    let Call { ui, key, args } = call;
    let name = key.value();
    let mut tracked = quote! {};
    // The framework's own keys are not in an app's catalog.
    if let (false, Some(path)) = (name.starts_with("seaglass-"), catalog()) {
        let shown = path.display().to_string();
        let source = std::fs::read_to_string(&path).map_err(|error| {
            Error::new(
                Span::call_site(),
                format!("tr!: cannot read the message catalog {shown}: {error}"),
            )
        })?;
        let messages = variables(&source)
            .map_err(|error| Error::new(Span::call_site(), format!("tr!: {shown}: {error}")))?;
        let Some(wanted) = messages.get(&name) else {
            return Err(Error::new(
                key.span(),
                format!("no message `{name}` in {shown}"),
            ));
        };
        for (argument, _) in &args {
            if !wanted.contains(&argument.to_string()) {
                return Err(Error::new(
                    argument.span(),
                    format!("message `{name}` has no `${argument}`"),
                ));
            }
        }
        if let Some(missing) = wanted
            .iter()
            .find(|wanted| !args.iter().any(|(given, _)| given == wanted.as_str()))
        {
            return Err(Error::new(
                key.span(),
                format!("message `{name}` needs `{missing} = ...`"),
            ));
        }
        // Makes the compiler rebuild this crate when the catalog changes.
        tracked = quote! { const _: &str = include_str!(#shown); };
    }
    let names = args.iter().map(|(name, _)| name.to_string());
    let values = args.iter().map(|(_, value)| value);
    Ok(quote! {{
        #tracked
        #ui.tr_args(#key, &[#((#names, ::core::convert::Into::into(#values))),*])
    }})
}
