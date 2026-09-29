//! The notes a folder names a row by, read from the catalogs' own documents:
//! a Discogs release's notes and identifiers, a MusicBrainz release's
//! disambiguation and annotation.

use super::*;
use crate::import::search::{discogs_release_to_metadata, musicbrainz_release_notes};
use crate::import::Catalog;
use crate::signals::{TextLine, TextOrigin};

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

/// A Discogs release document as its catalog serves it, read as a result.
fn discogs(document: serde_json::Value) -> MetadataResult {
    let release = crate::discogs::client::parse_discogs_release_json(&document.to_string())
        .expect("the release document parses");
    discogs_release_to_metadata(&release)
}

/// A MusicBrainz release document as its catalog serves it, as a result
/// carrying its notes.
fn musicbrainz(release_id: &str, document: serde_json::Value) -> MetadataResult {
    let release: crate::musicbrainz::MbReleaseResponse =
        serde_json::from_value(document).expect("the release document parses");
    MetadataResult {
        notes: musicbrainz_release_notes(&release),
        ..MetadataResult::for_test(Catalog::MusicBrainz, release_id, None)
    }
}

/// The note each row is named for, row by row, for rows of one record each.
fn named(rows: &[MetadataResult], text: &CandidateText) -> Vec<Option<String>> {
    named_notes(rows.iter().map(std::slice::from_ref), text)
        .into_iter()
        .map(|records| records.into_iter().next().flatten())
        .collect()
}

/// Two pressings made in one country, one by a plant only its Discogs notes
/// and companies name, the other's MusicBrainz disambiguation naming the
/// country alone. The country is in both rows' notes and says nothing; the
/// plant names the first, by the line of its notes that states it.
#[test]
fn the_release_notes_cancel_what_both_rows_write_and_name_the_plant() {
    let plant = discogs(serde_json::json!({
        "id": 5001,
        "title": "Album Title",
        "artists": [{ "id": 1, "name": "Artist Name" }],
        "notes": "Recorded at Studio Name.\r\n\r\nCD made in W-Germany by Discoplant\r\n",
        "companies": [{ "name": "Discoplant", "entity_type_name": "Made By" }],
    }));
    let country = musicbrainz(
        "mb-country",
        serde_json::json!({
            "id": "mb-country",
            "title": "Album Title",
            "disambiguation": "Made in W.-Germany",
            "cover-art-archive": { "front": false, "darkened": false },
        }),
    );

    assert_eq!(
        named(
            &[plant, country],
            &folder("Made in W.-Germany by Discoplant")
        ),
        vec![Some("CD made in W-Germany by Discoplant".to_string()), None]
    );
}

/// Two pressings alike but for the rights society their Discogs identifiers
/// state: the folder stating one names the pressing that states it, and the
/// society both write says nothing.
#[test]
fn a_rights_society_identifier_names_its_pressing() {
    let pressing = |id: u64, society: &str| {
        discogs(serde_json::json!({
            "id": id,
            "title": "Album Title",
            "artists": [{ "id": 1, "name": "Artist Name" }],
            "identifiers": [
                { "type": "Barcode", "value": "0 12345 67890 5" },
                { "type": "Rights Society", "value": society },
                { "type": "Label Code", "value": "LC 00000" },
            ],
        }))
    };

    assert_eq!(
        named(
            &[pressing(6001, "BIEM/GEMA"), pressing(6002, "BIEM/MCPS")],
            &folder("Album Title [BIEM/GEMA]")
        ),
        vec![Some("BIEM/GEMA".to_string()), None]
    );
}

/// A MusicBrainz release's annotation names its pressing line by line, like
/// a Discogs release's notes.
#[test]
fn an_annotation_line_names_its_pressing() {
    let pressing = |release_id: &str, annotation: &str| {
        musicbrainz(
            release_id,
            serde_json::json!({
                "id": release_id,
                "title": "Album Title",
                "annotation": annotation,
                "cover-art-archive": { "front": false, "darkened": false },
            }),
        )
    };

    assert_eq!(
        named(
            &[
                pressing("mb-one", "Matrix: DISC-1\nMastering SID: IFPI L001"),
                pressing("mb-two", "Matrix: DISC-1\nMastering SID: IFPI L002"),
            ],
            &folder("IFPI L002")
        ),
        vec![None, Some("Mastering SID: IFPI L002".to_string())]
    );
}
