//! The statement that offers a candidate's covers says what `pick_covers`
//! says over the same stored releases, while the draft was read from the
//! linked pressing.

use super::claimed_records::tests::{discogs_release, linked, musicbrainz_primary, pressing};
use super::*;
use crate::tables::tests::*;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::records::LookupRef;
use bae_catalogs::remote_image::{CoverLabel, CoverStanding, RemoteCover, RemoteImageSet};
use bae_catalogs::source_release::SourceRelease;
use bae_store::Store;

fn discogs_image(url: &str, standing: CoverStanding) -> RemoteCover {
    RemoteCover {
        image: RemoteImageSet::with_copies(url.to_owned(), Vec::new()),
        label: CoverLabel::Lead { release_group: false },
        source: Catalog::Discogs,
        standing,
    }
}

/// `release` with the covers its pressing and its album offer.
fn with_covers(
    mut release: SourceRelease,
    own: Vec<RemoteCover>,
    album: Vec<RemoteCover>,
) -> SourceRelease {
    release.covers.release = own;
    release.covers.album = album;
    release
}

/// The candidate's draft read from the pressing `record` names.
async fn read_from(
    db: &Store,
    candidate: &crate::folder_scanner::FolderCandidate,
    record: &LookupRef,
) {
    let tables = crate::tables::ImportTables::new(db.clone());
    let hash = candidate.files.content_hash();
    let draft = tables.load_import_candidate_pane_rows(&hash).await.unwrap().draft.release_edit();
    tables
        .replace_metadata(
            &hash,
            &crate::CandidateKey::folder(&candidate.path),
            &draft,
            Some(&crate::MetadataProvenance::ExternalRelease { record: record.clone() }),
            &[],
        )
        .await
        .unwrap();
}

async fn offered(db: &Store, content_hash: &str) -> Vec<(String, CoverStanding)> {
    let content_hash = content_hash.to_owned();
    db.read(move |sql| offered_covers_on(&sql, &content_hash))
        .await
        .unwrap()
        .into_iter()
        .map(|cover| (cover.image.url, cover.standing))
        .collect()
}

/// A pressing linked with one partner, its draft read from it, and the covers
/// each of the two releases offers.
async fn picked(
    primary_covers: (Vec<RemoteCover>, Vec<RemoteCover>),
    partner_covers: (Vec<RemoteCover>, Vec<RemoteCover>),
) -> (Store, tempfile::TempDir, crate::folder_scanner::FolderCandidate) {
    let (db, tmp, root) = watched_root().await;
    let primary = LookupRef::new(LookupCatalog::MusicBrainz, "mb-release");
    let candidate = linked(
        &db,
        &root,
        pressing(primary.clone(), vec![LookupRef::new(LookupCatalog::Discogs, "2222")]),
        &[
            with_covers(musicbrainz_primary(), primary_covers.0, primary_covers.1),
            with_covers(discogs_release(2222, 909090), partner_covers.0, partner_covers.1),
        ],
    )
    .await;
    read_from(&db, &candidate, &primary).await;
    (db, tmp, candidate)
}

#[tokio::test]
async fn offered_covers_statement_offers_stated_covers_only_when_any() {
    let (db, _tmp, candidate) = picked(
        (vec![discogs_image("https://img/own-unstated", CoverStanding::Unstated)], Vec::new()),
        (vec![discogs_image("https://img/partner-stated", CoverStanding::Stated)], Vec::new()),
    )
    .await;
    assert_eq!(
        offered(&db, &candidate.files.content_hash()).await,
        vec![("https://img/partner-stated".to_owned(), CoverStanding::Stated)],
    );
    let (db, _tmp, candidate) = picked(
        (vec![discogs_image("https://img/own-unstated", CoverStanding::Unstated)], Vec::new()),
        (Vec::new(), Vec::new()),
    )
    .await;
    let urls: Vec<_> = offered(&db, &candidate.files.content_hash()).await;
    assert_eq!(urls, vec![("https://img/own-unstated".to_owned(), CoverStanding::Unstated)]);
}

#[tokio::test]
async fn offered_covers_statement_lists_one_cover_per_image_stated_if_either_is() {
    let (db, _tmp, candidate) = picked(
        (
            vec![discogs_image("https://img/shared", CoverStanding::Unstated)],
            vec![discogs_image("https://img/album", CoverStanding::Stated)],
        ),
        (
            vec![
                discogs_image("https://img/shared", CoverStanding::Stated),
                discogs_image("https://img/partner", CoverStanding::Stated),
            ],
            vec![discogs_image("https://img/album", CoverStanding::Stated)],
        ),
    )
    .await;
    assert_eq!(
        offered(&db, &candidate.files.content_hash()).await,
        vec![
            ("https://img/shared".to_owned(), CoverStanding::Stated),
            ("https://img/partner".to_owned(), CoverStanding::Stated),
            ("https://img/album".to_owned(), CoverStanding::Stated),
        ],
        "each claimed release's own images come first, the albums' after, one per URL"
    );
}

#[tokio::test]
async fn offered_covers_statement_offers_none_unless_the_draft_is_the_pressing() {
    let (db, _tmp, root) = watched_root().await;
    let candidate = linked(
        &db,
        &root,
        pressing(LookupRef::new(LookupCatalog::MusicBrainz, "mb-release"), Vec::new()),
        &[with_covers(
            musicbrainz_primary(),
            vec![discogs_image("https://img/own", CoverStanding::Stated)],
            Vec::new(),
        )],
    )
    .await;
    assert_eq!(offered(&db, &candidate.files.content_hash()).await, Vec::new());
    read_from(&db, &candidate, &LookupRef::new(LookupCatalog::MusicBrainz, "mb-release")).await;
    assert_eq!(offered(&db, &candidate.files.content_hash()).await.len(), 1);
}

#[tokio::test]
async fn offered_covers_statement_matches_pick_covers() {
    let (db, _tmp, candidate) = picked(
        (
            vec![discogs_image("https://img/own", CoverStanding::Unstated)],
            vec![discogs_image("https://img/album", CoverStanding::Unstated)],
        ),
        (Vec::new(), Vec::new()),
    )
    .await;
    // A statement kept for the Discogs partner's master joins a MusicBrainz
    // release group to it, whose front image the picker offers too.
    crate::tables::ImportTables::new(db.clone())
        .replace_group_album_links(vec![(
            "mb-joined-group".to_owned(),
            vec![bae_catalogs::records::MetadataRef::new(Catalog::Discogs, "909090".to_owned())],
        )])
        .await
        .unwrap();
    let tables = crate::tables::ImportTables::new(db.clone());
    let primary = tables
        .load_source_release(&LookupRef::new(LookupCatalog::MusicBrainz, "mb-release"))
        .await
        .unwrap()
        .unwrap();
    let partner = tables
        .load_source_release(&LookupRef::new(LookupCatalog::Discogs, "2222"))
        .await
        .unwrap()
        .unwrap();
    let expected = bae_catalogs::source_release::pick_covers(&primary, &[partner]);
    assert!(
        expected.iter().any(|cover| cover.image.url.contains("mb-joined-group")),
        "the joined release group's image is among the covers: {expected:?}"
    );
    let content_hash = candidate.files.content_hash();
    let read = db.read(move |sql| offered_covers_on(&sql, &content_hash)).await.unwrap();
    assert_eq!(read, expected);
}
