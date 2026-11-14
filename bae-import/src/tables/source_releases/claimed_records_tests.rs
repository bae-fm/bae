//! The statement that merges the records of the releases a link claims says
//! what `claimed_records` says over the same stored releases.

use super::*;
use crate::tables::import_list::tests::*;
use crate::tables::tests::*;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::payloads::ReleasePayloads;
use bae_catalogs::records::LookupRef;
use bae_catalogs::records::MetadataRef;
use bae_catalogs::source_release::SourceRelease;
use bae_catalogs::test_documents::musicbrainz_release_with_relations;
use bae_store::Store;

/// A MusicBrainz release in `mb-group` that says its Discogs release is 1111.
pub(crate) fn musicbrainz_primary() -> SourceRelease {
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
pub(crate) fn discogs_release(id: u64, master: u64) -> SourceRelease {
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
pub(crate) async fn linked(
    db: &Store,
    root: &std::path::Path,
    link: crate::ReleaseLink,
    releases: &[SourceRelease],
) -> crate::folder_scanner::FolderCandidate {
    let candidate = scanned(db, root, "Album").await;
    save_verdict(db, &candidate, "mb-verdict").await;
    crate::tables::ImportTables::new(db.clone())
        .replace_link(&candidate.files.content_hash(), Some(&link), releases)
        .await
        .unwrap();
    candidate
}

async fn statement_records(
    db: &Store,
    content_hash: &str,
) -> Vec<(Catalog, String, Option<String>)> {
    let content_hash = content_hash.to_owned();
    db.read(move |sql| claimed_records_on(&sql, &content_hash))
        .await
        .unwrap()
        .iter()
        .map(|record| {
            (record.catalog(), record.key().to_owned(), record.album_ref().map(|album| album.key))
        })
        .collect()
}

pub(crate) fn pressing(record: LookupRef, partners: Vec<LookupRef>) -> crate::ReleaseLink {
    crate::ReleaseLink::Pressing(bae_identify::PressingLink { record, partners })
}

#[tokio::test]
async fn claimed_records_statement_prefers_each_release_own_record() {
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
        statement_records(&db, &hash).await,
        vec![
            (Catalog::MusicBrainz, "mb-release".to_owned(), Some("mb-group".to_owned())),
            (Catalog::Discogs, "2222".to_owned(), Some("909090".to_owned())),
        ],
        "the picked Discogs release outranks the one the MusicBrainz release cross-links"
    );
}

#[tokio::test]
async fn claimed_records_statement_puts_the_applied_catalog_first() {
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
        statement_records(&db, &hash).await.into_iter().map(|record| record.0).collect();
    assert_eq!(catalogs, vec![Catalog::Discogs, Catalog::MusicBrainz]);
}

#[tokio::test]
async fn claimed_records_statement_lists_an_album_link_records() {
    let (db, _tmp, root) = watched_root().await;
    let link = bae_identify::AlbumLink::new([
        MetadataRef::new(Catalog::Discogs, "909090".to_owned()),
        MetadataRef::new(Catalog::MusicBrainz, "mb-group".to_owned()),
    ])
    .unwrap();
    let hash = linked(&db, &root, crate::ReleaseLink::Album(link), &[]).await.files.content_hash();
    assert_eq!(
        statement_records(&db, &hash).await,
        vec![
            (Catalog::MusicBrainz, "mb-group".to_owned(), Some("mb-group".to_owned())),
            (Catalog::Discogs, "909090".to_owned(), Some("909090".to_owned())),
        ]
    );
}

#[tokio::test]
async fn claimed_records_statement_matches_claimed_records() {
    let (db, _tmp, root) = watched_root().await;
    // A statement kept for the primary's release group joins AllMusic to it,
    // a catalog none of its own records reaches.
    crate::tables::ImportTables::new(db.clone())
        .replace_group_album_links(vec![(
            "mb-group".to_owned(),
            vec![MetadataRef::new(Catalog::AllMusic, "mw0000424242".to_owned())],
        )])
        .await
        .unwrap();
    let primary = LookupRef::new(LookupCatalog::MusicBrainz, "mb-release");
    let partner = LookupRef::new(LookupCatalog::Discogs, "2222");
    let hash = linked(
        &db,
        &root,
        pressing(primary.clone(), vec![partner.clone()]),
        &[musicbrainz_primary(), discogs_release(2222, 909090)],
    )
    .await
    .files
    .content_hash();
    let tables = crate::tables::ImportTables::new(db.clone());
    let claimed = [
        tables.load_source_release(&primary).await.unwrap().unwrap().records(),
        tables.load_source_release(&partner).await.unwrap().unwrap().records(),
    ];
    let expected: Vec<_> = bae_catalogs::source_release::claimed_records(&claimed)
        .iter()
        .map(|record| {
            (record.catalog(), record.key().to_owned(), record.album_ref().map(|a| a.key))
        })
        .collect();
    assert!(
        expected.iter().any(|record| record.0 == Catalog::AllMusic),
        "the joined album is among the records: {expected:?}"
    );
    assert_eq!(statement_records(&db, &hash).await, expected);
}
