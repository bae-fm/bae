use super::*;

fn line<'a>(credits: impl IntoIterator<Item = (&'a str, Option<&'a str>)>) -> CreditLine {
    CreditLine::from_stored(credits)
}

#[test]
fn each_recognized_join_is_typed_and_the_last_prints_none() {
    let line = line([("Artist A", Some(" feat. ")), ("Artist B", Some(" & "))]);
    assert_eq!(
        line.names(),
        [
            CreditedName { name: "Artist A".into(), join: Some(CreditJoin::Featuring) },
            CreditedName { name: "Artist B".into(), join: None },
        ]
    );
    assert_eq!(line.plain_text(), "Artist A feat. Artist B");
}

#[test]
fn every_recognized_join_is_recognized_whatever_its_spacing_and_case() {
    for (stored, join) in [
        (" feat. ", CreditJoin::Featuring),
        ("Feat.", CreditJoin::Featuring),
        (" & ", CreditJoin::Ampersand),
        (" and ", CreditJoin::And),
        (" with ", CreditJoin::With),
        (" vs. ", CreditJoin::Versus),
        (" Vs. ", CreditJoin::Versus),
    ] {
        assert_eq!(CreditJoin::from_stored(stored), Some(join), "{stored:?}");
    }
}

#[test]
fn a_join_bae_does_not_recognize_is_kept_as_written() {
    assert_eq!(CreditJoin::from_stored(" / "), Some(CreditJoin::Written(" / ".to_string())));
    assert_eq!(line([("A", Some(" / ")), ("B", None)]).plain_text(), "A / B");
}

#[test]
fn a_comma_or_nothing_is_the_plain_join() {
    assert_eq!(CreditJoin::from_stored(", "), None);
    assert_eq!(CreditJoin::from_stored(""), None);
    assert_eq!(CreditJoin::from_stored("  "), None);
}

#[test]
fn plain_names_are_joined_with_a_comma_for_a_place_with_no_language() {
    assert_eq!(
        line([("Artist A", None), ("Artist B", None), ("Artist C", None)]).plain_text(),
        "Artist A, Artist B, Artist C"
    );
}

#[test]
fn a_run_of_plain_names_after_a_join_keeps_commas() {
    assert_eq!(
        line([("A", Some(" feat. ")), ("B", None), ("C", Some(", ")), ("D", None)]).plain_text(),
        "A feat. B, C, D"
    );
}

#[test]
fn no_credits_print_nothing() {
    assert_eq!(line(std::iter::empty()).plain_text(), "");
    assert!(line(std::iter::empty()).is_empty());
}

#[test]
fn the_last_credits_join_is_never_kept() {
    assert_eq!(line([("A", Some(" & "))]).names()[0].join, None);
}

#[test]
fn name_values_are_each_printed_name_alone() {
    let line = line([("A", Some(" feat. ")), ("B", None)]);
    assert_eq!(line.name_values().collect::<Vec<_>>(), ["A", "B"]);
}

#[test]
fn removing_the_last_credit_drops_its_neighbours_join() {
    let mut joins = vec![Some(" feat. ".to_string()), None];
    joins.pop();
    drop_last_join(&mut joins, |join| join);
    assert_eq!(joins, [None]);
}

#[test]
fn a_line_reads_back_from_its_stored_form() {
    let line = line([("A", Some(" feat. ")), ("B", Some(" / ")), ("C", Some(", ")), ("D", None)]);
    assert_eq!(CreditLine::from_stored_credits(&line.to_stored_credits()), line);
}
