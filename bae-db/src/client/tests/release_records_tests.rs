//! Which library album an import's release records name, and what the
//! status probe says a catalog release is in the library as.

use crate::test_fixtures::temp_db;
use crate::{Database, DbAlbum, DbArtist, DbRelease, DbTrack};
use bae_catalogs::catalog::Catalog;
use bae_catalogs::records::ReleaseRecord;
use chrono::Utc;
use tempfile::TempDir;
use uuid::Uuid;

/// A store holding a single test artist that the helpers below reference
/// for inserted albums.
async fn setup_test_db_with_artist() -> (Database, TempDir) {
    let (db, tmp) = temp_db().await;
    let artist = DbArtist {
        id: TEST_ARTIST_ID.to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    db.insert_artist(&artist).await.unwrap();
    (db, tmp)
}

/// The artist `setup_test_db_with_artist` inserts, which every test album
/// credits.
const TEST_ARTIST_ID: &str = "e36744a5-1a36-460f-891c-e7e558034edf";

fn make_album(title: &str) -> DbAlbum {
    let now = Utc::now();
    DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: title.to_string(),
        year: Some(2024),
        is_compilation: false,
        created_at: now,
    }
}

fn make_release(album_id: &str) -> DbRelease {
    let now = Utc::now();
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: bae_types::pressing::Pressing {
            year: Some(2024),
            labels: Vec::new(),
            facts: Default::default(),
            barcodes: Vec::new(),
        },

        source_folder_name: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    }
}

fn make_track(release_id: &str, number: i32) -> DbTrack {
    let now = Utc::now();
    DbTrack {
        id: Uuid::new_v4().to_string(),
        release_id: release_id.to_string(),
        title: format!("Track {}", number),
        medium_id: crate::identity::release_medium_id(release_id, 1),
        side: std::num::NonZeroU8::MIN,
        track_number: std::num::NonZeroU32::new(
            u32::try_from(number).expect("a test track number fits u32"),
        )
        .expect("track numbers count from one"),
        duration_ms: 180000,
        created_at: now,
    }
}

/// Exactly one record of a release reads its draft; these fixtures put that on
/// MusicBrainz, so a release carrying both records still has one.
fn mb_identity(group: &str, release: &str) -> ReleaseRecord {
    ReleaseRecord::new(
        &bae_catalogs::records::MetadataRef::new(Catalog::MusicBrainz, release),
        Some(group.to_string()),
    )
}

fn discogs_identity(master: &str, release: &str) -> ReleaseRecord {
    ReleaseRecord::new(
        &bae_catalogs::records::MetadataRef::new(Catalog::Discogs, release),
        Some(master.to_string()),
    )
}

/// Insert an album + release with the supplied identity rows. Mirrors
/// what the import commit path does, minus tracks/files; tests only
/// need the identity rows reachable from the album.
async fn insert_with_identities(
    db: &Database,
    album: &DbAlbum,
    release: &DbRelease,
    identities: &[ReleaseRecord],
) {
    let track = make_track(&release.id, 1);
    db.insert_album_with_release_and_tracks(
        album,
        &[crate::DbAlbumArtist::new_test(&album.id, TEST_ARTIST_ID)],
        release,
        &[crate::DbReleaseMedium::new_test(&release.id)],
        &[track],
        &[],
    )
    .await
    .unwrap();
    db.insert_release_records(&release.id, identities).await.unwrap();
}

/// A store holding one album titled `title`, whose single release carries
/// `identities`. The `TempDir` comes back because dropping it deletes the
/// store.
async fn album_with(
    title: &str,
    identities: &[ReleaseRecord],
) -> (Database, TempDir, DbAlbum, DbRelease) {
    let (db, tmp) = setup_test_db_with_artist().await;
    let album = make_album(title);
    let release = make_release(&album.id);
    insert_with_identities(&db, &album, &release, identities).await;
    (db, tmp, album, release)
}

/// The album an import carrying `identities` would join, or `None` when it
/// belongs to no album the library already holds.
async fn album_for_import(database: &Database, identities: &[ReleaseRecord]) -> Option<String> {
    database.album_records_name(identities, &[]).await.unwrap()
}

#[tokio::test]
async fn test_same_pressing_finds_existing_album() {
    let (db, _tmp, album, _release) =
        album_with("Album Title", &[mb_identity("mb-rg-456", "mb-rel-123")]).await;

    let incoming = vec![mb_identity("mb-rg-789", "mb-rel-123")];
    assert_eq!(album_for_import(&db, &incoming).await, Some(album.id));

    // Discogs side mirrors the same logic.
    let album2 = make_album("Other Album");
    let release2 = make_release(&album2.id);
    let identities2 = vec![discogs_identity("d-master-456", "d-rel-123")];
    insert_with_identities(&db, &album2, &release2, &identities2).await;

    let incoming2 = vec![discogs_identity("d-master-456", "d-rel-123")];
    let result2 = album_for_import(&db, &incoming2).await;
    assert_eq!(result2, Some(album2.id));
}

#[tokio::test]
async fn test_same_release_group_finds_existing_album() {
    let (db, _tmp, album, _release) =
        album_with("Album Title", &[mb_identity("mb-rg-456", "mb-rel-123")]).await;

    // Different release within the same group → merge into existing album.
    let incoming = vec![mb_identity("mb-rg-456", "mb-rel-999")];
    assert_eq!(album_for_import(&db, &incoming).await, Some(album.id));
}

#[tokio::test]
async fn test_same_discogs_master_finds_existing_album() {
    let (db, _tmp, album, _release) =
        album_with("Album Title", &[discogs_identity("d-master-456", "d-rel-123")]).await;

    let incoming = vec![discogs_identity("d-master-456", "d-rel-999")];
    assert_eq!(album_for_import(&db, &incoming).await, Some(album.id));
}

#[tokio::test]
async fn test_no_match_returns_none() {
    let (db, _tmp, _album, _release) =
        album_with("Existing Album", &[mb_identity("mb-rg-456", "mb-rel-123")]).await;

    // Different group + different release → no match.
    let incoming = vec![mb_identity("mb-rg-999", "mb-rel-999")];
    assert_eq!(album_for_import(&db, &incoming).await, None);

    // Empty identity vec (file metadata or direct entry) → skip lookup.
    assert_eq!(album_for_import(&db, &[]).await, None);
}

#[tokio::test]
async fn test_cross_source_no_false_merge() {
    // Existing album holds only a Discogs identity row.
    let (db, _tmp, _album, _release) =
        album_with("Album Title", &[discogs_identity("d-master-200", "d-rel-100")]).await;

    // Unrelated MB import → should not merge.
    let incoming = vec![mb_identity("mb-rg-600", "mb-rel-500")];
    assert_eq!(album_for_import(&db, &incoming).await, None);
}

#[tokio::test]
async fn test_cross_source_merge_via_path_2() {
    // An MB-rooted release that carries both an MB and a Discogs row
    // (because MB url-rels resolved to a Discogs release at commit
    // time) is reachable from a later Discogs-only import of the same
    // master.
    let (db, _tmp, album, _release) = album_with(
        "Album Title",
        &[mb_identity("mb-rg-100", "mb-rel-50"), discogs_identity("d-master-200", "d-rel-75")],
    )
    .await;

    // Later Discogs import of the same master, different pressing.
    let incoming = vec![discogs_identity("d-master-200", "d-rel-300")];
    assert_eq!(album_for_import(&db, &incoming).await, Some(album.id));
}

#[tokio::test]
async fn test_cross_source_merge_via_path_2_inverse() {
    // Inverse of `test_cross_source_merge_via_path_2`: existing album
    // holds only a Discogs row. A later MB-rooted import resolves a
    // cross-link Discogs row pointing to the same master and thus
    // attaches via the Discogs match — even though the existing album
    // has no MB row to compare against.
    let (db, _tmp, album, _release) =
        album_with("Album Title", &[discogs_identity("d-master-200", "d-rel-75")]).await;

    // MB-rooted import: an MB row plus a Discogs cross-link to the
    // same Discogs master the existing album sits on.
    let incoming =
        vec![mb_identity("mb-rg-100", "mb-rel-50"), discogs_identity("d-master-200", "d-rel-300")];
    assert_eq!(album_for_import(&db, &incoming).await, Some(album.id));
}

#[tokio::test]
async fn test_file_tags_import_skips_lookup() {
    // File-metadata imports never deduplicate against existing releases —
    // they always create a fresh album.
    let (db, _tmp, _album, _release) =
        album_with("Existing Album", &[mb_identity("mb-rg-1", "mb-rel-1")]).await;

    // Empty identity vec — no match should be returned.
    assert_eq!(album_for_import(&db, &[]).await, None);
}

#[tokio::test]
async fn test_merge_release_into_existing_album() {
    // A second release with the same MB group attaches to the
    // existing album via `album_records_name` returning
    // `Some(album_id)`. The caller redirects the new release's
    // album_id and inserts it as a sibling.
    let (db, _tmp, album, release1) =
        album_with("Album Title", &[mb_identity("mb-rg-500", "mb-rel-100")]).await;

    // Lookup returns the existing album for a release in the same
    // MB group.
    let incoming = vec![mb_identity("mb-rg-500", "mb-rel-200")];
    let existing_album_id = album_for_import(&db, &incoming).await;
    assert_eq!(existing_album_id, Some(album.id.clone()));

    // Insert a sibling release pointing at the existing album.
    let mut release2 = make_release(&album.id);
    release2.album_id = existing_album_id.unwrap();
    let track2 = make_track(&release2.id, 1);
    db.insert_release_with_tracks(
        &release2,
        &[crate::DbReleaseMedium::new_test(&release2.id)],
        &[track2],
        &[],
    )
    .await
    .unwrap();
    db.insert_release_records(&release2.id, &incoming).await.unwrap();

    let releases = db.get_releases_for_album(&album.id).await.unwrap();
    assert_eq!(releases.len(), 2);
    let release_ids: Vec<&str> = releases.iter().map(|r| r.id.as_str()).collect();
    assert!(release_ids.contains(&release1.id.as_str()));
    assert!(release_ids.contains(&release2.id.as_str()));
}

// ── check_releases_in_library (identity-based status badges) ─────

#[tokio::test]
async fn test_check_release_in_library_exact_match() {
    let (db, _tmp, album, _release) =
        album_with("Album Title", &[mb_identity("mb-rg-1", "mb-rel-1")]).await;

    let checks = vec![crate::LibraryCheck {
        release_id: "mb-rel-1".to_string(),
        source: Catalog::MusicBrainz,
        source_group_id: Some("mb-rg-1".to_string()),
    }];
    let statuses = db.check_releases_in_library(&checks).await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert!(statuses[0].release_in_library());
    assert!(statuses[0].album_in_library());
    assert_eq!(statuses[0].albums[0].album_id, album.id);
    assert_eq!(statuses[0].albums[0].album_title, "Album Title");
}

#[tokio::test]
async fn test_check_album_in_library_group_only() {
    let (db, _tmp, album, _release) =
        album_with("Album Title", &[mb_identity("mb-rg-1", "mb-rel-1")]).await;

    // Different release ID, same group → album_in_library only.
    let checks = vec![crate::LibraryCheck {
        release_id: "mb-rel-OTHER".to_string(),
        source: Catalog::MusicBrainz,
        source_group_id: Some("mb-rg-1".to_string()),
    }];
    let statuses = db.check_releases_in_library(&checks).await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert!(!statuses[0].release_in_library());
    assert!(statuses[0].album_in_library());
    assert_eq!(statuses[0].albums[0].album_id, album.id);
}

#[tokio::test]
async fn test_check_album_in_library_other_pressing_of_the_group() {
    // The library holds a different pressing of the same release group. A
    // candidate naming that group is in the library as an album, and not as a
    // pressing.
    let (db, _tmp, album, _release) =
        album_with("Album Title", &[mb_identity("mb-rg-1", "mb-rel-other")]).await;

    let checks = vec![crate::LibraryCheck {
        release_id: "mb-rel-1".to_string(),
        source: Catalog::MusicBrainz,
        source_group_id: Some("mb-rg-1".to_string()),
    }];
    let statuses = db.check_releases_in_library(&checks).await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert!(!statuses[0].release_in_library());
    assert!(statuses[0].album_in_library());
    assert_eq!(statuses[0].albums[0].album_id, album.id);
}

#[tokio::test]
async fn test_check_pressing_match_wins_over_sibling_pressing_row() {
    // Two releases in the group: one at a different pressing, one at the
    // pressing the check names. The pressing match is the row that answers.
    let (db, _tmp, album, _sibling) =
        album_with("Album Title", &[mb_identity("mb-rg-1", "mb-rel-other")]).await;
    let named = make_release(&album.id);
    let track = make_track(&named.id, 1);
    db.insert_release_with_tracks(
        &named,
        &[crate::DbReleaseMedium::new_test(&named.id)],
        &[track],
        &[],
    )
    .await
    .unwrap();
    db.insert_release_records(&named.id, &[mb_identity("mb-rg-1", "mb-rel-1")]).await.unwrap();

    let checks = vec![crate::LibraryCheck {
        release_id: "mb-rel-1".to_string(),
        source: Catalog::MusicBrainz,
        source_group_id: Some("mb-rg-1".to_string()),
    }];
    let statuses = db.check_releases_in_library(&checks).await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert!(statuses[0].release_in_library());
    assert!(statuses[0].album_in_library());
}

#[tokio::test]
async fn test_check_release_not_in_library() {
    let (db, _tmp) = setup_test_db_with_artist().await;

    let checks = vec![crate::LibraryCheck {
        release_id: "mb-rel-NONE".to_string(),
        source: Catalog::MusicBrainz,
        source_group_id: Some("mb-rg-NONE".to_string()),
    }];
    let statuses = db.check_releases_in_library(&checks).await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert!(!statuses[0].release_in_library());
    assert!(!statuses[0].album_in_library());
    assert!(statuses[0].albums.is_empty());
}

#[tokio::test]
async fn test_check_cross_source_doesnt_leak() {
    // An MB candidate against a Discogs-only library entry should
    // not match — different sources.
    let (db, _tmp, _album, _release) =
        album_with("Album Title", &[discogs_identity("d-master-1", "d-rel-1")]).await;

    let checks = vec![crate::LibraryCheck {
        release_id: "mb-rel-1".to_string(),
        source: Catalog::MusicBrainz,
        source_group_id: Some("mb-rg-1".to_string()),
    }];
    let statuses = db.check_releases_in_library(&checks).await.unwrap();
    assert!(!statuses[0].release_in_library());
    assert!(!statuses[0].album_in_library());
}

// ── An import whose records name two library albums (the applied record decides) ──

/// A library holding album X filed under MusicBrainz group `mb-rg-x` and
/// album Y filed under Discogs master `d-master-y`.
async fn two_albums() -> (Database, TempDir, DbAlbum, DbAlbum) {
    let (db, tmp, x, _) = album_with("Album X", &[mb_identity("mb-rg-x", "mb-rel-x")]).await;
    let y = make_album("Album Y");
    let release_y = make_release(&y.id);
    insert_with_identities(&db, &y, &release_y, &[discogs_identity("d-master-y", "d-rel-y")]).await;
    (db, tmp, x, y)
}

/// The applied record — the list's first — names the album, never the row
/// SQLite returns first: applied from MusicBrainz it joins X, applied from
/// Discogs Y.
#[tokio::test]
async fn the_applied_record_decides_between_two_albums() {
    let (db, _tmp, x, y) = two_albums().await;
    let pressing = |catalog, release: &str, album: &str| {
        ReleaseRecord::new(
            &bae_catalogs::records::MetadataRef::new(catalog, release),
            Some(album.to_string()),
        )
    };
    let musicbrainz_applied = vec![
        pressing(Catalog::MusicBrainz, "mb-rel-new", "mb-rg-x"),
        pressing(Catalog::Discogs, "d-rel-new", "d-master-y"),
    ];
    assert_eq!(album_for_import(&db, &musicbrainz_applied).await, Some(x.id));
    let discogs_applied = vec![
        pressing(Catalog::Discogs, "d-rel-new", "d-master-y"),
        pressing(Catalog::MusicBrainz, "mb-rel-new", "mb-rg-x"),
    ];
    assert_eq!(album_for_import(&db, &discogs_applied).await, Some(y.id));
}

/// An applied record naming no album the library holds leaves it to its
/// partner's; neither naming one is a new album.
#[tokio::test]
async fn an_applied_record_naming_no_album_takes_its_partner_s() {
    let (db, _tmp, _x, y) = two_albums().await;
    let records = vec![
        ReleaseRecord::new(
            &bae_catalogs::records::MetadataRef::new(Catalog::MusicBrainz, "mb-rel-new"),
            Some("mb-rg-unknown".to_string()),
        ),
        discogs_identity("d-master-y", "d-rel-new"),
    ];
    assert_eq!(album_for_import(&db, &records).await, Some(y.id));
    let neither = vec![ReleaseRecord::new(
        &bae_catalogs::records::MetadataRef::new(Catalog::MusicBrainz, "mb-rel-new"),
        Some("mb-rg-unknown".to_string()),
    )];
    assert_eq!(album_for_import(&db, &neither).await, None);
}

/// The status probe names every album holding a release of the group, those
/// holding the release itself first.
#[tokio::test]
async fn the_status_probe_names_every_album_of_the_group() {
    let (db, _tmp, x, _) = album_with("Album X", &[mb_identity("mb-rg-1", "mb-rel-1")]).await;
    let other = make_album("Album Other");
    let release = make_release(&other.id);
    insert_with_identities(&db, &other, &release, &[mb_identity("mb-rg-1", "mb-rel-2")]).await;

    let statuses = db
        .check_releases_in_library(&[crate::LibraryCheck {
            release_id: "mb-rel-2".to_string(),
            source: Catalog::MusicBrainz,
            source_group_id: Some("mb-rg-1".to_string()),
        }])
        .await
        .unwrap();
    let albums: Vec<(&str, bool)> = statuses[0]
        .albums
        .iter()
        .map(|album| (album.album_id.as_str(), album.holds_release))
        .collect();
    assert_eq!(albums, [(other.id.as_str(), true), (x.id.as_str(), false)]);
}
