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
    // Digits that are no year are still a number.
    assert_eq!(printed_catalog_numbers("P 12345"), vec!["P 12345"]);
}

/// Two letters, three letters or digits, two digits and five digits.
#[test]
fn a_recording_code_is_no_number() {
    assert!(printed_catalog_numbers("QQ1X90000123").is_empty());
    assert!(printed_catalog_numbers("QQXY19000123 QQXY10000124").is_empty());
    assert_eq!(printed_catalog_numbers("QQXY19000123 WPCR-80001"), vec!["WPCR-80001"]);
}

/// A ripper's report prints hashes of 8, 32, 40 or 64 hex digits that hold
/// a letter and a digit.
#[test]
fn a_checksum_is_no_number() {
    assert!(printed_catalog_numbers("Hash: A1B2C3D4E5F60718293A4B5C6D7E8F90").is_empty());
    assert!(
        printed_catalog_numbers("Signature: A1B2C3D4E5F60718293A4B5C6D7E8F90A1B2C3D4").is_empty()
    );
    assert!(printed_catalog_numbers(
        "A1B2C3D4E5F60718293A4B5C6D7E8F90A1B2C3D4E5F60718293A4B5C6D7E8F90"
    )
    .is_empty());
    assert!(printed_catalog_numbers("04   97,5 [5A2CF88C] [C0DE1234]   CRC32").is_empty());
    // Letters only, or another length, is no digest; a number of hex
    // characters stays one.
    assert!(printed_catalog_numbers("ABCDEFAB").is_empty());
    assert_eq!(printed_catalog_numbers("ABCDE12345"), vec!["ABCDE12345"]);
}

/// A percentage and a version are no number: `Conclusion: AAAA 100%` and a
/// program name with its version.
#[test]
fn a_ripper_report_piece_is_no_number() {
    assert!(printed_catalog_numbers("Conclusion: AAAA 100%").is_empty());
    assert!(printed_catalog_numbers("Conclusion: AAAA 98.5%").is_empty());
    assert!(printed_catalog_numbers("PROGRAM2 0.11").is_empty());
    assert!(printed_catalog_numbers("TOOL 10.15").is_empty());
    // A number of two groups of three digits after a dot stays one.
    assert_eq!(printed_catalog_numbers("XYZ 846.104"), vec!["XYZ 846.104"]);
}

/// A letter prefix and a year is no number, as a copyright mark and a year
/// are none; the same letters and a year without the space are one.
#[test]
fn a_name_and_a_year_is_no_number() {
    assert_eq!(
        printed_catalog_numbers("ABCD 1980 'Title' (Country Label XY 16018)"),
        vec!["XY 16018"]
    );
    assert_eq!(printed_catalog_numbers("MR2002"), vec!["MR2002"]);
}

/// A medium and a year in a scan's file name.
#[test]
fn a_medium_and_a_year_is_no_number() {
    assert!(printed_catalog_numbers("Artist Title CD1998 Back").is_empty());
    assert!(printed_catalog_numbers("Artist Title LP2011 500").is_empty());
    assert_eq!(printed_catalog_numbers("CD 099"), vec!["CD 099"]);
}

/// A composer's work number names a piece, not a pressing.
#[test]
fn a_work_number_is_no_number() {
    assert!(printed_catalog_numbers("Suite No. 5 In E Minor, BWV 810 - Prelude").is_empty());
    assert!(printed_catalog_numbers("Prelude No. 1 in C major, BWV846").is_empty());
    assert!(printed_catalog_numbers("Suites BWV 806-811 1994").is_empty());
}

/// A store's image size in the file name of a scan.
#[test]
fn an_image_size_is_no_number() {
    assert!(printed_catalog_numbers("61J9DVBCBJL._SS500_").is_empty());
    assert_eq!(printed_catalog_numbers("SS500"), vec!["SS500"]);
}

/// The shapes a folder prints its real numbers in stay.
#[test]
fn the_shapes_of_real_numbers_stay() {
    for number in [
        "ABCD-12345",
        "AB 12345-2",
        "TVT2617-2",
        "ARRCD 53-333",
        "ATCO 36-142-2",
        "BR16-2",
        "CCK 53226",
        "INT 846.104",
        "RISELP052",
        "P25L 25031",
        "S1001",
        "SH 45059",
        "MR063",
        "UDCD 632",
        "WPCR-80001",
    ] {
        assert_eq!(printed_catalog_numbers(&format!("Label [{number}]")), vec![number], "{number}");
    }
}
