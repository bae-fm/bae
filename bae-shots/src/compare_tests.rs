use super::*;
use crate::compare::normalize::tests::element;
use crate::shots::Role;

fn text(words: &str) -> Element {
    element(Role::Text, Some(words), None)
}

fn button(words: &str) -> Element {
    element(Role::Button, Some(words), None)
}

fn sentences(board: &[Element], shot: &[Element]) -> Vec<String> {
    compare(board, shot).findings.iter().map(Finding::sentence).collect()
}

#[test]
fn equal_lists_have_no_findings() {
    let list = [text("Title"), button("Set up")];
    assert!(compare(&list, &list).findings.is_empty());
}

#[test]
fn a_board_element_the_shot_lacks_is_missing() {
    assert_eq!(
        sentences(&[text("Title"), button("Set up")], &[text("Title")]),
        ["Shot is missing the button “Set up”"]
    );
}

#[test]
fn a_shot_element_the_board_lacks_is_extra() {
    assert_eq!(
        sentences(&[text("Title")], &[text("Title"), text("Beta")]),
        ["Shot has the text “Beta”, which the board does not"]
    );
}

#[test]
fn a_swapped_pair_is_one_out_of_order_finding_not_two() {
    let result = compare(&[text("A"), text("B"), text("C")], &[text("A"), text("C"), text("B")]);
    assert_eq!(result.findings.len(), 1);
    assert!(matches!(result.findings[0], Finding::OutOfOrder { .. }));
    assert_eq!(
        result.findings[0].sentence(),
        "The text “B” is in another place in the shot's reading order"
    );
}

#[test]
fn the_same_words_in_another_kind_at_the_same_place_is_a_role_difference() {
    let result = compare(
        &[text("Title"), button("Set up"), text("End")],
        &[text("Title"), text("Set up"), text("End")],
    );
    assert_eq!(result.findings.len(), 1);
    assert_eq!(
        result.findings[0].sentence(),
        "“Set up” is a button on the board and a text in the shot"
    );
}

#[test]
fn the_same_words_in_another_kind_far_apart_are_missing_and_extra() {
    let result = compare(
        &[button("Go"), text("A"), text("B"), text("C")],
        &[text("A"), text("B"), text("C"), text("Go")],
    );
    let kinds: Vec<_> = result.findings.iter().map(Finding::sentence).collect();
    assert_eq!(
        kinds,
        ["Shot is missing the button “Go”", "Shot has the text “Go”, which the board does not"]
    );
}

#[test]
fn a_matched_pair_whose_state_differs_says_which_part() {
    let mut on = element(Role::Toggle, Some("Sync"), None);
    on.value = Some("on".to_string());
    let mut off = on.clone();
    off.value = Some("off".to_string());
    assert_eq!(
        sentences(&[on.clone()], &[off]),
        ["The toggle “Sync” has the value “on” on the board and “off” in the shot"]
    );

    let mut disabled = button("Go");
    disabled.enabled = false;
    assert_eq!(
        sentences(&[button("Go")], &[disabled]),
        ["The button “Go” is enabled on the board and disabled in the shot"]
    );

    let mut selected = element(Role::Tab, Some("Library"), None);
    selected.selected = Some(true);
    assert_eq!(
        sentences(&[selected], &[element(Role::Tab, Some("Library"), None)]),
        ["The tab “Library” is selected on the board and not selected in the shot"]
    );
}

#[test]
fn a_field_with_no_text_on_one_side_and_an_empty_string_on_the_other_agrees() {
    let mut empty = element(Role::Field, Some("Name"), None);
    empty.value = Some(String::new());
    let absent = element(Role::Field, Some("Name"), None);
    assert!(compare(&[empty], &[absent]).findings.is_empty());
}

#[test]
fn findings_come_in_reading_order() {
    let result = compare(
        &[text("A"), button("B"), text("C"), button("D")],
        &[text("A"), text("C"), button("D"), text("E")],
    );
    let sentences: Vec<_> = result.findings.iter().map(Finding::sentence).collect();
    assert_eq!(
        sentences,
        ["Shot is missing the button “B”", "Shot has the text “E”, which the board does not"]
    );
}

#[test]
fn an_element_with_no_words_is_described_as_unlabelled() {
    let toggle = element(Role::Toggle, None, None);
    assert_eq!(sentences(&[toggle], &[]), ["Shot is missing the unlabelled toggle"]);
}
