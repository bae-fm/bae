//! What a candidate's link claims is stored with the link, and stored again
//! by each write that changes what it comes from.

use super::*;
use crate::tables::import_list::tests::*;
use crate::tables::tests::*;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::payloads::ReleasePayloads;
use bae_catalogs::records::{LookupRef, PayloadSource};
use bae_catalogs::remote_image::{CoverLabel, CoverStanding, RemoteCover, RemoteImageSet};
use bae_catalogs::source_release::{UnfetchedDocument, UnfetchedReason};
use bae_catalogs::test_documents::musicbrainz_release_with_relations;
use bae_store::Store;

/// A MusicBrainz release in `mb-group` that says its Discogs release is 1111.
fn musicbrainz_primary() -> SourceRelease {
    ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::MusicBrainz, "mb-release"),
        musicbrainz_release_with_relations(
            "mb-release",
            "mb-group",
            &["https://www.discogs.com/release/1111"],
        )
        .to_string(),
        Vec::new(),
    )
    .extract()
    .expect("the MusicBrainz release extracts")
}

/// A Discogs release filed under `master`.
fn discogs_release(id: u64, master: u64) -> SourceRelease {
    ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, id.to_string()),
        serde_json::json!({ "id": id, "title": "Album Title", "master_id": master }).to_string(),
        Vec::new(),
    )
    .extract()
    .expect("the Discogs release extracts")
}

/// A candidate with a stored state, linked as `link` says, with `releases`
/// stored.
async fn linked(
    db: &Store,
    root: &std::path::Path,
    link: ReleaseLink,
    releases: &[SourceRelease],
) -> crate::folder_scanner::FolderCandidate {
    let candidate = scanned(db, root, "Album").await;
    save_verdict(db, &candidate, "mb-verdict").await;
    ImportTables::new(db.clone())
        .replace_link(&candidate.files.content_hash(), Some(&link), releases)
        .await
        .unwrap();
    candidate
}

fn pressing(record: LookupRef, partners: Vec<LookupRef>) -> ReleaseLink {
    ReleaseLink::Pressing(bae_identify::PressingLink { record, partners })
}

/// The stored records the link of `content_hash` claims.
async fn stored_records(db: &Store, content_hash: &str) -> Vec<(Catalog, String, Option<String>)> {
    let content_hash = content_hash.to_owned();
    let json: String = db
        .read(move |sql| {
            Ok(sql.query_row(
                &format!("SELECT ({})", claimed_records_sql("?")),
                [content_hash],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    claimed_records_of_json(&json)
        .unwrap()
        .iter()
        .map(|record| {
            (record.catalog(), record.key().to_owned(), record.album_ref().map(|album| album.key))
        })
        .collect()
}

#[tokio::test]
async fn a_claimed_release_own_record_outranks_what_another_says() {
    let (db, _tmp, root) = watched_root().await;
    let hash = linked(
        &db,
        &root,
        pressing(
            LookupRef::new(LookupCatalog::MusicBrainz, "mb-release"),
            vec![LookupRef::new(LookupCatalog::Discogs, "2222")],
        ),
        &[musicbrainz_primary(), discogs_release(2222, 909090)],
    )
    .await
    .files
    .content_hash();
    assert_eq!(
        stored_records(&db, &hash).await,
        vec![
            (Catalog::MusicBrainz, "mb-release".to_owned(), Some("mb-group".to_owned())),
            (Catalog::Discogs, "2222".to_owned(), Some("909090".to_owned())),
        ],
        "the picked Discogs release outranks the one the MusicBrainz release cross-links"
    );
}

#[tokio::test]
async fn the_applied_catalog_record_comes_first() {
    let (db, _tmp, root) = watched_root().await;
    let hash = linked(
        &db,
        &root,
        pressing(
            LookupRef::new(LookupCatalog::Discogs, "2222"),
            vec![LookupRef::new(LookupCatalog::MusicBrainz, "mb-release")],
        ),
        &[discogs_release(2222, 909090), musicbrainz_primary()],
    )
    .await
    .files
    .content_hash();
    let catalogs: Vec<Catalog> =
        stored_records(&db, &hash).await.into_iter().map(|record| record.0).collect();
    assert_eq!(catalogs, vec![Catalog::Discogs, Catalog::MusicBrainz]);
}

#[tokio::test]
async fn an_album_link_claims_its_albums() {
    let (db, _tmp, root) = watched_root().await;
    let link = bae_identify::AlbumLink::new([
        MetadataRef::new(Catalog::MusicBrainz, "mb-group".to_owned()),
        MetadataRef::new(Catalog::Discogs, "909090".to_owned()),
    ])
    .unwrap();
    let hash = linked(&db, &root, ReleaseLink::Album(link), &[]).await.files.content_hash();
    assert_eq!(
        stored_records(&db, &hash).await,
        vec![
            (Catalog::MusicBrainz, "mb-group".to_owned(), Some("mb-group".to_owned())),
            (Catalog::Discogs, "909090".to_owned(), Some("909090".to_owned())),
        ]
    );
}

/// Keeping an album for the primary's release group after the link is stored
/// stores the link's records again, with the album joined on the catalog none
/// of its own records reaches.
#[tokio::test]
async fn an_album_kept_for_a_claimed_release_album_joins_its_records() {
    let (db, _tmp, root) = watched_root().await;
    let hash = linked(
        &db,
        &root,
        pressing(
            LookupRef::new(LookupCatalog::MusicBrainz, "mb-release"),
            vec![LookupRef::new(LookupCatalog::Discogs, "2222")],
        ),
        &[musicbrainz_primary(), discogs_release(2222, 909090)],
    )
    .await
    .files
    .content_hash();
    let allmusic = (Catalog::AllMusic, "mw0000424242".to_owned(), Some("mw0000424242".to_owned()));
    assert!(!stored_records(&db, &hash).await.contains(&allmusic));
    let tables = ImportTables::new(db.clone());
    tables
        .replace_group_album_links(vec![(
            "mb-group".to_owned(),
            vec![MetadataRef::new(Catalog::AllMusic, "mw0000424242".to_owned())],
        )])
        .await
        .unwrap();
    assert!(stored_records(&db, &hash).await.contains(&allmusic));
    tables.replace_group_album_links(vec![("mb-group".to_owned(), Vec::new())]).await.unwrap();
    assert!(
        !stored_records(&db, &hash).await.contains(&allmusic),
        "the album goes once no statement keeps it"
    );
}

/// A claimed release stored again from a fuller read stores the link's
/// records again from it.
#[tokio::test]
async fn a_claimed_release_stored_again_stores_the_records_again() {
    let (db, _tmp, root) = watched_root().await;
    let link = pressing(LookupRef::new(LookupCatalog::Discogs, "2222"), Vec::new());
    let mut partial = discogs_release(2222, 909090);
    partial.unfetched.push(UnfetchedDocument {
        document: PayloadSource::MusicBrainzDiscogsXref,
        key: "2222".to_owned(),
        reason: UnfetchedReason::Failed,
    });
    let hash = linked(&db, &root, link.clone(), &[partial]).await.files.content_hash();
    let cross_linked =
        (Catalog::MusicBrainz, "mb-xref".to_owned(), Some("mb-xref-group".to_owned()));
    assert!(!stored_records(&db, &hash).await.contains(&cross_linked));
    let mut fuller = discogs_release(2222, 909090);
    fuller.other_records.push(ReleaseRecord::new(
        &MetadataRef::new(Catalog::MusicBrainz, "mb-xref".to_owned()),
        Some("mb-xref-group".to_owned()),
    ));
    ImportTables::new(db.clone()).replace_link(&hash, Some(&link), &[fuller]).await.unwrap();
    assert!(stored_records(&db, &hash).await.contains(&cross_linked));
}

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

/// Reads the candidate's draft from the pressing that `record` names.
async fn read_from(
    db: &Store,
    candidate: &crate::folder_scanner::FolderCandidate,
    record: &LookupRef,
) {
    let tables = ImportTables::new(db.clone());
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

/// The covers the picker of `content_hash` lists as its pressing's.
async fn offered_covers(db: &Store, content_hash: &str) -> Vec<RemoteCover> {
    let content_hash = content_hash.to_owned();
    let json: Vec<String> = db
        .read(move |sql| {
            Ok(sql.query(
                &format!(
                    "SELECT cover FROM ({}) ORDER BY position",
                    offered_covers_sql("(SELECT ? AS content_hash)")
                ),
                [content_hash],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    json.iter().map(|cover| serde_json::from_str(cover).unwrap()).collect()
}

async fn offered(db: &Store, content_hash: &str) -> Vec<(String, CoverStanding)> {
    offered_covers(db, content_hash)
        .await
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
async fn a_pressing_offers_stated_covers_only_when_any() {
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
    assert_eq!(
        offered(&db, &candidate.files.content_hash()).await,
        vec![("https://img/own-unstated".to_owned(), CoverStanding::Unstated)]
    );
}

#[tokio::test]
async fn a_pressing_offers_one_cover_per_image_stated_if_either_is() {
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
async fn a_pressing_offers_no_cover_unless_the_draft_is_read_from_it() {
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

/// Keeping a MusicBrainz release group for the Discogs partner's master after
/// the link is stored offers that group's front image after the partner's
/// own album images.
#[tokio::test]
async fn a_release_group_kept_for_a_claimed_release_offers_its_front_image() {
    let (db, _tmp, candidate) = picked(
        (
            vec![discogs_image("https://img/own", CoverStanding::Unstated)],
            vec![discogs_image("https://img/album", CoverStanding::Unstated)],
        ),
        (Vec::new(), Vec::new()),
    )
    .await;
    ImportTables::new(db.clone())
        .replace_group_album_links(vec![(
            "mb-joined-group".to_owned(),
            vec![MetadataRef::new(Catalog::Discogs, "909090".to_owned())],
        )])
        .await
        .unwrap();
    let urls: Vec<String> = offered_covers(&db, &candidate.files.content_hash())
        .await
        .into_iter()
        .map(|cover| cover.image.url)
        .collect();
    assert_eq!(urls.len(), 3, "{urls:?}");
    assert_eq!(urls[..2], ["https://img/own".to_owned(), "https://img/album".to_owned()]);
    assert!(urls[2].contains("mb-joined-group"), "{urls:?}");
}
