//! The public surface, written down.
//!
//! An application's code is written against these signatures, so changing
//! one is a decision rather than an accident. The snapshot is the record:
//! any change to what the crates export shows up as a diff in
//! `tests/snapshots/public-api.snap`, to be accepted deliberately with
//! `UPDATE_SNAPSHOTS=1` and explained in the release notes.
//!
//! What is recorded is what an application can name: public items and the
//! public methods on them, with their signatures, from every crate an
//! application depends on. Private items, bodies and documentation are not.

use glassmere_testing::assert_snapshot;
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

const SNAPSHOTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots");

/// The crates an application writes against, in the order it meets them.
const CRATES: [&str; 11] = [
    "glassmere",
    "glassmere-core",
    "glassmere-scene",
    "glassmere-text",
    "glassmere-render-wgpu",
    "glassmere-render-cpu",
    "glassmere-winit",
    "glassmere-testing",
    "glassmere-devtools",
    "glassmere-image",
    "glassmere-win32",
];

#[test]
fn the_public_surface_is_what_was_recorded() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates directory")
        .to_path_buf();
    let mut lines = Vec::new();
    for name in CRATES {
        let entry = root.join(name).join("src/lib.rs");
        let mut found = Vec::new();
        read(&entry, &mut found);
        found.sort();
        found.dedup();
        for line in found {
            lines.push(format!("{name} {line}"));
        }
    }
    let mut text = String::new();
    for line in &lines {
        let _ = writeln!(text, "{line}");
    }
    assert_snapshot(SNAPSHOTS, "public-api", &text);
}

/// Reads one file of a crate and the modules it declares. Only the file's
/// own text is parsed: `cfg` and features are ignored, so everything an
/// application could reach under any feature is recorded.
fn read(path: &Path, found: &mut Vec<String>) {
    let Ok(source) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(file) = syn::parse_file(&source) else {
        panic!("{} could not be parsed", path.display());
    };
    for item in &file.items {
        items(item, found);
        // Modules of their own, as files beside or below this one.
        if let syn::Item::Mod(declared) = item
            && declared.content.is_none()
        {
            let name = declared.ident.to_string();
            // `lib.rs` and `mod.rs` keep their modules beside them; any other
            // `x.rs` keeps them in the directory `x/`.
            let parent = path.parent().unwrap_or(Path::new("."));
            let owner = path.file_stem().and_then(|stem| stem.to_str());
            let directory = match owner {
                Some("lib" | "mod" | "main") | None => parent.to_path_buf(),
                Some(stem) => parent.join(stem),
            };
            let beside = directory.join(format!("{name}.rs"));
            let below = directory.join(&name).join("mod.rs");
            let nested = if beside.exists() { beside } else { below };
            read(&nested, found);
        }
    }
}

/// One item, and the public methods of an `impl` block.
fn items(item: &syn::Item, found: &mut Vec<String>) {
    let public = |visibility: &syn::Visibility| matches!(visibility, syn::Visibility::Public(_));
    match item {
        syn::Item::Struct(declared) if public(&declared.vis) => {
            found.push(format!("struct {}", declared.ident));
            for field in &declared.fields {
                if let Some(name) = &field.ident
                    && public(&field.vis)
                {
                    found.push(format!("  field {}::{name}", declared.ident));
                }
            }
        }
        syn::Item::Enum(declared) if public(&declared.vis) => {
            found.push(format!("enum {}", declared.ident));
            for variant in &declared.variants {
                found.push(format!("  variant {}::{}", declared.ident, variant.ident));
            }
        }
        syn::Item::Trait(declared) if public(&declared.vis) => {
            found.push(format!("trait {}", declared.ident));
            for inside in &declared.items {
                if let syn::TraitItem::Fn(method) = inside {
                    found.push(format!(
                        "  fn {}::{}",
                        declared.ident,
                        signature(&method.sig)
                    ));
                }
            }
        }
        syn::Item::Fn(declared) if public(&declared.vis) => {
            found.push(format!("fn {}", signature(&declared.sig)));
        }
        syn::Item::Const(declared) if public(&declared.vis) => {
            found.push(format!("const {}", declared.ident));
        }
        syn::Item::Type(declared) if public(&declared.vis) => {
            found.push(format!("type {}", declared.ident));
        }
        syn::Item::Use(declared) if public(&declared.vis) => {
            found.push(format!("use {}", one_line(&declared.tree)));
        }
        syn::Item::Mod(declared) if declared.content.is_some() => {
            if let Some((_, inside)) = &declared.content {
                for item in inside {
                    items(item, found);
                }
            }
        }
        syn::Item::Impl(block) => {
            let owner = owner(block);
            for inside in &block.items {
                if let syn::ImplItem::Fn(method) = inside
                    && public(&method.vis)
                {
                    found.push(format!("  fn {owner}::{}", signature(&method.sig)));
                }
            }
        }
        _ => {}
    }
}

/// What an `impl` block is for, as written.
fn owner(block: &syn::ItemImpl) -> String {
    let text = one_line(&block.self_ty);
    match &block.trait_ {
        // Trait implementations add no name an application calls by; they
        // are recorded so that losing one shows up.
        Some((_, path, _)) => format!("{text} as {}", one_line(path)),
        None => text,
    }
}

/// A signature without its body, on one line.
fn signature(signature: &syn::Signature) -> String {
    one_line(signature).replacen("fn ", "", 1)
}

fn one_line(tokens: &impl quote::ToTokens) -> String {
    let text = quote::quote!(#tokens).to_string();
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() && !matches!(character, ',' | ')' | '>' | ';') {
            out.push(' ');
        }
        space = false;
        out.push(character);
    }
    out
}
