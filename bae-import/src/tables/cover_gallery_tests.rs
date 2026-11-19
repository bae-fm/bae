//! The covers a library release's picker offers: what one read shows under
//! each source, how the query counts each section and the total, and how a
//! fetch replaces what the last one stored.

use super::*;
use bae_catalogs::remote_image::{CoverStanding, RemoteImageSet};
use bae_records::test_fixtures::live_query::{live_db, RELEASE_ID};
use bae_store::Store;
use std::time::Duration;

fn offered(url: &str, source: Catalog) -> RemoteCover {
    RemoteCover {
        image: RemoteImageSet::original(url.to_string()),
        label: CoverLabel::Lead { release_group: false },
        source,
        standing: CoverStanding::Stated,
    }
}

async fn image_file(
    db: &Store,
    dir: &bae_util::test_files::TestFolder,
    id: &str,
    name: &str,
    kind: ContentType,
) {
    bae_records::test_fixtures::aggregate_ordering::insert_file(
        db,
        dir,
        bae_records::NewReleaseFile::new(
            RELEASE_ID,
            name,
            kind,
            bae_records::ReleaseFileRole::Artwork,
            id.to_string(),
            time::UtcDateTime::now(),
        ),
        1,
    )
    .await;
}

use bae_util::content_type::ContentType;

/// The release has a saved cover, three catalog covers and files of three
/// kinds: its two images and a PDF the picker never offers.
async fn seeded(
) -> (ImportTables, Store, bae_util::test_files::TestFolder, bae_util::test_files::TestFolder) {
    let (db, temp) = live_db(crate::TEST_SCHEMA).await;
    let files = bae_util::test_files::TestFolder::new();
    bae_records::test_fixtures::write_cover(
        &bae_records::test_fixtures::test_records(&db),
        RELEASE_ID,
        "saved cover",
    )
    .await;
    image_file(
        &db,
        &files,
        &bae_util::test_ids::test_uuid("file-back"),
        "Back.jpg",
        ContentType::Jpeg,
    )
    .await;
    image_file(
        &db,
        &files,
        &bae_util::test_ids::test_uuid("file-art"),
        "art 2.png",
        ContentType::Png,
    )
    .await;
    image_file(
        &db,
        &files,
        &bae_util::test_ids::test_uuid("file-booklet"),
        "booklet.pdf",
        ContentType::Pdf,
    )
    .await;
    let tables = ImportTables::new(db.clone());
    tables
        .replace_release_covers(
            RELEASE_ID,
            Some(vec![
                offered("https://caa.example/front", Catalog::MusicBrainz),
                offered("https://discogs.example/1", Catalog::Discogs),
                offered("https://discogs.example/2", Catalog::Discogs),
            ]),
        )
        .await
        .unwrap();
    (tables, db, temp, files)
}

type Gallery = coven::ReconfigurableLiveQuery<
    Option<CoverGalleryRequest>,
    Option<CoverGallerySnapshot>,
    bae_store::DbError,
>;

fn request(filter: CoverFilter) -> CoverGalleryRequest {
    CoverGalleryRequest {
        owner: CoverGalleryOwner::Release(RELEASE_ID.to_string()),
        filter,
        focus: None,
    }
}

/// The first gallery a read gives for `asked` that `accept` takes, failing
/// when none arrives.
async fn gallery_until(
    gallery: &mut Gallery,
    asked: &CoverGalleryRequest,
    accept: impl Fn(&CoverGallery) -> bool,
) -> CoverGallery {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let event = gallery.next_inline().await.expect("the read runs");
            if event.request.as_ref() != Some(asked) {
                continue;
            }
            let snapshot = event.result.unwrap().expect("the live query answers the request");
            assert_eq!(&snapshot.request, asked, "the snapshot names the request it answers");
            if accept(&snapshot.gallery) {
                break snapshot.gallery;
            }
        }
    })
    .await
    .expect("the read delivered the gallery asked for")
}

/// The gallery `filter` shows now.
async fn read(tables: &ImportTables, filter: CoverFilter) -> CoverGallery {
    let asked = request(filter);
    let mut gallery = tables.subscribe_cover_gallery(Some(asked.clone()));
    gallery_until(&mut gallery, &asked, |_| true).await
}

/// A section's count and its number of covers, `None` while the filter hides
/// it.
type SectionCount = Option<(u32, usize)>;

/// The catalogs' section's [`SectionCount`] and the image files' section's.
fn counts(gallery: &CoverGallery) -> (SectionCount, SectionCount) {
    let count = |section: &Option<CoverSection>| {
        section.as_ref().map(|section| (section.count, section.items.len()))
    };
    (count(&gallery.remote), count(&gallery.files))
}

/// The catalogs' section, which the filter shows.
fn remote(gallery: &CoverGallery) -> &CoverSection {
    gallery.remote.as_ref().expect("the filter shows the catalogs' covers")
}

/// The image files' section, which the filter shows.
fn files(gallery: &CoverGallery) -> &CoverSection {
    gallery.files.as_ref().expect("the filter shows the image files")
}

fn names(section: &CoverSection) -> Vec<String> {
    section
        .items
        .iter()
        .map(|item| match &item.label {
            GalleryLabel::File { name, .. } => name.clone(),
            GalleryLabel::Remote { .. } => match &item.image {
                CoverImageSource::Remote { image } => image.url.clone(),
                other => panic!("a remote cover draws from {other:?}"),
            },
            GalleryLabel::Current => "current".to_string(),
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn all_sources_show_the_saved_cover_the_offered_covers_and_the_image_files() {
    let (tables, _db, _temp, _files) = seeded().await;
    let gallery = read(&tables, CoverFilter::All).await;

    let current = gallery.current.clone().expect("the saved cover");
    assert_eq!(current.label, GalleryLabel::Current);
    assert!(matches!(
        current.image,
        CoverImageSource::ReleaseImage { source: bae_records::GallerySource::Cover(_), .. }
    ));
    assert_eq!(gallery.fetch, CoverFetch::Linked);
    assert_eq!(counts(&gallery), (Some((3, 3)), Some((2, 2))));
    assert_eq!(
        names(remote(&gallery)),
        ["https://caa.example/front", "https://discogs.example/1", "https://discogs.example/2"]
    );
    assert_eq!(names(files(&gallery)), ["art 2.png", "Back.jpg"]);
    assert_eq!(gallery.total, 6);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_catalog_filter_shows_only_that_catalogs_covers() {
    let (tables, _db, _temp, _files) = seeded().await;
    let gallery = read(&tables, CoverFilter::Catalog(Catalog::Discogs)).await;

    assert!(gallery.current.is_none());
    assert_eq!(counts(&gallery), (Some((2, 2)), None));
    assert_eq!(gallery.total, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_release_files_filter_shows_only_the_image_files() {
    let (tables, _db, _temp, _files) = seeded().await;
    let gallery = read(&tables, CoverFilter::ReleaseFiles).await;

    assert!(gallery.current.is_none());
    assert_eq!(counts(&gallery), (None, Some((2, 2))));
    assert_eq!(gallery.fetch, CoverFetch::Linked);
    assert_eq!(gallery.total, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_fetch_replaces_the_covers_of_the_last() {
    let (tables, _db, _temp, _files) = seeded().await;
    tables
        .replace_release_covers(
            RELEASE_ID,
            Some(vec![offered("https://discogs.example/3", Catalog::Discogs)]),
        )
        .await
        .unwrap();
    let gallery = read(&tables, CoverFilter::All).await;
    assert_eq!(names(remote(&gallery)), ["https://discogs.example/3"]);
    assert_eq!(remote(&gallery).count, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unlinked_release_stores_that_it_offers_nothing() {
    let (tables, _db, _temp, _files) = seeded().await;
    tables.replace_release_covers(RELEASE_ID, None).await.unwrap();
    let gallery = read(&tables, CoverFilter::All).await;
    assert_eq!(gallery.fetch, CoverFetch::Unlinked);
    assert_eq!(remote(&gallery).count, 0);
    assert_eq!(gallery.total, 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_release_nothing_fetched_for_has_no_answer() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let gallery = read(&ImportTables::new(db), CoverFilter::All).await;
    assert_eq!(gallery.fetch, CoverFetch::NotFetched);
    assert_eq!(counts(&gallery), (Some((0, 0)), Some((0, 0))));
    assert_eq!(gallery.total, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_the_release_deletes_what_was_fetched_for_it() {
    let (tables, db, _temp, _files) = seeded().await;
    bae_records::test_fixtures::delete_release(
        &bae_records::test_fixtures::test_records(&db),
        RELEASE_ID,
    )
    .await;
    assert_eq!(
        tables.count_rows_for_test("release_cover_fetch", "release_id", RELEASE_ID).await.unwrap(),
        0
    );
    assert_eq!(
        tables.count_rows_for_test("release_cover_entry", "release_id", RELEASE_ID).await.unwrap(),
        0
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fetch_reaches_an_open_picker() {
    let (tables, _db, _temp, _files) = seeded().await;
    let asked = request(CoverFilter::All);
    let mut gallery = tables.subscribe_cover_gallery(Some(asked.clone()));
    gallery_until(&mut gallery, &asked, |gallery| remote(gallery).count == 3).await;

    tables
        .replace_release_covers(
            RELEASE_ID,
            Some(vec![offered("https://discogs.example/9", Catalog::Discogs)]),
        )
        .await
        .unwrap();

    let after = gallery_until(&mut gallery, &asked, |gallery| remote(gallery).count == 1).await;
    assert_eq!(names(remote(&after)), ["https://discogs.example/9"]);
    assert_eq!(after.total, 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_image_file_reaches_an_open_picker() {
    let (tables, db, _temp, file_dir) = seeded().await;
    let asked = request(CoverFilter::ReleaseFiles);
    let mut gallery = tables.subscribe_cover_gallery(Some(asked.clone()));
    gallery_until(&mut gallery, &asked, |gallery| gallery.total == 2).await;

    image_file(
        &db,
        &file_dir,
        &bae_util::test_ids::test_uuid("file-new"),
        "Inlay.jpg",
        ContentType::Jpeg,
    )
    .await;

    let after = gallery_until(&mut gallery, &asked, |gallery| gallery.total == 3).await;
    assert_eq!(names(files(&after)), ["art 2.png", "Back.jpg", "Inlay.jpg"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn another_source_is_a_new_request_on_the_same_read() {
    let (tables, _db, _temp, _files) = seeded().await;
    let all = request(CoverFilter::All);
    let mut gallery = tables.subscribe_cover_gallery(Some(all.clone()));
    gallery_until(&mut gallery, &all, |gallery| gallery.total == 6).await;

    let discogs = request(CoverFilter::Catalog(Catalog::Discogs));
    gallery.requests().set(Some(discogs.clone())).unwrap();

    let after = gallery_until(&mut gallery, &discogs, |_| true).await;
    assert_eq!(counts(&after), (Some((2, 2)), None));
}

/// The gallery `filter` shows now, focused on the cover `focus` names.
async fn read_focused(
    tables: &ImportTables,
    filter: CoverFilter,
    focus: Option<CoverKey>,
) -> CoverGallery {
    let asked = CoverGalleryRequest { focus, ..request(filter) };
    let mut gallery = tables.subscribe_cover_gallery(Some(asked.clone()));
    gallery_until(&mut gallery, &asked, |_| true).await
}

/// Where the gallery's focus is: its cover's key, its place, and the covers
/// before and after it.
fn focus_of(gallery: &CoverGallery) -> (CoverKey, u32, Option<(CoverKey, CoverKey)>) {
    let focus = gallery.focus.clone().expect("a gallery with covers focuses one");
    (
        focus.cover.key,
        focus.position,
        focus.neighbors.map(|neighbors| (neighbors.previous, neighbors.next)),
    )
}

fn remote_key(url: &str) -> CoverKey {
    CoverKey::Remote(url.to_string())
}

fn file_key(name: &str) -> CoverKey {
    CoverKey::File(bae_util::test_ids::test_uuid(name))
}

/// With nothing asked, a release's picker focuses its saved cover; the
/// covers before and after it wrap around the ends of the walk.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_focuses_its_saved_cover_unless_asked() {
    let (tables, _db, _temp, _files) = seeded().await;
    let gallery = read_focused(&tables, CoverFilter::All, None).await;
    assert_eq!(
        focus_of(&gallery),
        (
            CoverKey::Saved,
            0,
            Some((file_key("file-back"), remote_key("https://caa.example/front")))
        )
    );
}

/// The picker focuses the cover asked for at its place in the walk: after
/// the saved cover and the catalog covers before it.
#[tokio::test(flavor = "multi_thread")]
async fn the_cover_asked_for_is_focused_where_it_sits() {
    let (tables, _db, _temp, _files) = seeded().await;
    let gallery =
        read_focused(&tables, CoverFilter::All, Some(remote_key("https://discogs.example/1")))
            .await;
    assert_eq!(
        focus_of(&gallery),
        (
            remote_key("https://discogs.example/1"),
            2,
            Some((
                remote_key("https://caa.example/front"),
                remote_key("https://discogs.example/2")
            ))
        )
    );
    let file = read_focused(&tables, CoverFilter::All, Some(file_key("file-art"))).await;
    assert_eq!(focus_of(&file).0, file_key("file-art"));
    assert_eq!(focus_of(&file).1, 4);
}

/// The picker never focuses a cover the source hides. It focuses its first
/// cover instead, and a lone cover has none before or after it.
#[tokio::test(flavor = "multi_thread")]
async fn a_hidden_cover_leaves_the_focus_on_the_first() {
    let (tables, _db, _temp, _files) = seeded().await;
    let discogs = read_focused(
        &tables,
        CoverFilter::Catalog(Catalog::Discogs),
        Some(remote_key("https://caa.example/front")),
    )
    .await;
    assert_eq!(
        focus_of(&discogs),
        (
            remote_key("https://discogs.example/1"),
            0,
            Some((
                remote_key("https://discogs.example/2"),
                remote_key("https://discogs.example/2")
            ))
        )
    );
    let archive = read_focused(&tables, CoverFilter::Catalog(Catalog::MusicBrainz), None).await;
    assert_eq!(focus_of(&archive), (remote_key("https://caa.example/front"), 0, None));
}

/// A picker that shows no cover focuses none.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_picker_focuses_nothing() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let gallery =
        read_focused(&ImportTables::new(db), CoverFilter::All, Some(CoverKey::Saved)).await;
    assert_eq!(gallery.focus, None);
}
