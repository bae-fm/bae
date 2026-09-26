use super::codes_in;
use crate::signals::{ArtworkAnalysis, DetectedBarcode, ImageRegion, RecognizedLine, SignalOrigin};

fn region(y: f32) -> Option<ImageRegion> {
    ImageRegion::new(0.1, y, 0.5, 0.05)
}

fn line(text: &str, y: f32) -> RecognizedLine {
    RecognizedLine {
        text: text.to_string(),
        region: region(y),
    }
}

fn codes(analysis: &ArtworkAnalysis) -> Vec<(String, Option<ImageRegion>, SignalOrigin)> {
    codes_in(analysis)
        .map(|reading| (reading.code.into_string(), reading.region, reading.origin))
        .collect()
}

/// A back cover whose bars the detector could not decode still carries the
/// digits printed under them, and those are the code. A line whose check
/// digit fails, and a catalog number grouped as no barcode prints its digits,
/// are not.
#[test]
fn the_digits_printed_under_the_bars_are_a_code_the_detector_missed() {
    let analysis = ArtworkAnalysis {
        barcodes: Vec::new(),
        text_lines: vec![
            line("Artist Name", 0.1),
            line("0123 4 56789 0 5", 0.2),
            line("5 012345 678901", 0.3),
            line("5 012345678900", 0.8),
        ],
    };
    assert_eq!(
        codes(&analysis),
        vec![(
            "5012345678900".to_string(),
            region(0.8),
            SignalOrigin::Artwork
        )]
    );
}

/// The bars and the digits under them spell one code the same way, however
/// each writes it, and each says how it was read — here a UPC-A the detector reports as its EAN-13 and the
/// line prints as twelve digits — and the detector's reading comes first.
#[test]
fn the_bars_and_their_printed_digits_spell_one_code() {
    let analysis = ArtworkAnalysis {
        barcodes: vec![DetectedBarcode {
            payload: "0012345678905".to_string(),
            region: region(0.7),
        }],
        text_lines: vec![line("0 12345 67890 5", 0.8), line("5 012345 678900", 0.9)],
    };
    assert_eq!(
        codes(&analysis),
        vec![
            (
                "0012345678905".to_string(),
                region(0.7),
                SignalOrigin::ArtworkBarcode
            ),
            (
                "0012345678905".to_string(),
                region(0.8),
                SignalOrigin::Artwork
            ),
            (
                "5012345678900".to_string(),
                region(0.9),
                SignalOrigin::Artwork
            ),
        ]
    );
}

/// A detector payload is a code on the same terms as any other: an unfilled
/// run of one digit or a payload whose check digit fails is not one.
#[test]
fn a_detector_payload_that_is_no_code_is_left_out() {
    let analysis = ArtworkAnalysis {
        barcodes: vec![
            DetectedBarcode {
                payload: "0000000000000".to_string(),
                region: None,
            },
            DetectedBarcode {
                payload: "5012345678901".to_string(),
                region: None,
            },
            DetectedBarcode {
                payload: "12345670".to_string(),
                region: None,
            },
        ],
        text_lines: Vec::new(),
    };
    assert_eq!(
        codes(&analysis),
        vec![("12345670".to_string(), None, SignalOrigin::ArtworkBarcode)]
    );
}
