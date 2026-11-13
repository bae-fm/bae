//! Which of the offered rows that would look alike on screen is listed.

use crate::agreements::CandidateText;
use crate::combine::{combine_results, LookupAnswers};
use crate::medium::FolderAudio;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::MetadataResult;
use bae_types::pressing::{Packaging, ReleaseLabel};

/// A 1959 pressing on Label One, LBL-100, as `catalog` lists it under `id`.
fn pressing(catalog: LookupCatalog, id: &str) -> MetadataResult {
    MetadataResult {
        year: Some(1959),
        labels: vec![ReleaseLabel::of(Some("Label One"), Some("LBL-100"))],
        ..MetadataResult::for_test(catalog, id, Some("group"))
    }
}

fn offered(results: Vec<MetadataResult>) -> Vec<String> {
    combine_results(
        LookupAnswers { catalog: results, ..LookupAnswers::default() },
        &CandidateText::default(),
        FolderAudio::UNPROVEN,
    )
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
            pressing(LookupCatalog::Discogs, "300"),
            pressing(LookupCatalog::Discogs, "20"),
            pressing(LookupCatalog::Discogs, "1000"),
        ]),
        vec!["20"]
    );
}

/// A row both catalogs list is kept over a look-alike only one lists, whatever
/// their ids.
#[test]
fn a_row_both_catalogs_list_is_kept_over_a_look_alike() {
    let linked = |mut result: MetadataResult| {
        result.links = vec![bae_catalogs::records::MetadataRef::new(
            bae_catalogs::catalog::Catalog::Discogs,
            "5000",
        )];
        result
    };
    let kept = offered(vec![
        pressing(LookupCatalog::Discogs, "20"),
        pressing(LookupCatalog::Discogs, "5000"),
        linked(pressing(LookupCatalog::MusicBrainz, "mb-1")),
    ]);
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert!(!kept.contains(&"20".to_string()), "{kept:?}");
}

/// Ids and notes no badge shows are not what a row shows: rows differing only
/// in them are listed once.
#[test]
fn rows_differing_only_in_their_ids_and_notes_are_listed_once() {
    let noted = |id: &str, note: &str| {
        let mut result = pressing(LookupCatalog::Discogs, id);
        result.notes = vec![note.to_string()];
        result
    };
    assert_eq!(offered(vec![noted("20", "Plant Alpha"), noted("30", "Plant Beta")]), vec!["20"]);
}

/// A row whose second record states another year than its lead lists both,
/// so it is listed beside a row whose records agree on the lead's year.
#[test]
fn rows_whose_records_state_different_years_are_both_listed() {
    let linked = |id: &str, partner: &str| {
        let mut result = pressing(LookupCatalog::MusicBrainz, id);
        result.links = vec![bae_catalogs::records::MetadataRef::new(
            bae_catalogs::catalog::Catalog::Discogs,
            partner,
        )];
        result
    };
    let partner = |id: &str, year: i32| MetadataResult {
        year: Some(year),
        ..pressing(LookupCatalog::Discogs, id)
    };
    let kept = offered(vec![
        linked("mb-1", "dg-1"),
        partner("dg-1", 1959),
        linked("mb-2", "dg-2"),
        partner("dg-2", 1960),
    ]);
    // Two rows of two records each; one row would be left out if the rows
    // were compared by their lead's year alone.
    assert_eq!(kept.len(), 4, "{kept:?}");
}

/// A row that shows something another does not is listed beside it.
#[test]
fn rows_that_show_different_details_are_both_listed() {
    let mut boxed = pressing(LookupCatalog::Discogs, "30");
    boxed.packaging = Some(Packaging::Box);
    assert_eq!(offered(vec![pressing(LookupCatalog::Discogs, "20"), boxed]), vec!["20", "30"]);
}
