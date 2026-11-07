use super::*;

/// A restore code as coven makes it: its prefix, a dash, then base32.
const CODE: &str = "CVR1-UJZDEGXDNCF32EPF3DHODZDOCIS2JHTLGMXGEDN73U55XTPLPFT7";

#[test]
fn a_code_shows_in_groups_of_four_with_its_prefix_the_first() {
    assert_eq!(
        grouped(CODE),
        "CVR1 UJZD EGXD NCF3 2EPF 3DHO DZDO CIS2 JHTL GMXG EDN7 3U55 XTPL PFT7"
    );
}

#[test]
fn a_code_with_a_short_last_group_keeps_it() {
    assert_eq!(grouped("CVR1-ABCDEF"), "CVR1 ABCD EF");
}

#[test]
fn the_code_as_shown_is_the_code_when_typed_back() {
    assert_eq!(entered(&grouped(CODE)), CODE);
}

#[test]
fn spaces_and_line_breaks_in_a_typed_code_are_dropped() {
    assert_eq!(entered(" CVR1-UJZD  EGXD\nNCF3\t2EPF "), "CVR1-UJZDEGXDNCF32EPF");
}

#[test]
fn a_code_with_its_dash_is_left_as_it_is() {
    assert_eq!(entered(CODE), CODE);
}

#[test]
fn text_too_short_to_hold_a_prefix_is_left_alone() {
    assert_eq!(entered("CVR"), "CVR");
    assert_eq!(entered(""), "");
}
