use super::*;

type Pattern = NamePattern<&'static str>;

fn value(name: &'static str) -> NamePiece<&'static str> {
    NamePiece::Value(name)
}

fn text(text: &str) -> NamePiece<&'static str> {
    NamePiece::Text(text.to_string())
}

fn pattern(pieces: Vec<NamePiece<&'static str>>) -> Pattern {
    NamePattern::new(pieces)
}

#[test]
fn a_pattern_is_filled_with_its_text_kept_as_typed() {
    let pattern = pattern(vec![value("artist"), text("  -   "), value("album"), text(" ")]);
    let filled = pattern.fill(|name| match *name {
        "artist" => "Mara Quill".to_string(),
        _ => "Low Tide".to_string(),
    });
    assert_eq!(filled, "Mara Quill  -   Low Tide ");
}

#[test]
fn an_empty_value_leaves_only_its_neighbours_text() {
    let pattern = pattern(vec![value("disc"), text(" - "), value("track")]);
    let filled = pattern.fill(|name| match *name {
        "disc" => String::new(),
        _ => "04".to_string(),
    });
    assert_eq!(filled, " - 04");
}

#[test]
fn a_pattern_of_values_and_ordinary_text_can_make_a_name() {
    assert_eq!(pattern(vec![value("title")]).validate(), Ok(()));
    assert_eq!(
        pattern(vec![text("Live "), value("title"), text(" (demo) 2.0, ok & #1")]).validate(),
        Ok(())
    );
    // Only the first piece starting the name matters, and a dot inside is fine.
    assert_eq!(pattern(vec![value("title"), text(".x")]).validate(), Ok(()));
    assert_eq!(pattern(vec![text("v1.2 "), value("title")]).validate(), Ok(()));
}

#[test]
fn no_pieces_make_no_name() {
    assert_eq!(pattern(vec![]).validate(), Err(NamePatternError::Empty));
}

#[test]
fn text_starting_the_name_with_a_dot_is_refused() {
    assert_eq!(
        pattern(vec![text(".hidden "), value("title")]).validate(),
        Err(NamePatternError::StartsWithDot)
    );
}

#[test]
fn text_holding_a_character_a_name_cant_have_is_refused() {
    for character in ['\\', ':', '?', '*', '"', '<', '>', '|', '/'] {
        assert_eq!(
            pattern(vec![value("title"), text(&format!("a{character}b"))]).validate(),
            Err(NamePatternError::ForbiddenCharacter(character)),
            "{character}"
        );
    }
}

#[test]
fn the_first_problem_in_the_pattern_is_the_one_named() {
    assert_eq!(
        pattern(vec![text("a/b"), text("c:d")]).validate(),
        Err(NamePatternError::ForbiddenCharacter('/'))
    );
    assert_eq!(pattern(vec![text(".a/b")]).validate(), Err(NamePatternError::StartsWithDot));
}

#[test]
fn typed_text_of_nothing_is_noticed() {
    assert!(pattern(vec![value("title"), text("")]).has_empty_text());
    assert!(!pattern(vec![value("title"), text(" ")]).has_empty_text());
}
