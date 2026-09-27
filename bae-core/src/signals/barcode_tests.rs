use super::codes_in;
use crate::signals::ArtworkAnalysis;

fn lines(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| text.to_string()).collect()
}

fn codes(analysis: &ArtworkAnalysis) -> Vec<String> {
    codes_in(analysis).map(|code| code.into_string()).collect()
}

/// The digits printed under bars the detector could not decode are the code;
/// a line whose check digit fails, or grouped as no barcode prints, is not.
#[test]
fn the_digits_printed_under_the_bars_are_a_code_the_detector_missed() {
    let analysis = ArtworkAnalysis {
        barcodes: Vec::new(),
        text_lines: lines(&[
            "Artist Name",
            "0123 4 56789 0 5",
            "5 012345 678901",
            "5 012345678900",
        ]),
    };
    assert_eq!(codes(&analysis), vec!["5012345678900".to_string()]);
}

/// The bars and the digits under them spell one code the same way — here a
/// UPC-A the detector reports as EAN-13 and the line prints as twelve digits —
/// and the detector's reading comes first.
#[test]
fn the_bars_and_their_printed_digits_spell_one_code() {
    let analysis = ArtworkAnalysis {
        barcodes: lines(&["0012345678905"]),
        text_lines: lines(&["0 12345 67890 5", "5 012345 678900"]),
    };
    assert_eq!(
        codes(&analysis),
        vec![
            "0012345678905".to_string(),
            "0012345678905".to_string(),
            "5012345678900".to_string(),
        ]
    );
}

/// A detector payload is a code on the same terms as any other: an unfilled
/// run of one digit, or a failed check digit, is not one.
#[test]
fn a_detector_payload_that_is_no_code_is_left_out() {
    let analysis = ArtworkAnalysis {
        barcodes: lines(&["0000000000000", "5012345678901", "12345670"]),
        text_lines: Vec::new(),
    };
    assert_eq!(codes(&analysis), vec!["12345670".to_string()]);
}
