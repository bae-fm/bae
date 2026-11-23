use super::*;

#[test]
fn separators_and_case_make_no_other_number() {
    assert_eq!(catalog_key("WPCR-80001"), catalog_key("wpcr 80001"));
    assert_eq!(catalog_key("WPCR-80001").as_deref(), Some("wpcr80001"));
}

#[test]
fn a_placeholder_is_no_number() {
    for placeholder in ["[none]", "none", "None", "(none)", "- none", "-none-", "", "--"] {
        assert_eq!(catalog_key(placeholder), None, "{placeholder}");
    }
}

#[test]
fn a_joined_hyphen_dot_or_slash_keeps_a_number_whole() {
    assert_eq!(catalog_words("AB12-2"), vec!["ab122"]);
    assert_eq!(catalog_words("6789-12345-2"), vec!["6789123452"]);
    assert_eq!(catalog_words("CD1.234/5"), vec!["cd12345"]);
    assert_eq!(catalog_words("AB12\u{2010}2"), vec!["ab122"]);
}

#[test]
fn spaces_brackets_and_loose_separators_cut_numbers() {
    assert_eq!(
        catalog_words("Artist - Album [AB12-2] (XYZ 100)"),
        vec!["artist", "album", "ab122", "xyz", "100"]
    );
    assert_eq!(catalog_words("AB12 / CD34"), vec!["ab12", "cd34"]);
    assert_eq!(catalog_words("-AB12- 2."), vec!["ab12", "2"]);
    assert_eq!(catalog_words("AB12--2"), vec!["ab12", "2"]);
}

#[test]
fn a_printed_number_is_whole_numbers_of_its_line() {
    assert_eq!(
        printed_catalog_numbers("Label AB 12345-2 (1986) WPCR-80001"),
        vec!["AB 12345-2", "WPCR-80001"]
    );
    assert_eq!(printed_catalog_numbers("Z1 12345, MD6-233"), vec!["Z1 12345", "MD6-233"]);
    assert_eq!(
        printed_catalog_numbers("BVCK-15024/5 BVCP 21011~2"),
        vec!["BVCK-15024/5", "BVCP 21011"]
    );
    // Lower case, too few digits, or a letter piece before a word.
    assert!(printed_catalog_numbers("lowercase-12345 CD 1 SIDE 2 Box 123 AB CD12").is_empty());
}

#[test]
fn every_printed_number_is_printed_by_the_same_cut() {
    let line = "Artist - Album AB 12345-2 [XY-100] Z1 12345";
    let pieces = catalog_words(line);
    for number in printed_catalog_numbers(line) {
        let key = squash(&number);
        let mut run = String::new();
        let printed = pieces.iter().enumerate().any(|(start, _)| {
            run.clear();
            pieces[start..].iter().any(|piece| {
                run.push_str(piece);
                run == key
            })
        });
        assert!(printed, "{number}");
    }
}

#[test]
fn the_pieces_run_together_are_the_squashed_text() {
    for text in ["Ärtist - 7559-61571-2 [AB.12]", "  ", "x/-y"] {
        assert_eq!(catalog_words(text).concat(), squash(text), "{text}");
    }
}

/// The phonographic and copyright marks, written as the letters P and C
/// before a year, are no catalog number: `P 2007` is a letter and four
/// digits, which is what a number printed with a space looks like.
#[test]
fn a_copyright_line_is_no_number() {
    assert!(printed_catalog_numbers("P 2007").is_empty());
    assert!(printed_catalog_numbers("C 1999 Imprint").is_empty());
    assert_eq!(printed_catalog_numbers("P 2007 Imprint KBOX3555B"), vec!["KBOX3555B"]);
    // A prefix of two letters, or digits that are no year, is still a number.
    assert_eq!(printed_catalog_numbers("PC 2007"), vec!["PC 2007"]);
    assert_eq!(printed_catalog_numbers("P 12345"), vec!["P 12345"]);
}
