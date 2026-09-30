//! Which of the offered rows that would look alike on screen is listed.

use crate::db::LibraryStatus;
use crate::identify::agreements::CandidateText;
use crate::identify::combine::{combine_results, LookupAnswers};
use crate::identify::medium::FolderAudio;
use crate::import::search::MetadataResult;
use crate::import::Catalog;
use crate::pressing::{Packaging, ReleaseLabel};

/// A 1959 pressing on Label One, LBL-100, as `catalog` lists it under `id`.
fn pressing(catalog: Catalog, id: &str) -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            year: Some(1959),
            labels: vec![ReleaseLabel::of(Some("Label One"), Some("LBL-100"))],
            ..MetadataResult::for_test(catalog, id, Some("group"))
        },
        LibraryStatus::absent(id),
    )
}

fn offered(results: Vec<(MetadataResult, LibraryStatus)>) -> Vec<String> {
    combine_results(
        LookupAnswers {
            catalog: results,
            ..LookupAnswers::default()
        },
        &CandidateText::default(),
        FolderAudio::UNPROVEN,
    )
    .0
    .matches
    .into_iter()
    .map(|result| result.release_id)
    .collect()
}

/// Rows that show the same thing are listed once: the lowest id of them.
#[test]
fn look_alike_rows_are_listed_once() {
    assert_eq!(
        offered(vec![
            pressing(Catalog::Discogs, "300"),
            pressing(Catalog::Discogs, "20"),
            pressing(Catalog::Discogs, "1000"),
        ]),
        vec!["20"]
    );
}

/// A row both catalogs list is kept over a look-alike only one lists, whatever
/// their ids.
#[test]
fn a_row_both_catalogs_list_is_kept_over_a_look_alike() {
    let barcoded = |(mut result, status): (MetadataResult, LibraryStatus)| {
        result.barcodes = vec!["012345678905".to_string()];
        (result, status)
    };
    let kept = offered(vec![
        pressing(Catalog::Discogs, "20"),
        barcoded(pressing(Catalog::Discogs, "5000")),
        barcoded(pressing(Catalog::MusicBrainz, "mb-1")),
    ]);
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert!(!kept.contains(&"20".to_string()), "{kept:?}");
}

/// Ids and notes no badge shows are not what a row shows: rows differing only
/// in them are listed once.
#[test]
fn rows_differing_only_in_their_ids_and_notes_are_listed_once() {
    let noted = |id: &str, note: &str| {
        let (mut result, status) = pressing(Catalog::Discogs, id);
        result.notes = vec![note.to_string()];
        (result, status)
    };
    assert_eq!(
        offered(vec![noted("20", "Plant Alpha"), noted("30", "Plant Beta")]),
        vec!["20"]
    );
}

/// A row that shows something another does not is listed beside it.
#[test]
fn rows_that_show_different_details_are_both_listed() {
    let (mut boxed, status) = pressing(Catalog::Discogs, "30");
    boxed.packaging = Some(Packaging::Box);
    assert_eq!(
        offered(vec![pressing(Catalog::Discogs, "20"), (boxed, status)]),
        vec!["20", "30"]
    );
}
