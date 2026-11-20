use super::*;
use bae_catalogs::test_releases::{mb_printed_track, mb_release};
use bae_store::test_fixtures::empty_db;
use bae_types::mediums::SideHeading;

/// An imaginary MusicBrainz release of two records, each with sides lettered
/// as printed: A and B, then C and D.
fn two_records() -> SourceRelease {
    let record = |sides: [&str; 2]| bae_catalogs::musicbrainz::MbMedium {
        discs: vec![],
        format: Some("Vinyl".to_string()),
        tracks: sides
            .iter()
            .map(|side| mb_printed_track(&format!("{side}1"), "Track Title"))
            .collect(),
    };
    let response = bae_catalogs::musicbrainz::MbReleaseResponse {
        media: vec![record(["A", "B"]), record(["C", "D"])],
        ..mb_release("release-1", "group-1", "Album Title")
    };
    bae_catalogs::payloads::ReleasePayloads::for_test(
        bae_catalogs::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
            "release-1",
        ),
        serde_json::to_string(&response).expect("the release serializes"),
        Vec::new(),
    )
    .extract()
    .expect("the release extracts")
}

/// A tie names the tied record's sides by their headings on the release,
/// lettered on from the record before it; a count refusal names both
/// counts.
#[tokio::test]
async fn a_reading_refusal_is_the_import_refusal_it_names() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = ImportTables::new(db);
    let release = two_records();
    let tied = tables.reading_refusal(&release, ReadingRefusal::Tied { mediums: vec![2] }).await;
    let sided = |letter: &str| SideHeading::Sided { side_letter: letter.to_owned() };
    assert!(
        matches!(&tied, ImportError::TiedMediums { sides } if *sides == [sided("C"), sided("D")]),
        "{tied:?}"
    );
    let counted = tables
        .reading_refusal(&release, ReadingRefusal::TrackCount { folder: 2, release: 7 })
        .await;
    assert!(
        matches!(counted, ImportError::MetadataTrackCount { folder_tracks: 2, release_tracks: 7 }),
        "{counted:?}"
    );
}
