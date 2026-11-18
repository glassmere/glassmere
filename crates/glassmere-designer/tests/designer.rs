use glassmere::{Point, Size};
use glassmere_designer::{Designer, Msg};
use glassmere_testing::Harness;
use std::path::PathBuf;

const VIEW: &str = "data { name: \"Ada\" }\n\ncolumn { padding: 12; gap: 8\n    label \"Hello, {name}\"\n    button \"Go\" { on-click: go }\n}\n";

/// A designer on its own copy of `VIEW`.
fn open(name: &str) -> (Harness<Designer>, PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "glassmere-designer-{}-{name}.glass",
        std::process::id()
    ));
    std::fs::write(&path, VIEW).unwrap();
    let designer = Designer::open(&path).unwrap();
    (Harness::app(Size::new(1280.0, 800.0), designer), path)
}

fn saved(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn a_pick_in_the_preview_selects_the_element_and_edits_reach_the_file() {
    let (mut harness, path) = open("pick");
    harness.by_label("Hello, Ada");
    let go = harness.by_label("Go");
    let bounds = harness.context().node(go).unwrap().bounds;
    harness.send(Msg::TogglePick);
    harness.send(Msg::PickAt(Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }));
    harness.run();
    harness.by_label("button, line 5");

    harness.send(Msg::Text("Start".into()));
    harness.by_label("Start");
    assert!(saved(&path).contains("button \"Start\" { on-click: go }"));

    harness.send(Msg::Property("background".into(), "#16695f".into()));
    assert!(saved(&path).contains("{ background: #16695f; on-click: go }"));

    // A value that does not parse stays in its field and out of the file.
    harness.send(Msg::Property("background".into(), "#16".into()));
    assert!(saved(&path).contains("background: #16695f"));
    harness.send(Msg::Property("background".into(), "#166534".into()));
    assert!(saved(&path).contains("background: #166534;"));

    harness.send(Msg::RemoveProperty("background".into()));
    assert!(saved(&path).contains("button \"Start\" { on-click: go }"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn elements_are_added_moved_and_deleted() {
    let (mut harness, path) = open("arrange");
    let label = harness.by_label("label  \"Hello, {name}\"");
    harness.click(label);
    harness.by_label("label, line 4");

    harness.send(Msg::Move(false));
    assert!(
        saved(&path).contains("    button \"Go\" { on-click: go }\n    label \"Hello, {name}\"\n")
    );
    // The selection follows the element it moved.
    harness.by_label("label, line 5");
    harness.send(Msg::Move(true));
    assert_eq!(saved(&path), VIEW);
    harness.by_label("label, line 4");

    harness.send(Msg::Palette(0));
    harness.send(Msg::Add(false));
    assert!(saved(&path).contains("    label \"Hello, {name}\"\n    label \"Text\"\n"));
    harness.send(Msg::Delete);
    assert!(!saved(&path).contains("Hello"));
    assert_eq!(harness.state().selected(), None);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn the_preview_runs_on_the_data_of_the_file() {
    let (mut harness, path) = open("data");
    harness.send(Msg::Data("name".into(), "Grace".into()));
    harness.by_label("Hello, Grace");
    let go = harness.by_label("Go");
    harness.click(go);
    harness.by_label("go()");
    harness.send(Msg::ResetData);
    harness.by_label("Hello, Ada");
    std::fs::remove_file(path).unwrap();
}
