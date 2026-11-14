use super::*;
use bae_catalogs::search::MetadataResult;

use bae_catalogs::catalog::LookupCatalog;
use bae_types::pressing::ReleaseLabel;

fn record(
    source: bae_catalogs::catalog::LookupCatalog,
    id: &str,
    labels: &[(&str, &str)],
) -> MetadataResult {
    MetadataResult {
        labels: labels
            .iter()
            .map(|(name, number)| ReleaseLabel::of(Some(name), Some(number)))
            .collect(),
        ..MetadataResult::for_test(source, id, None)
    }
}

fn line(name: &str, numbers: &[&str]) -> bae_types::pressing::LabelLine {
    bae_types::pressing::LabelLine {
        names: vec![name.to_string()],
        catalog_numbers: numbers.iter().map(|number| number.to_string()).collect(),
    }
}

/// Labels that share a name show it once, followed by every number, at
/// the place the name first appears; storage keeps one entry per label.
#[test]
fn a_name_is_shown_once_with_every_number_under_it() {
    let pressing = Pressing::over(vec![record(
        LookupCatalog::Discogs,
        "dg-1",
        &[("Label One", "AB-100"), ("Label Two", "CD-200"), ("label one", "EF-300")],
    )]);
    assert_eq!(pressing.labels().len(), 3);
    assert_eq!(
        pressing.label_lines(),
        vec![line("Label One", &["AB-100", "EF-300"]), line("Label Two", &["CD-200"]),]
    );
}

/// A row two catalogs name shows one list, in the order of the record it
/// leads with; what only the other record states follows.
#[test]
fn a_merged_row_lists_its_labels_in_its_lead_s_order() {
    let pressing = Pressing::over(vec![
        record(
            LookupCatalog::MusicBrainz,
            "mb-1",
            &[("Label One", "EF-300"), ("Label One", "AB-100")],
        ),
        record(
            LookupCatalog::Discogs,
            "dg-1",
            &[("Label One", "AB-100"), ("Label Two", "CD-200"), ("Label One", "EF 300")],
        ),
    ]);
    assert_eq!(
        pressing.label_lines(),
        vec![line("Label One", &["EF-300", "AB-100"]), line("Label Two", &["CD-200"]),]
    );
}

/// Two records that write one label two ways — with and without the
/// trade word it trails — are paired as one pressing on that label, and
/// the row lists it once.
#[test]
fn a_label_written_two_ways_is_listed_once() {
    let pressing = Pressing::over(vec![
        record(LookupCatalog::MusicBrainz, "mb-1", &[("Harbor Records", "AB-100")]),
        record(LookupCatalog::Discogs, "dg-1", &[("Harbor", "AB 100")]),
    ]);
    let paired = crate::pressing_evidence::PressingEvidence::between(
        &crate::pressing_evidence::ComparedPressing::of(&pressing.releases[0]),
        &crate::pressing_evidence::ComparedPressing::of(&pressing.releases[1]),
    );
    assert_eq!(paired.label, crate::pressing_evidence::Comparison::Same);
    assert_eq!(pressing.labels().len(), 1);
    assert_eq!(pressing.label_lines(), vec![line("Harbor Records", &["AB-100"])]);
}
