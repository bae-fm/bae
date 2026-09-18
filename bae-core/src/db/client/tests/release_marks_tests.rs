//! The names read off an object: what a commit keeps, and what the read hands
//! back.

use super::*;
use crate::db::{DbAlbum, DbArtist, DbRelease};
use crate::import::{MarkKind, ReleaseMark};
use crate::signals::{ImageRegion, SignalOrigin, SourcedValue};

const MARKED_ARTIST: &str = "a1a1a1a1-0000-4000-8000-000000000001";
const MARKED_ALBUM: &str = "b1b1b1b1-0000-4000-8000-000000000001";
const MARKED_RELEASE: &str = "c1c1c1c1-0000-4000-8000-000000000001";

fn seen_on_artwork(value: &str, file: &str, region: Option<ImageRegion>) -> ReleaseMark {
    ReleaseMark {
        corroborated: false,
        kind: MarkKind::Barcode,
        sighting: SourcedValue::in_file(value.to_string(), SignalOrigin::Artwork, file.to_string())
            .at(region),
    }
}

/// The commit writes one row per sighting and the read hands every one of them
/// back whole — the file it was read off and the box the detector drew around
/// it included, because a later crop of the scan has nothing else to go on.
#[tokio::test]
async fn a_commit_keeps_every_sighting_whole() {
    let (db, _tmp) = empty_db().await;
    let now = fixed_now();
    let artist = DbArtist {
        id: MARKED_ARTIST.to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    db.insert_artist(&artist).await.unwrap();
    let album = DbAlbum {
        id: MARKED_ALBUM.to_string(),
        title: "Album Title".to_string(),
        artist_id: artist.id.clone(),
        year: None,
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease::new_test(&album.id, MARKED_RELEASE);

    let marks = vec![
        ReleaseMark {
            corroborated: true,
            kind: MarkKind::DiscId,
            sighting: SourcedValue::in_file(
                "XyZ.abc-123".to_string(),
                SignalOrigin::DiscToc,
                "Album.log".to_string(),
            ),
        },
        seen_on_artwork(
            "0075678164521",
            "back.jpg",
            ImageRegion::new(0.1, 0.2, 0.3, 0.4),
        ),
        seen_on_artwork("0075678164521", "obi.jpg", None),
        ReleaseMark {
            corroborated: false,
            kind: MarkKind::CatalogNumber,
            sighting: SourcedValue::new("7559-60691-2".to_string(), SignalOrigin::FolderName),
        },
    ];

    db.finalize_import_atomic(
        crate::db::ImportCommitGuard::UncheckedTestSetup,
        Some(&album),
        &release,
        &[],
        crate::db::ImportRows {
            marks: &marks,
            ..Default::default()
        },
        Vec::new(),
        None,
        &[],
        None,
        crate::config::HomeStorage::Opaque,
        &[],
    )
    .await
    .unwrap();

    assert_eq!(
        db.get_release_marks(&release.id).await.unwrap(),
        marks,
        "every sighting survives the commit, region and all"
    );

    db.set_records_atomic(&release.id, &[], true, None, &album.id, &album.id, None)
        .await
        .unwrap();
    let replaced = db.get_release_marks(&release.id).await.unwrap();
    assert_eq!(replaced.len(), marks.len());
    assert!(
        replaced.iter().all(|mark| !mark.corroborated),
        "choosing another source must not keep proof for the previous record"
    );
}

/// A release nothing was read off carries no marks, and the read says so
/// rather than inventing one.
#[tokio::test]
async fn a_release_nothing_was_read_off_carries_no_marks() {
    let (db, _tmp) = empty_db().await;
    assert!(db
        .get_release_marks(MARKED_RELEASE)
        .await
        .unwrap()
        .is_empty());
}
