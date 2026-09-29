//! How the catalogs' notes break a tie between pressings that agree on
//! everything else: the folder naming a word only one row's notes write.

use super::*;
use crate::signals::{TextLine, TextOrigin};

type Outcome = (Findings, LibraryStatuses);

fn folder(line: &str) -> CandidateText {
    CandidateText::of(
        &[TextLine {
            text: line.to_string(),
            origin: TextOrigin::FolderName,
        }],
        &[],
        &[],
    )
}

/// A pressing of Album One on Label One, L1-100, whose read document writes
/// `notes`, each in its own packaging so no two look alike.
fn pressing(release_id: &str, notes: &[&str]) -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            title: "Album One".to_string(),
            artist: Some("Artist One".to_string()),
            year: Some(1987),
            labels: vec![crate::pressing::ReleaseLabel::of(
                Some("Label One"),
                Some("L1-100"),
            )],
            notes: notes.iter().map(|note| note.to_string()).collect(),
            packaging: Some(match release_id {
                "rel-alpha" => crate::pressing::Packaging::JewelCase,
                "rel-beta" => crate::pressing::Packaging::Digipak,
                "rel-gamma" => crate::pressing::Packaging::GatefoldCover,
                _ => crate::pressing::Packaging::CardboardSleeve,
            }),
            ..MetadataResult::for_test(Catalog::MusicBrainz, release_id, Some("rg-album-one"))
        },
        LibraryStatus::absent(release_id),
    )
}

fn by_catalog(results: Vec<(MetadataResult, LibraryStatus)>, text: &CandidateText) -> Outcome {
    combine_results(
        LookupAnswers {
            catalog: results,
            ..LookupAnswers::default()
        },
        Vec::new(),
        text,
        FolderAudio::UNPROVEN,
    )
}

fn offered(outcome: &Outcome) -> Vec<&str> {
    outcome
        .0
        .matches
        .iter()
        .map(|result| result.release_id.as_str())
        .collect()
}

/// Two pressings alike in every field: the folder naming a word only the
/// second's notes write offers that one.
#[test]
fn a_word_only_one_row_s_notes_write_offers_that_row() {
    let outcome = by_catalog(
        vec![
            pressing("rel-alpha", &["Pressed By Plant Alpha"]),
            pressing("rel-beta", &["Made In Nordland By Plant Beta"]),
        ],
        &folder("1987 Nordland Label One L1-100"),
    );
    assert_eq!(offered(&outcome), vec!["rel-beta"]);
}

/// The folder naming a word of each row's own notes sets neither apart.
#[test]
fn words_from_both_rows_notes_keep_the_tie() {
    let outcome = by_catalog(
        vec![
            pressing("rel-alpha", &["Pressed By Plant Alpha"]),
            pressing("rel-beta", &["Made In Nordland By Plant Beta"]),
        ],
        &folder("1987 Nordland Alpha L1-100"),
    );
    assert_eq!(offered(&outcome), vec!["rel-alpha", "rel-beta"]);
}

/// A word both rows' notes write tells neither apart, however the folder
/// names it.
#[test]
fn a_word_every_row_s_notes_write_gives_no_point() {
    let outcome = by_catalog(
        vec![
            pressing("rel-alpha", &["Pressed By Plant Alpha"]),
            pressing("rel-beta", &["Pressed By Plant Beta"]),
            pressing("rel-gamma", &["Plant Gamma"]),
        ],
        &folder("1987 Plant L1-100"),
    );
    assert_eq!(
        offered(&outcome),
        vec!["rel-alpha", "rel-beta", "rel-gamma"]
    );
}

/// A matrix inscription writing the catalog number the folder names gives no
/// point: numbers in notes are the codes every pressing of the number
/// carries, which one catalog entry transcribes and a look-alike's may not.
#[test]
fn numbers_in_the_notes_give_no_point() {
    let outcome = by_catalog(
        vec![
            pressing("rel-alpha", &["1 100 L1-100 PA-01"]),
            pressing("rel-beta", &["Made In Nordland By Plant Beta"]),
        ],
        &folder("1987 Nordland Label One L1-100"),
    );
    assert_eq!(offered(&outcome), vec!["rel-beta"]);
}

/// The notes only break ties: a row the folder's catalog number names is
/// offered over one whose notes the folder names.
#[test]
fn the_notes_never_outrank_an_earlier_field() {
    let (numbered, status) = pressing("rel-alpha", &["Pressed By Plant Alpha"]);
    let unnumbered = MetadataResult {
        labels: vec![crate::pressing::ReleaseLabel::of(Some("Label One"), None)],
        ..pressing("rel-beta", &["Made In Nordland By Plant Beta"]).0
    };
    let outcome = by_catalog(
        vec![
            (numbered, status),
            (unnumbered, LibraryStatus::absent("rel-beta")),
        ],
        &folder("1987 Nordland Label One L1-100"),
    );
    assert_eq!(offered(&outcome), vec!["rel-alpha"]);
}

/// Where nothing else tells two look-alike rows apart, the dated one is
/// offered and the undated one set aside.
#[test]
fn an_undated_look_alike_folds_below_a_dated_one() {
    let (dated, dated_status) = pressing("rel-dated", &[]);
    let undated = MetadataResult {
        year: None,
        ..pressing("rel-undated", &[]).0
    };
    let outcome = by_catalog(
        vec![
            (undated, LibraryStatus::absent("rel-undated")),
            (dated, dated_status),
        ],
        &folder("Label One L1-100"),
    );
    assert_eq!(offered(&outcome), vec!["rel-dated"]);
    assert_eq!(
        outcome
            .0
            .narrowed_out
            .matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["rel-undated"]
    );
}

/// Stating a year only breaks a tie: an undated row better on an earlier
/// field is offered over a dated one.
#[test]
fn stating_a_year_never_outranks_an_earlier_field() {
    let undated = MetadataResult {
        year: None,
        ..pressing("rel-undated", &[]).0
    };
    let dated = MetadataResult {
        labels: vec![crate::pressing::ReleaseLabel::of(Some("Label One"), None)],
        ..pressing("rel-dated", &[]).0
    };
    let outcome = by_catalog(
        vec![
            (undated, LibraryStatus::absent("rel-undated")),
            (dated, LibraryStatus::absent("rel-dated")),
        ],
        &folder("Label One L1-100"),
    );
    assert_eq!(offered(&outcome), vec!["rel-undated"]);
}

/// The notes are weighed among every row tied above them, dated or not: an
/// undated look-alike's notes cancel the word it shares with a dated row, so
/// the folder naming that word sets neither apart.
#[test]
fn the_notes_are_weighed_before_the_year() {
    let undated = MetadataResult {
        year: None,
        ..pressing("rel-undated", &["Plant Beta"]).0
    };
    let outcome = by_catalog(
        vec![
            pressing("rel-alpha", &["Plant Alpha"]),
            pressing("rel-beta", &["Plant Beta"]),
            (undated, LibraryStatus::absent("rel-undated")),
        ],
        &folder("Label One L1-100 Beta"),
    );
    assert_eq!(offered(&outcome), vec!["rel-alpha", "rel-beta"]);
}
