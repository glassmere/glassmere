use seaglass_core::{Direction, FindState, Style, TextDocument, TextStyle, TextWrap};

/// Finds matches the way the find bar does, by hand.
fn matches(document: &TextDocument, state: &mut FindState) -> Vec<String> {
    state.refresh(document);
    let text = document.text();
    state
        .matches()
        .iter()
        .map(|range| text[range.clone()].to_owned())
        .collect()
}

#[test]
fn the_find_bar_searches_by_pattern_and_says_why_a_bad_one_found_nothing() {
    let document = TextDocument::from_text("alpha 12 beta\nAlpha 345 gamma\n");
    let mut state = FindState::default();
    state.open = true;
    state.query = "[0-9]+".into();
    state.regex = true;
    assert_eq!(matches(&document, &mut state), ["12", "345"]);
    assert_eq!(state.error(), None);

    // Case follows the same flag as a plain search.
    state.query = "alpha".into();
    assert_eq!(matches(&document, &mut state), ["alpha", "Alpha"]);
    state.case_sensitive = true;
    assert_eq!(matches(&document, &mut state), ["alpha"]);

    // A match never crosses a line, so an anchor means the line's edge.
    state.query = "^Alpha".into();
    assert_eq!(matches(&document, &mut state), ["Alpha"]);

    // A pattern that cannot be compiled reports why and finds nothing.
    state.query = "([0-9]".into();
    assert!(matches(&document, &mut state).is_empty());
    let error = state.error().expect("an error");
    assert!(!error.is_empty() && error.len() < 200, "{error}");

    // An empty pattern is not an error, and a literal search still works.
    state.query = String::new();
    assert!(matches(&document, &mut state).is_empty() && state.error().is_none());
    state.regex = false;
    state.query = "beta".into();
    assert_eq!(matches(&document, &mut state), ["beta"]);
    assert_eq!(state.error(), None);
}

#[test]
fn a_paragraph_can_be_forced_to_start_in_one_direction() {
    // The style says what the text system is told.
    let forced = Style::default().base_direction(Direction::Rtl);
    let resolved = forced.resolve_text(&seaglass_core::ResolvedText::from_theme(
        &seaglass_core::Theme::dark(),
    ));
    assert_eq!(resolved.base_direction, Some(Direction::Rtl));
    assert_eq!(resolved.text_style(TextWrap::None).base, Some(true));

    // It is inherited, and left to the text when nobody forces it.
    let inside = Style::default().resolve_text(&resolved);
    assert_eq!(inside.base_direction, Some(Direction::Rtl));
    let plain = Style::default().resolve_text(&seaglass_core::ResolvedText::from_theme(
        &seaglass_core::Theme::dark(),
    ));
    assert_eq!(plain.text_style(TextWrap::None).base, None);
    assert_eq!(
        Style::default()
            .base_direction(Direction::Ltr)
            .resolve_text(&resolved)
            .text_style(TextWrap::None)
            .base,
        Some(false),
        "an inner paragraph may force the other way"
    );

    // The caret order setting is a different thing and is left alone.
    assert_eq!(TextStyle::default().base, None);
}
