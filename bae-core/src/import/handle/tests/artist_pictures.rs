//! The pictures of the Discogs artists a release credits are fetched by the
//! import that creates those artists, before it writes anything, and never by
//! the pick that reads the release into the draft.

use super::*;

/// A candidate picked from a Discogs release crediting an album artist and a
/// track artist, whose artist documents Discogs has not been asked for: no
/// answer is seeded for them, so a lookup of either would fail on the test
/// transport.
struct DiscogsPick {
    handle: ImportServiceHandle,
    _tmp: TempDir,
    key: String,
    images: bae_test_support::RemoteImageHost,
    album_artist_id: String,
    track_artist_id: String,
    /// Where the track artist's picture is served on the image host.
    picture_path: String,
}

impl DiscogsPick {
    /// Discogs' artist documents: none for the album artist, and for the
    /// track artist one whose primary image is `picture_url`.
    fn seed_artists(&self, picture_url: String) {
        let discogs = self.handle.library_manager.providers().discogs();
        discogs.seed_artist_image_response(&self.album_artist_id, None);
        discogs.seed_artist_image_response(&self.track_artist_id, Some(picture_url));
    }

    async fn import(&self) -> Result<(String, String), String> {
        let mut events = self.handle.every_event();
        let import_id = self.handle.start_import(&self.key).await.unwrap().expect_queued();
        await_import_outcome(&mut events, &import_id).await
    }

    /// The library artist the release's first track credits as the track
    /// artist.
    async fn track_artist(&self, release_id: &str) -> crate::db::DbArtist {
        let first_track = self
            .handle
            .library_manager
            .get_tracks_for_release(release_id)
            .await
            .unwrap()
            .into_iter()
            .next()
            .expect("the imported release has its first track");
        self.handle
            .library_manager
            .get_artists_for_track(&first_track.id)
            .await
            .unwrap()
            .into_iter()
            .find(|artist| artist.artist.discogs_artist_id.as_deref() == Some(&self.track_artist_id))
            .expect("the imported track credits the Discogs track artist")
            .artist
    }

    /// The bytes of the picture the library holds for `artist_id`, if any.
    async fn picture_of(&self, artist_id: &str) -> Option<Vec<u8>> {
        let result = self
            .handle
            .library_manager
            .search_artists(
                &crate::library::LibrarySearchQuery::parse("Track Artist")
                    .expect("the artist query is not blank"),
            )
            .await
            .unwrap()
            .into_iter()
            .find(|result| result.artist.id == artist_id)
            .expect("the track artist is searchable");
        let image = result.image?;
        self.handle
            .library_manager
            .read_image_bytes(&image)
            .await
            .unwrap()
    }
}

/// A picture larger than bae stores, as a PNG: 1200 by 800 pixels.
fn large_picture() -> Vec<u8> {
    let image = image::RgbImage::from_pixel(1200, 800, image::Rgb([90, 30, 160]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("the picture encodes");
    bytes.into_inner()
}

/// That `stored` is [`large_picture`] as bae stores it: a JPEG downscaled to
/// 600 pixels on its longer side.
fn assert_stored_large_picture(stored: Option<Vec<u8>>) {
    let stored = stored.expect("the imported artist has a picture");
    assert_eq!(
        image::guess_format(&stored).expect("the stored picture has a format"),
        image::ImageFormat::Jpeg,
        "the stored picture is a JPEG"
    );
    let decoded = image::load_from_memory(&stored).expect("the stored picture decodes");
    assert_eq!(
        (decoded.width(), decoded.height()),
        (600, 400),
        "the stored picture is downscaled to 600 pixels on its longer side"
    );
}

async fn picked_discogs_release() -> DiscogsPick {
    let images = bae_test_support::RemoteImageHost::start().await;
    let (handle, tmp, key, _hash) = pane_fixture_with(
        crate::util::http::Http::for_test()
            .serve(bae_test_support::TEST_IMAGE_HOST, images.origin()),
    )
    .await;
    handle
        .library_manager
        .set_discogs_key("test-discogs-token", crate::config::DiscogsValidation::Valid)
        .await
        .unwrap();

    let numeric_id = || (uuid::Uuid::new_v4().as_u128() % u128::from(u64::MAX)) as u64;
    let album_artist_id = numeric_id();
    let track_artist_id = numeric_id();
    let source_release_id = numeric_id().to_string();
    let raw_release = serde_json::json!({
        "id": source_release_id.parse::<u64>().unwrap(),
        "title": "Album Title",
        "year": 2020,
        "formats": [{ "name": "CD" }],
        "artists": [{ "id": album_artist_id, "name": "Album Artist" }],
        "tracklist": [
            {
                "position": "1",
                "title": "Track One",
                "duration": "0:01",
                "artists": [{ "id": track_artist_id, "name": "Track Artist" }],
            },
            { "position": "2", "title": "Track Two", "duration": "0:01" },
        ],
    })
    .to_string();
    crate::discogs::client::parse_discogs_release_json(&raw_release)
        .expect("the rendered Discogs release parses");
    handle
        .library_manager
        .providers()
        .discogs()
        .seed_release_cache(&source_release_id, raw_release);
    handle
        .library_manager
        .providers()
        .musicbrainz()
        .seed_discogs_page_lookup(&[(crate::musicbrainz::DiscogsPage::Release(source_release_id.clone()), None)]);

    handle
        .select_candidate_release(
            key.clone(),
            crate::import::PressingLink {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::Discogs,
                    source_release_id,
                ),
                partners: vec![],
            },
        )
        .await
        .expect("the pick asks Discogs for no artist");

    DiscogsPick {
        handle,
        _tmp: tmp,
        key,
        images,
        album_artist_id: album_artist_id.to_string(),
        track_artist_id: track_artist_id.to_string(),
        picture_path: format!("/discogs-artist-{}.png", uuid::Uuid::new_v4()),
    }
}

/// The pick asks for no picture; the import fetches the new track artist's
/// once and stores it as bae stores every image, a JPEG at most 600 pixels
/// on its longer side, whatever Discogs served.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_fetches_no_artist_picture_and_the_import_stores_each_as_a_600_pixel_jpeg() {
    let pick = picked_discogs_release().await;
    let picture_url = pick.images.serve_image(&pick.picture_path, large_picture());
    assert_eq!(
        pick.images.requests(&pick.picture_path),
        0,
        "the pick downloads no artist picture"
    );

    pick.seed_artists(picture_url);
    let (release_id, _album_id) = pick
        .import()
        .await
        .unwrap_or_else(|error| panic!("import failed: {error}"));

    assert_eq!(
        pick.images.requests(&pick.picture_path),
        1,
        "the import downloads the track artist's picture once"
    );
    let artist = pick.track_artist(&release_id).await;
    assert_stored_large_picture(pick.picture_of(&artist.id).await);
    shut_down(pick.handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_artist_the_library_already_has_is_not_fetched_again() {
    let pick = picked_discogs_release().await;
    let existing = make_artist("Track Artist", Some(&pick.track_artist_id), None);
    pick.handle
        .library_manager
        .insert_artist(&existing)
        .await
        .unwrap();
    let picture_url = pick
        .images
        .serve_image(&pick.picture_path, bae_test_support::cover_png());

    pick.seed_artists(picture_url);
    let (release_id, _album_id) = pick
        .import()
        .await
        .unwrap_or_else(|error| panic!("import failed: {error}"));

    assert_eq!(pick.track_artist(&release_id).await.id, existing.id);
    assert_eq!(
        pick.images.requests(&pick.picture_path),
        0,
        "an artist the import links to keeps its own picture, so none is fetched"
    );
    shut_down(pick.handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_picture_that_fails_to_download_fails_the_import_before_any_row_and_a_retry_imports() {
    let pick = picked_discogs_release().await;
    let picture_url = pick.images.fail_image(&pick.picture_path, 500);
    pick.seed_artists(picture_url);

    pick.import()
        .await
        .expect_err("a picture that fails to download fails the import");
    let failed_requests = pick.images.requests(&pick.picture_path);
    assert!(failed_requests > 0, "the failed import asked for the picture");
    let rows = pick.handle.library_manager.every_row_for_test().await;
    for table in ["releases", "albums", "artists", "artist_images", "release_files"] {
        assert!(
            rows.get(table).is_none_or(Vec::is_empty),
            "the failed import wrote nothing to {table}"
        );
    }

    pick.images.serve_image(&pick.picture_path, large_picture());
    let (release_id, _album_id) = pick
        .import()
        .await
        .unwrap_or_else(|error| panic!("the retry failed: {error}"));

    assert_eq!(
        pick.images.requests(&pick.picture_path),
        failed_requests + 1,
        "the retry fetches the picture again"
    );
    let artist = pick.track_artist(&release_id).await;
    assert_stored_large_picture(pick.picture_of(&artist.id).await);
    shut_down(pick.handle).await;
}

/// The step `event` reports for `import_id`, if it reports one.
fn step_of(event: ImportEvent, import_id: &str) -> Option<crate::import::ImportStep> {
    let ImportEvent::ImportProgress { progress, .. } = event else {
        return None;
    };
    if progress.import_id() != import_id {
        return None;
    }
    match progress {
        crate::import::ImportProgress::Preparing { step, .. } => {
            Some(crate::import::ImportStep::Preparing(step))
        }
        crate::import::ImportProgress::Progress { phase, .. } => {
            Some(crate::import::ImportStep::Running(phase))
        }
        crate::import::ImportProgress::Complete { .. }
        | crate::import::ImportProgress::RemoteUploadQueued { .. }
        | crate::import::ImportProgress::Failed { .. }
        | crate::import::ImportProgress::Cancelled { .. } => None,
    }
}

/// While the import waits on a new artist's picture, the candidate says it
/// is fetching artist pictures: the step is reported before the picture is
/// asked for, and no file is read until it answers.
#[tokio::test(flavor = "multi_thread")]
async fn the_import_says_it_is_fetching_artist_pictures_while_it_fetches_them() {
    use crate::import::{ImportStep, PrepareStep};
    let pick = picked_discogs_release().await;
    let picture_url = pick.images.serve_image(&pick.picture_path, large_picture());
    let hold = pick.images.hold_image(&pick.picture_path);
    pick.seed_artists(picture_url);

    let mut events = pick.handle.every_event();
    let import_id = pick.handle.start_import(&pick.key).await.unwrap().expect_queued();
    tokio::time::timeout(HANG_GUARD, hold.arrived())
        .await
        .expect("the import asks for the track artist's picture");

    let fetching = ImportStep::Preparing(PrepareStep::FetchingArtistPictures);
    let steps: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| step_of(event, &import_id))
        .collect();
    assert_eq!(
        steps,
        vec![
            ImportStep::Preparing(PrepareStep::ValidatingSourceFiles),
            fetching,
        ],
        "the fetch is reported before the picture is asked for, and nothing is read yet"
    );
    assert_eq!(
        pick.handle
            .candidate_runtime(&pick.key)
            .and_then(|runtime| runtime.import)
            .map(|import| import.step),
        Some(fetching),
        "the candidate shows the fetch while it waits on the picture"
    );

    hold.release();
    let (release_id, _album_id) = await_import_outcome(&mut events, &import_id)
        .await
        .unwrap_or_else(|error| panic!("import failed: {error}"));
    let artist = pick.track_artist(&release_id).await;
    assert_stored_large_picture(pick.picture_of(&artist.id).await);
    shut_down(pick.handle).await;
}
