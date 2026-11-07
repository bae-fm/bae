//! A user's metadata edit of a release: what it keeps, the artists it links,
//! and the rows it writes.

use super::*;
use crate::manager::tests::*;

#[tokio::test]
async fn release_metadata_edit_preserves_records_and_audio() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = bae_records::DbTrack::new_test(&release.id, TRACK_1, "Track Title", 1);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    manager.records.insert_track(&track).await.unwrap();
    add_track_audio_sources(
        manager,
        &track,
        &[("track.flac", bae_records::source_audio::SourceAudioLayout::File)],
    )
    .await;
    let identity = bae_catalogs::records::ReleaseRecord::new(
        &bae_catalogs::records::MetadataRef::new(
            bae_catalogs::catalog::Catalog::MusicBrainz,
            "source-release",
        ),
        Some("source-group".to_string()),
    );
    manager
        .records
        .insert_release_records(&release.id, std::slice::from_ref(&identity))
        .await
        .unwrap();
    let super::release_edit::ReleaseEditContext {
        detail: stored_before, tracks: audio_before, ..
    } = manager
        .find_release_edit_context(&release.id)
        .await
        .unwrap()
        .expect("the release detail is stored");

    manager
        .apply_release_metadata_user_edit(
            &release.id,
            &bae_import::ReleaseUserEdit {
                album_title: "Edited Album".to_string(),
                album_artist_assignments: vec![bae_import::Credited::plain(
                    bae_import::ArtistAssignment::named("Edited Album Artist"),
                )],
                album_year: Some(1984),
                pressing: bae_types::pressing::Pressing {
                    year: Some(1991),
                    labels: vec![bae_types::pressing::ReleaseLabel::of(
                        Some("Edited Label"),
                        Some("CAT-1"),
                    )],
                    facts: bae_types::pressing::PressingFacts {
                        area: Some(bae_types::pressing::area("US")),
                        ..Default::default()
                    },
                    barcodes: vec!["123456789".to_string()],
                },
                tracks: vec![bae_import::TrackUserEdit {
                    title: "Edited Track".to_string(),
                    artist_assignments: bae_import::TrackArtistAssignments::Explicit(vec![
                        bae_import::Credited::plain(bae_import::ArtistAssignment::named(
                            "Edited Track Artist",
                        )),
                    ]),
                    file: None,
                }],
            },
        )
        .await
        .unwrap();

    let super::release_edit::ReleaseEditContext {
        detail: stored_after, tracks: audio_after, ..
    } = manager
        .find_release_edit_context(&release.id)
        .await
        .unwrap()
        .expect("the edited release remains stored");
    let segments = |tracks: &[bae_records::DbReleaseEditTrack]| {
        tracks
            .iter()
            .map(|track| (track.audio_format_count, track.segments.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(segments(&audio_after), segments(&audio_before));
    assert_eq!(stored_after.files, stored_before.files);
    assert_eq!(manager.records.get_release_records(&release.id).await.unwrap(), vec![identity]);
    assert_eq!(stored_after.release.id, release.id);
    assert_eq!(stored_after.release.album_id, album.id);
    assert_eq!(stored_after.release.pressing.year, Some(1991));
    assert_eq!(stored_after.release.pressing.barcodes, ["123456789"]);
    assert_eq!(stored_after.tracks[0].track.title, "Edited Track");
    // The track's place is not in the edit: it stays where the import put it.
    assert_eq!(stored_after.tracks[0].track.side, track.side);
    assert_eq!(stored_after.tracks[0].track.track_number, track.track_number);
    assert_eq!(
        manager
            .records
            .get_shown_artists_for_track(&track.id)
            .await
            .unwrap()
            .into_iter()
            .map(|artist| artist.artist.name)
            .collect::<Vec<_>>(),
        vec!["Edited Track Artist"]
    );
}

/// Saving an edit whose album artist is named, not picked, links the library
/// artist of that name when the library already holds one — as saving the
/// same edit a second time does.
#[tokio::test]
async fn a_named_album_artist_already_in_the_library_is_linked() {
    let (manager, _temp_dir, album, release) = manager_with_release().await;
    let edit = bae_import::ReleaseUserEdit {
        album_title: "Album Title".to_string(),
        album_artist_assignments: vec![
            bae_import::Credited::plain(bae_import::ArtistAssignment::named("Album Artist One")),
            bae_import::Credited::plain(bae_import::ArtistAssignment::named("Album Artist Two")),
        ],
        album_year: None,
        pressing: bae_types::pressing::Pressing::blank(),
        tracks: Vec::new(),
    };
    manager.library.manager().apply_release_metadata_user_edit(&release.id, &edit).await.unwrap();
    let artists = manager.library.manager().get_artists_for_album(&album.id).await.unwrap();

    manager.library.manager().apply_release_metadata_user_edit(&release.id, &edit).await.unwrap();

    assert_eq!(manager.library.manager().get_artists_for_album(&album.id).await.unwrap(), artists);
    assert_eq!(
        artists.into_iter().map(|artist| artist.artist.name).collect::<Vec<_>>(),
        vec!["Album Artist One", "Album Artist Two"]
    );
}

/// A credit list naming one artist twice keeps both credits: each place in
/// the list is its own row.
#[tokio::test]
async fn an_album_credit_list_naming_one_artist_twice_keeps_both_credits() {
    let (manager, _temp_dir, album, release) = manager_with_release().await;
    let edit = bae_import::ReleaseUserEdit {
        album_title: "Album Title".to_string(),
        album_artist_assignments: vec![
            bae_import::Credited::plain(bae_import::ArtistAssignment::named("Album Artist One")),
            bae_import::Credited::plain(bae_import::ArtistAssignment::named("Album Artist Two")),
            bae_import::Credited::plain(bae_import::ArtistAssignment::named("Album Artist One")),
        ],
        album_year: None,
        pressing: bae_types::pressing::Pressing::blank(),
        tracks: Vec::new(),
    };

    manager.library.manager().apply_release_metadata_user_edit(&release.id, &edit).await.unwrap();

    let artists = manager.library.manager().get_artists_for_album(&album.id).await.unwrap();
    assert_eq!(
        artists.iter().map(|artist| artist.artist.name.as_str()).collect::<Vec<_>>(),
        vec!["Album Artist One", "Album Artist Two", "Album Artist One"]
    );
    assert_eq!(artists[0].artist.id, artists[2].artist.id, "one library artist, credited twice");
}

/// An edit of a two-track release crediting two album artists and, per track,
/// the artists `track_artists` names, in order.
fn credited_release_edit(track_artists: [&[&str]; 2]) -> bae_import::ReleaseUserEdit {
    bae_import::ReleaseUserEdit {
        album_title: "Album Title".to_string(),
        album_artist_assignments: vec![
            bae_import::Credited::plain(bae_import::ArtistAssignment::named("Album Artist One")),
            bae_import::Credited::plain(bae_import::ArtistAssignment::named("Album Artist Two")),
        ],
        album_year: Some(2024),
        pressing: bae_types::pressing::Pressing::blank(),
        tracks: track_artists
            .iter()
            .enumerate()
            .map(|(index, artists)| bae_import::TrackUserEdit {
                title: format!("Track {}", index + 1),
                artist_assignments: bae_import::TrackArtistAssignments::Explicit(
                    artists
                        .iter()
                        .map(|name| {
                            bae_import::Credited::plain(bae_import::ArtistAssignment::named(*name))
                        })
                        .collect(),
                ),
                file: None,
            })
            .collect(),
    }
}

async fn queued_write_rows(manager: &LibraryManager) -> Vec<Vec<String>> {
    manager.store.queued_write_rows_for_test().await.unwrap()
}

/// Saving a release's metadata again without changing it is no write: nothing
/// waits to sync.
#[tokio::test]
async fn saving_unchanged_metadata_writes_nothing() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    insert_n_tracks(&manager.library.manager().records, &release.id, 2).await;
    let edit = credited_release_edit([
        &["Track Artist One", "Track Artist Two"],
        &["Track Artist One", "Track Artist Three"],
    ]);
    manager.library.manager().apply_release_metadata_user_edit(&release.id, &edit).await.unwrap();
    let writes = queued_write_rows(manager.library.manager()).await;

    manager.library.manager().apply_release_metadata_user_edit(&release.id, &edit).await.unwrap();

    assert_eq!(queued_write_rows(manager.library.manager()).await, writes);
}

/// Changing one track's credit rewrites the row of that place in its list
/// and nothing else: every credit the edit leaves alone is left alone in the
/// store.
#[tokio::test]
async fn changing_one_track_credit_writes_only_that_credit() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    insert_n_tracks(&manager.library.manager().records, &release.id, 2).await;
    manager
        .library
        .manager()
        .apply_release_metadata_user_edit(
            &release.id,
            &credited_release_edit([
                &["Track Artist One", "Track Artist Two"],
                &["Track Artist One", "Track Artist Three"],
            ]),
        )
        .await
        .unwrap();
    let writes = queued_write_rows(manager.library.manager()).await.len();

    manager
        .library
        .manager()
        .apply_release_metadata_user_edit(
            &release.id,
            &credited_release_edit([
                &["Track Artist One", "Track Artist Four"],
                &["Track Artist One", "Track Artist Three"],
            ]),
        )
        .await
        .unwrap();

    let rows = queued_write_rows(manager.library.manager()).await;
    assert_eq!(rows.len(), writes + 1);
    let credit_rows = rows[writes]
        .iter()
        .filter(|row| row.starts_with("album_artists") || row.starts_with("track_artists"))
        .collect::<Vec<_>>();
    assert_eq!(credit_rows, ["track_artists"], "the one place whose artist changed");
    assert_eq!(rows[writes].iter().filter(|row| *row == "artists").count(), 1, "the new artist");
    let first_track =
        &manager.library.records().get_tracks_for_release(&release.id).await.unwrap()[0];
    assert_eq!(
        manager
            .library
            .records()
            .get_shown_artists_for_track(&first_track.id)
            .await
            .unwrap()
            .into_iter()
            .map(|artist| artist.artist.name)
            .collect::<Vec<_>>(),
        vec!["Track Artist One", "Track Artist Four"]
    );
}
