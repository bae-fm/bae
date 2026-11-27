//! A Done row: the library release imported from its folder's files, as the
//! list reads it.

use super::*;
use bae_catalogs::catalog::Catalog;
use bae_records::test_fixtures::artists_apart::{meet_musicbrainz_artist, musicbrainz_artist};
use bae_records::test_fixtures::write_cover;
use bae_store::test_fixtures::fixed_now;
use bae_util::test_channels::HANG_GUARD;

/// A Done row reads the library release imported from the folder's files —
/// its album's title and credits, its cover and the catalogs its records
/// name — and nothing of the candidate's draft, link or cover; and an edit to
/// the library release reaches the row.
#[tokio::test]
async fn a_done_row_reads_its_library_release() {
    let lib = Library::new().await;
    let records = lib.records();
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let artist_mbid = "artist-mbid";
    meet_musicbrainz_artist(&records, artist_mbid, "Artist Name", None).await;
    let pressing = bae_catalogs::records::ReleaseRecord::new(
        &bae_catalogs::records::MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        Some("mb-group".to_string()),
    );
    lib.run(
        lib.scan("release")
            + lib.draft("release", "Draft Title")
            + lib.linked("release")
            + lib.import_of(
                lib.candidate("release"),
                "release",
                "2026-01-01T00:00:00Z",
                "Album Title",
                vec![pressing.clone()],
            ),
    )
    .await;
    // The album takes its release group's id.
    let album_id = bae_records::identity::album_id_for_records(std::slice::from_ref(&pressing))
        .expect("the record names a group");
    let release_id = lib.release_id("release");
    let artist = musicbrainz_artist(artist_mbid);
    let artist_row =
        records.find_artist_by_id(&artist).await.unwrap().expect("the library holds the artist");
    records
        .save_release_metadata_for_test(&release_id, |rows| {
            rows.picked.push(artist.clone());
            rows.credits.push(artist_row);
            rows.album_artists = vec![bae_records::DbAlbumArtist::new(
                &coven::UuidIds,
                &album_id,
                &artist,
                0,
                fixed_now(),
            )];
        })
        .await
        .unwrap();
    let version = write_cover(&records, &release_id, "the library cover").await;
    let list = in_path_order(crate::TriageTab::Done);
    let whole = bae_store::ListWindow::top(50);
    let mut live = tables.subscribe_import_window(list.clone(), whole);
    let first = live.next_inline().await.expect("the query stays open").result.unwrap();
    let done = window_rows(&first);
    assert_eq!(done.len(), 1);
    let done = &done[0];
    assert_eq!(done.placement, crate::TriagePlacement::Done);
    assert_eq!(
        done.import_status,
        Some(crate::TriageImportStatus::Complete {
            release: crate::ImportedRelease {
                release_id: release_id.clone(),
                album_id: album_id.clone(),
            },
        })
    );
    assert_eq!(
        done.reading.album().cloned(),
        Some(crate::AlbumLine::TitleAndArtists {
            title: "Album Title".to_string(),
            artist_line: bae_types::credits::CreditLine::of_name("Artist Name"),
        })
    );
    assert!(matches!(
        &first.rows[0].0,
        crate::ImportListItem::Candidate { live, .. } if live.status.is_none()
    ));
    assert_eq!(
        done.cover,
        Some(crate::CoverImageSource::Library {
            image: bae_records::ImageRef {
                id: release_id.clone(),
                version,
                image_type: LibraryImageType::Cover,
            },
        })
    );
    assert!(
        matches!(
            &done.reading,
            crate::triage::TriageReading::Identified { records, .. } if records == &[pressing]
        ),
        "{:?}",
        done.reading
    );
    assert_eq!(done.metadata_provenance, None);
    assert_eq!(done.release_link, None);

    records.rename_album_for_test(&album_id, "Retitled").await.unwrap();
    let retitled = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("retitling the album wakes the list")
        .expect("the query stays open")
        .result
        .unwrap();
    let artist_line = bae_types::credits::CreditLine::of_name("Artist Name");
    let line = crate::AlbumLine::TitleAndArtists { title: "Retitled".to_string(), artist_line };
    assert_eq!(window_rows(&retitled)[0].reading.album(), Some(&line));
}

/// A Done row reads as its library release's records describe it: identified
/// when a record names its pressing, identified as an album when the records
/// name only its album, and prefilled when no catalog describes it.
#[tokio::test]
async fn a_done_row_reads_as_its_library_release_s_records_describe_it() {
    let lib = Library::new().await;
    let pressing = bae_catalogs::records::ReleaseRecord::new(
        &bae_catalogs::records::MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        Some("mb-group".to_string()),
    );
    let album = bae_catalogs::records::ReleaseRecord::album(
        &bae_catalogs::records::MetadataRef::new(Catalog::Discogs, "master-7"),
    );
    let imported = |folder: &'static str, records: Vec<bae_catalogs::records::ReleaseRecord>| {
        lib.scan(folder)
            + lib.import_of(
                lib.candidate(folder),
                folder,
                "2026-01-01T00:00:00Z",
                "Album Title",
                records,
            )
    };
    lib.run(
        imported("a-pressed", vec![pressing.clone(), album.clone()])
            + imported("b-album", vec![album.clone()])
            + imported("c-bare", Vec::new()),
    )
    .await;

    let list = in_path_order(crate::TriageTab::Done);
    let read = crate::tables::ImportTables::new(lib.db.clone())
        .read_import_window(list.clone(), bae_store::ListWindow::top(50))
        .await
        .unwrap();
    let readings: Vec<_> = window_rows(&read).into_iter().map(|row| row.reading).collect();
    let line = crate::AlbumLine::TitleAlone { title: "Album Title".to_string() };
    assert_eq!(
        readings,
        [
            crate::triage::TriageReading::Identified {
                album: line.clone(),
                records: vec![pressing, album.clone()],
            },
            crate::triage::TriageReading::IdentifiedAlbum {
                album: line.clone(),
                records: vec![album],
            },
            crate::triage::TriageReading::Prefilled { album: line },
        ]
    );
}
