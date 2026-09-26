use super::*;

fn stated(value: &str) -> Option<String> {
    Barcode::stated(value).map(Barcode::into_string)
}

fn printed(line: &str) -> Option<String> {
    Barcode::printed(line).map(Barcode::into_string)
}

/// Every symbology is admitted on its own check digit, and a UPC-A is
/// spelled as the EAN-13 it is.
#[test]
fn a_stated_code_is_admitted_on_its_check_digit() {
    assert_eq!(stated("5012345678900").as_deref(), Some("5012345678900"));
    assert_eq!(stated("012345678905").as_deref(), Some("0012345678905"));
    assert_eq!(stated("0012345678905").as_deref(), Some("0012345678905"));
    assert_eq!(stated("12345670").as_deref(), Some("12345670"), "EAN-8");
    assert_eq!(stated("06543217").as_deref(), Some("06543217"), "UPC-E");
    assert_eq!(stated(" 5012345678900 ").as_deref(), Some("5012345678900"));

    assert_eq!(stated("5012345678901"), None, "check digit fails");
    assert_eq!(stated("012345678906"), None, "check digit fails");
    assert_eq!(stated("12345671"), None, "neither EAN-8 nor UPC-E");
    assert_eq!(
        stated("26543210"),
        None,
        "UPC-E encodes number systems 0 and 1 only"
    );
    assert_eq!(stated("0123456784"), None, "no symbology has ten digits");
    assert_eq!(stated("ABC-1234"), None);
    assert_eq!(stated(""), None);
}

/// A run of one digit is an unfilled field, even where its check digit
/// holds — all zeros and all twos both pass theirs.
#[test]
fn a_placeholder_is_no_code_whatever_its_check_digit() {
    for value in ["0000000000000", "2222222222222", "000000000000", "00000000"] {
        assert_eq!(stated(value), None, "{value} is a placeholder");
        assert_eq!(printed(value), None, "{value} is a placeholder");
    }
}

/// The digits under the bars, as a recognizer returns them: split where the
/// symbology prints its gaps, or at fewer of them.
#[test]
fn a_printed_line_is_read_in_its_symbology_s_layout() {
    for line in [
        "5 012345 678900",
        "5 012345678900",
        "5012345 678900",
        "5012345678900",
        "5-012345-678900",
    ] {
        assert_eq!(printed(line).as_deref(), Some("5012345678900"), "{line}");
    }
    for line in ["0 12345 67890 5", "0 1234567890 5", "012345678905"] {
        assert_eq!(printed(line).as_deref(), Some("0012345678905"), "{line}");
    }
    assert_eq!(printed("1234 5670").as_deref(), Some("12345670"), "EAN-8");
    assert_eq!(printed("0 654321 7").as_deref(), Some("06543217"), "UPC-E");
}

/// A line of digits grouped where no barcode prints a gap is some other
/// number — a catalog number made from the barcode body reads that way.
#[test]
fn digits_grouped_as_no_barcode_prints_them_are_not_a_code() {
    // The body and check digit of UPC-A 0 12345 67890 5, grouped as a catalog
    // number.
    assert_eq!(printed("0123 4 56789 0 5"), None);
    assert_eq!(printed("50 12345 678900"), None);
    assert_eq!(printed("123 45670"), None, "an EAN-8 breaks after four");
}

/// The line must be the code and nothing else, and its check digit must
/// hold.
#[test]
fn a_printed_line_with_anything_else_on_it_is_not_a_code() {
    assert_eq!(printed("5 012345 678901"), None, "check digit fails");
    assert_eq!(printed("UPC 0 12345 67890 5"), None);
    assert_eq!(printed("5 012345 678900 >"), None);
    assert_eq!(printed("Made in EU"), None);
    assert_eq!(printed(""), None);
}

/// The one rewrite is UPC-A to EAN-13; every other length is compared as
/// written, so codes of different lengths never meet by accident. No check
/// digit is asked for.
#[test]
fn comparison_keys_meet_only_where_the_encodings_define_it() {
    let key = |stated: &str| comparison_key(stated);
    assert_eq!(key("0 12345 67890 5"), Ok("0012345678905".to_string()));
    assert_eq!(key("012345678905"), Ok("0012345678905".to_string()));
    assert_eq!(key("0012345678905"), Ok("0012345678905".to_string()));
    assert_eq!(key("5051961234567"), Ok("5051961234567".to_string()));
    assert_eq!(key("12345678"), Ok("12345678".to_string()));
    assert_eq!(key("1234567"), Err(Unusable::TooShort));
    assert_eq!(key("0000000000000"), Err(Unusable::Placeholder));
    assert_eq!(key("none"), Err(Unusable::NotACode));
    assert_eq!(key("0 12345 67890 5 (sticker)"), Err(Unusable::NotACode));
    assert_eq!(key("012345678905>"), Err(Unusable::NotACode));
}

#[test]
fn written_digits_read_the_digits_out_of_the_spacing() {
    assert_eq!(
        written_digits(" 5 012345-678900 ").as_deref(),
        Some("5012345678900")
    );
    assert_eq!(written_digits("12"), Some("12".to_string()));
    assert_eq!(written_digits("- -"), None);
    assert_eq!(written_digits("ABC 123"), None);
}
