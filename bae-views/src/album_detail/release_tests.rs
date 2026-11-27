use super::*;

/// The detail of a release of `release_year`, in an album of `album_year`.
fn detail_in_years(release_year: Option<i32>, album_year: Option<i32>) -> ReleaseDetail {
    detail_of(release_year, album_year, |_| {}, None)
}

/// The detail of a release whose read `edit` changes and whose entry in the
/// download queue is `download`.
fn detail_of(
    release_year: Option<i32>,
    album_year: Option<i32>,
    edit: impl FnOnce(&mut bae_records::DbReleaseDetail),
    download: Option<bae_transfers::DownloadEntry>,
) -> ReleaseDetail {
    let mut raw = detail_raw();
    raw.release.pressing.typed.year = release_year;
    edit(&mut raw);
    let ctx = ReleaseResolveCtx { cover: None, is_compilation: false, album_year };
    ReleaseDetail::from_raw(
        raw,
        download,
        no_storage_actions(),
        &ctx,
        bae_people::people::RatingSystem::Stars,
    )
}

/// The rows of a release with no tracks, files, cover or transfers.
fn detail_raw() -> bae_records::DbReleaseDetail {
    bae_records::DbReleaseDetail {
        release: bae_records::DbRelease::new_test("album-id", "release-id"),
        total_duration_ms: 0,
        track_groups: Vec::new(),
        credits: Vec::new(),
        has_credits: false,
        performs_works: false,
        files: bae_records::DbReleaseFiles {
            files: Vec::new(),
            images: Vec::new(),
            others: Vec::new(),
            carried: Vec::new(),
            has_others: false,
            count: 0,
            image_count: 0,
            total_size: 0,
            source_audio: None,
        },
        records: Vec::new(),
        has_records: false,
        elsewhere: None,
        pinned: false,
        remote: false,
        upload: None,
        cover: None,
        release_number: 1,
    }
}

/// The storage actions of a release that offers none.
fn no_storage_actions() -> crate::DbStorageActions {
    crate::DbStorageActions {
        offered: crate::DbStorageActionFlags {
            upload: false,
            pin: false,
            unpin: false,
            move_to_this_device: false,
            cancel_only: false,
        },
        transitioning: false,
    }
}

/// A line about the pressing names its year only when it differs from the
/// album's, which is shown above it; the release keeps its year either way.
#[test]
fn a_release_names_its_year_only_when_it_differs_from_its_albums() {
    let reissue = detail_in_years(Some(2001), Some(1992));
    assert_eq!((reissue.year, reissue.distinct_year), (Some(2001), Some(2001)));

    let first = detail_in_years(Some(1992), Some(1992));
    assert_eq!((first.year, first.distinct_year), (Some(1992), None));

    let album_unknown = detail_in_years(Some(2001), None);
    assert_eq!(album_unknown.distinct_year, Some(2001));

    let release_unknown = detail_in_years(None, Some(1992));
    assert_eq!(release_unknown.distinct_year, None);
}

/// The page offers playback exactly for a release this device can play.
#[test]
fn a_release_on_another_device_offers_no_playback() {
    let here = detail_in_years(None, None);
    assert!(here.offers_playback);

    let elsewhere = detail_of(
        None,
        None,
        |raw| {
            raw.elsewhere = Some(bae_records::OtherDevice::Active { name: "Device A".to_string() })
        },
        None,
    );
    assert!(!elsewhere.offers_playback);
}

/// The transfer lines show only while files upload or a pin waits, runs or has
/// failed, and an unplayable release keeps its upload line.
#[test]
fn the_transfer_lines_show_only_while_something_transfers() {
    use bae_transfers::{DownloadEntry, DownloadKind, DownloadStep};
    let device = || Some(bae_records::OtherDevice::Active { name: "Device A".to_string() });
    let uploading = |raw: &mut bae_records::DbReleaseDetail| {
        raw.upload =
            Some(bae_records::DbReleaseUpload { waiting: 1, sending: 0, stored: 0, missing: None });
    };
    let pin = |step| Some(DownloadEntry { kind: DownloadKind::Pin, step });

    assert!(!detail_in_years(None, None).shows_transfer_lines);
    assert!(detail_of(None, None, uploading, None).shows_transfer_lines);
    assert!(
        detail_of(
            None,
            None,
            |raw| {
                uploading(raw);
                raw.elsewhere = device();
            },
            None
        )
        .shows_transfer_lines
    );
    for step in [
        DownloadStep::Queued,
        DownloadStep::Running,
        DownloadStep::Failed { error: "no".to_string() },
    ] {
        assert!(detail_of(None, None, |_| {}, pin(step)).shows_transfer_lines);
    }
    let moving =
        Some(DownloadEntry { kind: DownloadKind::MoveToThisDevice, step: DownloadStep::Queued });
    assert!(!detail_of(None, None, |_| {}, moving).shows_transfer_lines);
}

fn image_file(id: &str, name: &str) -> bae_records::DbFile {
    bae_records::DbFile {
        id: id.to_string(),
        release_id: "release-id".to_string(),
        original_filename: name.to_string(),
        file_size: 10,
        content_type: bae_util::content_type::ContentType::Jpeg,
        role: bae_records::ReleaseFileRole::Artwork,
        source_audio: None,
        file: coven::FileId(uuid::Uuid::nil()),
        stored: true,
        created_at: time::UtcDateTime::now(),
    }
}

/// The gallery of a release with two image files, whose cover is `cover_source`.
fn gallery_of(cover_source: Option<&str>, has_cover: bool) -> ReleaseDetail {
    let mut raw = detail_raw();
    raw.files.images = vec![image_file("back", "back.jpg"), image_file("front", "front.jpg")];
    raw.files.image_count = 2;
    raw.cover = has_cover.then(|| bae_records::DbReleaseCover {
        version: "v1".to_string(),
        source: match cover_source {
            Some(file_id) => bae_records::CoverSource::ReleaseFile { file_id: file_id.to_string() },
            None => bae_records::CoverSource::Original,
        },
    });
    let cover = has_cover.then(|| bae_records::ImageRef {
        id: "release-id".to_string(),
        version: "v1".to_string(),
        image_type: bae_records::LibraryImageType::Cover,
    });
    let ctx = ReleaseResolveCtx { cover, is_compilation: false, album_year: None };
    ReleaseDetail::from_raw(
        raw,
        None,
        no_storage_actions(),
        &ctx,
        bae_people::people::RatingSystem::Stars,
    )
}

/// A cover that keeps a picture of its own is a slot beside the release's
/// image files.
#[test]
fn a_cover_with_a_picture_of_its_own_is_a_gallery_slot_before_the_image_files() {
    let detail = gallery_of(None, true);

    let labels: Vec<_> = detail.gallery_items.iter().map(|item| item.label.clone()).collect();
    assert_eq!(
        labels,
        [
            GalleryLabel::Cover,
            GalleryLabel::File { name: "back.jpg".to_string() },
            GalleryLabel::File { name: "front.jpg".to_string() },
        ]
    );
    assert!(matches!(detail.gallery_items[0].source, GallerySource::Cover(_)));
    assert_eq!(detail.gallery_item_count, 3);
}

/// A cover made from one of the release's image files is that file's slot, so
/// the picture shows once, first and as the cover.
#[test]
fn a_cover_made_from_an_image_file_shows_once_as_that_files_slot() {
    let detail = gallery_of(Some("front"), true);

    let slots: Vec<_> =
        detail.gallery_items.iter().map(|item| (item.id.clone(), item.label.clone())).collect();
    assert_eq!(
        slots,
        [
            ("front".to_string(), GalleryLabel::Cover),
            ("back".to_string(), GalleryLabel::File { name: "back.jpg".to_string() }),
        ]
    );
    assert_eq!(
        detail.gallery_items[0].source,
        GallerySource::ReleaseFile { file_id: "front".to_string() }
    );
    assert_eq!(detail.gallery_item_count, 2);
}

/// A release with no cover shows only its image files.
#[test]
fn a_release_with_no_cover_has_a_slot_for_each_image_file() {
    let detail = gallery_of(None, false);

    assert_eq!(detail.gallery_items.len(), 2);
    assert_eq!(detail.gallery_item_count, 2);
}
