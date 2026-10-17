//! `#[derive(Properties)]`: property grid rows from a struct's fields.
//!
//! ```ignore
//! #[derive(Properties)]
//! struct Card {
//!     #[property(group = "Content")]
//!     title: String,
//!     #[property(choices("Start", "Center", "End"))]
//!     align: usize,
//!     #[property(group = "Layout", min = 80.0, max = 420.0, step = 10.0, suffix = " px", default = 260.0)]
//!     width: f64,
//!     #[property(name = "Corner radius", min = 0, max = 60)]
//!     radius: u8,
//!     #[property(group = "Appearance")]
//!     fill: Color,
//!     #[property(flatten)]
//!     shadow: Shadow,          // another `Properties` struct, as a nested group
//!     #[property(readonly)]
//!     revision: u32,
//!     #[property(with = edit_anchor)]   // see below
//!     anchor: Anchor,
//!     #[property(skip)]
//!     cache: Vec<u8>,
//! }
//! ```
//!
//! Field types decide the editor: `bool`, `String`, `Color`, the number types
//! (integers step by one and keep their type's range), and `usize` with
//! `choices(...)`. `group` stays in force for the fields that follow until
//! another one is given; `group = ""` returns to the top level. Names default
//! to the field name with spaces and a capital.
//!
//! It generates `CardMsg`, with a variant per editable field, `Card::apply`,
//! which stores one, and `Card::properties`, which declares the rows. A `flatten` field is a nested group whose
//! variant wraps the inner struct's own message (`ShadowMsg`, which must be
//! derived beside it in the same module path). A `with` field is declared
//! through `PropertyGrid::custom` by a function of your own that sends the
//! new value as a message:
//!
//! ```ignore
//! fn edit_anchor<M: 'static>(
//!     ui: &mut Ui<'_, M>,
//!     label: &str,
//!     anchor: &Anchor,
//!     on: impl Fn(Anchor) -> M + 'static,
//! ) { /* declare widgets, `on(next)` as their messages */ }
//! ```
//!
//! A struct with generics cannot derive it.
use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Error, Expr, Fields, LitStr, Path, Type, parse_macro_input};

mod sheet;
mod tr;

/// A stylesheet parsed while the crate compiles:
/// `style_sheet!("styles/app.style")`.
///
/// The path is relative to the crate's manifest. An unknown property, a
/// value the parser cannot read and an unclosed rule are compile errors that
/// name the line, and editing the file rebuilds the crate. It expands to a
/// `StyleSheet`, already parsed, that cannot fail at start.
#[proc_macro]
pub fn style_sheet(input: TokenStream) -> TokenStream {
    let path = parse_macro_input!(input as LitStr);
    sheet::expand(path)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// A localized text whose key and arguments are checked while compiling:
/// `tr!(ui, "files-selected", count = files.len())`.
///
/// Name the default-locale catalog in the manifest of the crate that uses it:
///
/// ```toml
/// [package.metadata.seaglass]
/// messages = "locales/en.ftl"
/// ```
///
/// An unknown key, an argument the message does not use and a variable the
/// call does not pass are compile errors, and editing the catalog rebuilds
/// the crate. Without that entry the macro checks nothing. Keys starting with
/// `seaglass-` are the framework's own and are not checked. It expands to
/// `ui.tr_args(key, &[(name, value.into()), ...])`.
#[proc_macro]
pub fn tr(input: TokenStream) -> TokenStream {
    let call = parse_macro_input!(input as tr::Call);
    tr::expand(call)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Properties, attributes(property))]
pub fn properties(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

#[derive(Default)]
struct Options {
    name: Option<String>,
    group: Option<String>,
    skip: bool,
    readonly: bool,
    flatten: bool,
    with: Option<Path>,
    default: Option<Expr>,
    min: Option<Expr>,
    max: Option<Expr>,
    step: Option<Expr>,
    decimals: Option<Expr>,
    suffix: Option<String>,
    choices: Vec<String>,
}

fn options(field: &syn::Field) -> syn::Result<Options> {
    let mut options = Options::default();
    for attribute in &field.attrs {
        if !attribute.path().is_ident("property") {
            continue;
        }
        attribute.parse_nested_meta(|meta| {
            let text = |meta: &syn::meta::ParseNestedMeta<'_>| {
                Ok::<_, Error>(meta.value()?.parse::<LitStr>()?.value())
            };
            let key = meta
                .path
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default();
            match key.as_str() {
                "name" => options.name = Some(text(&meta)?),
                "group" => options.group = Some(text(&meta)?),
                "suffix" => options.suffix = Some(text(&meta)?),
                "skip" => options.skip = true,
                "readonly" => options.readonly = true,
                "flatten" => options.flatten = true,
                "with" => options.with = Some(meta.value()?.parse()?),
                "default" => options.default = Some(meta.value()?.parse()?),
                "min" => options.min = Some(meta.value()?.parse()?),
                "max" => options.max = Some(meta.value()?.parse()?),
                "step" => options.step = Some(meta.value()?.parse()?),
                "decimals" => options.decimals = Some(meta.value()?.parse()?),
                "choices" => {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    options.choices = content
                        .parse_terminated(<LitStr as syn::parse::Parse>::parse, syn::Token![,])?
                        .iter()
                        .map(LitStr::value)
                        .collect();
                }
                _ => {
                    return Err(meta.error(
                        "unknown property option; expected name, group, default, min, max, step, \
                         decimals, suffix, choices(...), readonly, flatten, with or skip",
                    ));
                }
            }
            Ok(())
        })?;
    }
    Ok(options)
}

/// `corner_radius` becomes `Corner radius`.
fn title(field: &str) -> String {
    let spaced = field.trim_start_matches("r#").replace('_', " ");
    let mut characters = spaced.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => spaced,
    }
}

const INTEGERS: [&str; 12] = [
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
];

fn expand(input: &DeriveInput) -> syn::Result<Tokens> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            input,
            "Properties can be derived for structs with named fields",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new_spanned(
            input,
            "Properties can be derived for structs with named fields",
        ));
    };
    if !input.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &input.generics,
            "Properties cannot be derived for a struct with generics",
        ));
    }
    let name = &input.ident;
    let message = format_ident!("{}Msg", name);
    // Consecutive fields of one group share one `grid.group` call.
    let mut message_groups: Vec<(String, Vec<Tokens>)> = Vec::new();
    let mut variants = Vec::new();
    let mut arms = Vec::new();
    let mut current = String::new();
    for field in &fields.named {
        let options = options(field)?;
        if let Some(group) = &options.group {
            current.clone_from(group);
        }
        if options.skip {
            continue;
        }
        let (carried, row) = message_row(field, &options, &message)?;
        if let Some((variant, ty, arm)) = carried {
            variants.push(quote! { #variant(#ty) });
            arms.push(arm);
        }
        push_row(&mut message_groups, &current, row);
    }
    let message_body = bodies(&message_groups);
    let doc = format!(
        "What the property grid of `{name}` sends: one variant per editable property, with its new value."
    );
    let declares = format!(
        "Declares the rows of `{name}` in `Ui::property_grid`'s grid: each edit sends the `{message}` of its property through `on`, which `apply` stores."
    );
    Ok(quote! {
            #[doc = #doc]
            #[derive(Clone, Debug, PartialEq)]
            pub enum #message {
                #(#variants,)*
            }

            impl #name {
                /// Stores a change the property grid sent.
                pub fn apply(&mut self, message: #message) {
                    match message {
                        #(#arms)*
                    }
                }

                #[doc = #declares]
                #[allow(unused_variables)]
                pub fn properties<M: 'static>(
                    &self,
                    grid: &mut ::seaglass::PropertyGrid<'_, '_, M>,
                    on: impl Fn(#message) -> M + 'static,
                ) {
                    let __on = ::std::rc::Rc::new(on);
                    #(#message_body)*
                }
            }
    })
}

fn push_row(groups: &mut Vec<(String, Vec<Tokens>)>, current: &str, row: Tokens) {
    match groups.last_mut() {
        Some((group, rows)) if group.as_str() == current => rows.push(row),
        _ => groups.push((current.to_owned(), vec![row])),
    }
}

fn bodies(groups: &[(String, Vec<Tokens>)]) -> Vec<Tokens> {
    groups
        .iter()
        .map(|(group, rows)| {
            if group.is_empty() {
                quote! { #(#rows)* }
            } else {
                quote! { grid.group(#group, |grid| { #(#rows)* }); }
            }
        })
        .collect()
}

/// The `NumberOptions` a number field declares. Integers keep their own
/// range unless a narrower one is given; all numbers travel through the grid
/// as `f64`.
fn number_options(options: &Options, kind: &str) -> Tokens {
    let integer = kind != "f64" && kind != "f32";
    let ty = format_ident!("{}", kind);
    let min = match (&options.min, integer) {
        (Some(min), _) => quote! { (#min) as f64 },
        (None, true) => quote! { <#ty>::MIN as f64 },
        (None, false) => quote! { f64::NEG_INFINITY },
    };
    let max = match (&options.max, integer) {
        (Some(max), _) => quote! { (#max) as f64 },
        (None, true) => quote! { <#ty>::MAX as f64 },
        (None, false) => quote! { f64::INFINITY },
    };
    let step = match &options.step {
        Some(step) => quote! { (#step) as f64 },
        None => quote! { 1.0 },
    };
    let decimals = match (&options.decimals, integer) {
        (Some(decimals), _) => quote! { .decimals((#decimals) as usize) },
        (None, true) => quote! { .decimals(0) },
        (None, false) => quote! {},
    };
    let suffix = options
        .suffix
        .as_ref()
        .map(|suffix| quote! { .suffix(#suffix) });
    quote! { ::seaglass::NumberOptions::default().range(#min, #max).step(#step) #decimals #suffix }
}

/// A field's variant name: `corner_radius` becomes `CornerRadius`.
fn variant_name(ident: &syn::Ident) -> syn::Ident {
    let text = ident.to_string();
    let camel: String = text
        .trim_start_matches("r#")
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().chain(characters).collect::<String>(),
                None => String::new(),
            }
        })
        .collect();
    format_ident!("{}", camel)
}

/// A message variant, the type it carries and the `apply` arm that stores it.
type Carried = (syn::Ident, Tokens, Tokens);

/// The message form of one row. It declares the editor with a closure that
/// sends the field's message variant, and returns that variant with the field
/// and its type; a read-only row carries nothing.
fn message_row(
    field: &syn::Field,
    options: &Options,
    message: &syn::Ident,
) -> syn::Result<(Option<Carried>, Tokens)> {
    let ident = field.ident.as_ref().unwrap();
    let ty = &field.ty;
    let name = options
        .name
        .clone()
        .unwrap_or_else(|| title(&ident.to_string()));
    let variant = variant_name(ident);
    let send =
        quote! { { let __on = __on.clone(); move |__value| (*__on)(#message::#variant(__value)) } };
    if options.flatten {
        let Type::Path(path) = ty else {
            return Err(Error::new_spanned(
                ty,
                "`flatten` needs a path to a struct that derives Properties",
            ));
        };
        // `Shadow` or `shapes::Shadow` names `ShadowMsg` beside it.
        let inner: Tokens = format!("{}Msg", quote! { #path }.to_string().replace(' ', ""))
            .parse()
            .map_err(|_| Error::new_spanned(ty, "`flatten` needs a plain path to the struct"))?;
        return Ok((
            Some((
                variant.clone(),
                inner,
                quote! { #message::#variant(__value) => self.#ident.apply(__value), },
            )),
            quote! { grid.group(#name, |grid| self.#ident.properties(grid, #send)); },
        ));
    }
    let carried = Some((
        variant.clone(),
        quote! { #ty },
        quote! { #message::#variant(__value) => self.#ident = __value, },
    ));
    if options.readonly {
        return Ok((
            None,
            quote! { grid.readonly(#name, ::std::format!("{}", self.#ident)); },
        ));
    }
    if let Some(with) = &options.with {
        return Ok((
            carried,
            quote! { grid.custom(#name, |ui, label| #with(ui, label, &self.#ident, #send)); },
        ));
    }
    let kind = match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    };
    let numeric = kind == "f64" || kind == "f32" || INTEGERS.contains(&kind.as_str());
    let number_only = [
        (&options.min, "min"),
        (&options.max, "max"),
        (&options.step, "step"),
        (&options.decimals, "decimals"),
    ];
    if let Some((_, option)) = number_only
        .iter()
        .find(|(value, _)| value.is_some() && !numeric)
    {
        return Err(Error::new_spanned(
            field,
            format!("`{option}` applies to number fields only"),
        ));
    }
    if options.suffix.is_some() && !numeric {
        return Err(Error::new_spanned(
            field,
            "`suffix` applies to number fields only",
        ));
    }
    if !options.choices.is_empty() {
        if kind != "usize" {
            return Err(Error::new_spanned(
                field,
                "`choices(...)` needs a `usize` field holding the chosen index",
            ));
        }
        let choices = &options.choices;
        let default = options
            .default
            .as_ref()
            .map(|default| quote! { grid.default::<usize>(#default); });
        return Ok((
            carried,
            quote! {
                #default
                grid.choice_with(#name, self.#ident, &[#(#choices),*], #send);
            },
        ));
    }
    if numeric {
        let number = number_options(options, &kind);
        let default = options
            .default
            .as_ref()
            .map(|default| quote! { grid.default::<f64>((#default) as f64); });
        let send = quote! { { let __on = __on.clone(); move |__value: f64| (*__on)(#message::#variant(__value as #ty)) } };
        return Ok((
            carried,
            quote! {
                #default
                grid.number_with(#name, self.#ident as f64, #number, #send);
            },
        ));
    }
    let (row, default) = match kind.as_str() {
        "bool" => (
            quote! { grid.bool_with(#name, self.#ident, #send); },
            options
                .default
                .as_ref()
                .map(|default| quote! { grid.default::<bool>(#default); }),
        ),
        "String" => (
            quote! { grid.text_with(#name, &self.#ident, #send); },
            options.default.as_ref().map(
                |default| quote! { grid.default::<::std::string::String>(::std::string::String::from(#default)); },
            ),
        ),
        "Color" => (
            quote! { grid.color_with(#name, self.#ident, #send); },
            options
                .default
                .as_ref()
                .map(|default| quote! { grid.default::<::seaglass::Color>(#default); }),
        ),
        _ => {
            return Err(Error::new_spanned(
                &field.ty,
                "no property editor for this type; add #[property(skip)], #[property(flatten)] for \
                 a nested Properties struct, #[property(readonly)], or #[property(with = function)]",
            ));
        }
    };
    Ok((carried, quote! { #default #row }))
}
