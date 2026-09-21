use rust_gui_core::{Context, Messages, MonospaceMeasurer};
use rust_gui_fluent::FluentMessages;
use rust_gui_scene::Size;

const ENGLISH: &str = "
greeting = Hello, { $name }!
files = { $count ->
    [one] { $count } file
   *[other] { $count } files
}
only-english = Only in English
";

const CZECH: &str = "
greeting = Ahoj, { $name }!
files = { $count ->
    [one] { $count } soubor
    [few] { $count } soubory
   *[other] { $count } souborů
}
";

fn czech() -> FluentMessages {
    let mut messages = FluentMessages::new("cs-CZ", "en").unwrap();
    assert_eq!(messages.chain().collect::<Vec<_>>(), ["cs-CZ", "cs", "en"]);
    assert!(messages.add("cs", CZECH).unwrap());
    assert!(messages.add("en", ENGLISH).unwrap());
    assert!(
        !messages.add("de", "greeting = Hallo").unwrap(),
        "not in the chain"
    );
    messages
}

#[test]
fn plurals_follow_the_language_and_missing_keys_fall_back_along_the_chain() {
    let messages = czech();
    assert_eq!(messages.locale(), "cs-CZ");
    let files = |count: i32| messages.text("files", &[("count", count.into())]).unwrap();
    assert_eq!(files(1), "1 soubor");
    assert_eq!(files(3), "3 soubory");
    assert_eq!(files(5), "5 souborů");
    assert_eq!(
        messages
            .text("greeting", &[("name", "Ado".into())])
            .unwrap(),
        "Ahoj, Ado!"
    );
    // Not translated yet: the fallback locale has it.
    assert_eq!(
        messages.text("only-english", &[]).unwrap(),
        "Only in English"
    );
    assert_eq!(messages.text("nowhere", &[]), None);

    // A regional catalog overrides its language key by key.
    let mut messages = czech();
    messages
        .add("cs-CZ", "greeting = Nazdar, { $name }!")
        .unwrap();
    assert_eq!(
        messages
            .text("greeting", &[("name", "Ado".into())])
            .unwrap(),
        "Nazdar, Ado!"
    );
    assert_eq!(files_of(&messages, 2), "2 soubory");
}

fn files_of(messages: &FluentMessages, count: i32) -> String {
    messages.text("files", &[("count", count.into())]).unwrap()
}

#[test]
fn a_context_shows_catalog_text_then_built_in_english_then_the_key() {
    let mut context = Context::new();
    context.set_messages(Some(Box::new(czech())));
    assert_eq!(context.locale(), "cs-CZ");
    let mut shown = Vec::new();
    context.run(Size::new(400.0, 300.0), &mut MonospaceMeasurer, |ui| {
        shown.push(ui.tr_args("files", &[("count", 2.into())]));
        shown.push(ui.tr("rust-gui-toast-dismiss"));
        shown.push(ui.tr("not-written-yet"));
        shown.push(ui.tr("not-written-yet"));
    });
    // The framework's own text comes from the shipped Czech catalog.
    assert_eq!(
        shown,
        ["2 soubory", "Zavřít", "not-written-yet", "not-written-yet"]
    );
    // A language without one falls back to the built-in English.
    let polish = FluentMessages::new("pl", "pl").unwrap();
    assert_eq!(polish.text("rust-gui-toast-dismiss", &[]), None);
    let mut other = Context::new();
    other.set_messages(Some(Box::new(polish)));
    assert_eq!(other.tr("rust-gui-toast-dismiss", &[]), "Dismiss");
    assert_eq!(context.missing_messages(), ["not-written-yet"]);

    // The language changes while the app runs; a catalog may restate a built-in key.
    let mut english = FluentMessages::new("en", "en").unwrap();
    english.add("en", ENGLISH).unwrap();
    english.add("en", "rust-gui-toast-dismiss = Close").unwrap();
    context.set_messages(Some(Box::new(english)));
    assert_eq!(context.tr("files", &[("count", 1.into())]), "1 file");
    assert_eq!(context.tr("rust-gui-toast-dismiss", &[]), "Close");
    assert!(
        context.missing_messages().is_empty(),
        "a new language starts a new list"
    );
}

#[test]
fn a_broken_catalog_names_its_line_and_directories_load_by_locale() {
    let mut messages = FluentMessages::new("cs", "en").unwrap();
    let error = messages
        .add("cs", "fine = ok\n\nbroken = { $count ->\n")
        .unwrap_err();
    assert_eq!((error.locale.as_str(), error.line), ("cs", 3));
    assert!(FluentMessages::new("not a tag!", "en").is_err());

    let directory = std::env::temp_dir().join(format!("rust-gui-fluent-{}", std::process::id()));
    std::fs::create_dir_all(directory.join("cs")).unwrap();
    std::fs::write(directory.join("en.ftl"), ENGLISH).unwrap();
    std::fs::write(directory.join("cs").join("10-main.ftl"), CZECH).unwrap();
    std::fs::write(
        directory.join("cs").join("20-more.ftl"),
        "greeting = Čau, { $name }!",
    )
    .unwrap();
    std::fs::write(directory.join("de.ftl"), "greeting = Hallo").unwrap();
    assert_eq!(
        messages.load_dir(&directory).unwrap(),
        3,
        "de is not in the chain"
    );
    assert_eq!(
        messages
            .text("greeting", &[("name", "Evo".into())])
            .unwrap(),
        "Čau, Evo!",
        "files load in name order and later ones win"
    );
    std::fs::write(directory.join("cs.ftl"), "oops = {").unwrap();
    let error = messages.load_dir(&directory).unwrap_err();
    assert!(error.to_string().contains("cs.ftl:1:"), "{error}");
    std::fs::remove_dir_all(&directory).unwrap();
}

/// One declaration of a view that installs `wanted` once; what it showed and
/// whether the host has to declare again.
fn declare(context: &mut Context, wanted: &mut Option<&str>) -> (String, usize) {
    let mut shown = String::new();
    let stats = context.run(Size::new(400.0, 300.0), &mut MonospaceMeasurer, |ui| {
        if let Some(locale) = wanted.take() {
            let mut messages = FluentMessages::new(locale, "en").unwrap();
            messages.add("cs", CZECH).unwrap();
            messages.add("en", ENGLISH).unwrap();
            ui.set_messages(Some(Box::new(messages)));
        }
        shown = ui.tr_args("greeting", &[("name", "Ido".into())]);
    });
    (shown, stats.activations)
}

#[test]
fn a_view_installs_and_switches_its_language_and_the_host_declares_once_more() {
    let mut context = Context::new();
    let mut wanted = Some("cs");
    // The declaration that asks still speaks the old language, and says so.
    let (shown, again) = declare(&mut context, &mut wanted);
    assert_eq!(shown, "greeting");
    assert!(again > 0, "hosts declare again before presenting");
    assert_eq!(
        declare(&mut context, &mut wanted),
        ("Ahoj, Ido!".to_owned(), 0)
    );
    assert_eq!(context.locale(), "cs");

    wanted = Some("en");
    declare(&mut context, &mut wanted);
    assert_eq!(declare(&mut context, &mut wanted).0, "Hello, Ido!");
}

#[test]
fn shipped_catalogs_translate_every_built_in_key_and_nothing_else() {
    use rust_gui_core::BUILTIN_MESSAGES;
    for (language, source) in rust_gui_fluent::FRAMEWORK_CATALOGS {
        let messages = FluentMessages::new(language, language).unwrap();
        let name = [("name", "X".into())];
        for (key, english) in BUILTIN_MESSAGES {
            let text = messages
                .text(key, &name)
                .unwrap_or_else(|| panic!("{language} lacks {key}"));
            assert!(
                !text.is_empty() && !text.contains("{"),
                "{language} {key}: {text}"
            );
            // An argument the English text uses has to appear in the translation.
            assert_eq!(
                english.contains("$name"),
                text.contains('X'),
                "{language} {key}"
            );
        }
        let keys = source
            .lines()
            .filter_map(|line| line.split_once(" = "))
            .map(|(key, _)| key);
        for key in keys {
            assert!(
                BUILTIN_MESSAGES.iter().any(|(known, _)| *known == key),
                "{language} translates unknown key {key}"
            );
        }
    }
}

#[test]
fn the_frameworks_own_controls_speak_the_installed_language() {
    use rust_gui_core::{PropertyGridOptions, PropertyGridState, Toast};
    let mut context = Context::new();
    context.set_messages(Some(Box::new(FluentMessages::new("cs-CZ", "en").unwrap())));
    let mut state = PropertyGridState::default();
    let mut width = 1.0;
    context.push_toast(Toast::new("Uloženo"));
    for _ in 0..2 {
        context.run(Size::new(600.0, 600.0), &mut MonospaceMeasurer, |ui| {
            ui.property_grid(
                "grid",
                &mut state,
                PropertyGridOptions::new("Karta"),
                |grid| {
                    grid.group("Rozměry", |grid| {
                        grid.default(2.0)
                            .number("Šířka", &mut width, Default::default());
                    });
                },
            );
        });
    }
    let label = |path: &[&str]| {
        let node = context.node(context.find(path).unwrap()).unwrap();
        (node.semantics.label.clone(), node.description.clone())
    };
    assert_eq!(label(&["grid", "$filter"]).0, "Filtr: Karta");
    assert_eq!(
        label(&["grid", "$rows", "group:Rozměry", "$header"])
            .1
            .as_deref(),
        Some("Sbalit skupinu Rozměry")
    );
    assert_eq!(
        label(&["grid", "$rows", "group:Rozměry", "row:Šířka", "$reset"])
            .1
            .as_deref(),
        Some("Obnovit výchozí hodnotu: Šířka")
    );
    let more = [
        "grid",
        "$rows",
        "group:Rozměry",
        "row:Šířka",
        "$editor",
        "$value",
        "$more",
    ];
    assert_eq!(label(&more).1.as_deref(), Some("Zvýšit"));
    let toasts = context.node(context.find(&["$toasts"]).unwrap()).unwrap();
    let toast = context.node(toasts.children[0]).unwrap();
    let close = context.node(*toast.children.last().unwrap()).unwrap();
    assert_eq!(close.description.as_deref(), Some("Zavřít"));
    assert!(context.missing_messages().is_empty());
}
