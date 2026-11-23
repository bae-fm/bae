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
    let mut release = bae_records::DbRelease::new_test("album-id", "release-id");
    release.pressing.year = release_year;
    let mut raw = bae_records::DbReleaseDetail {
        release,
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
        cover_version: None,
        release_number: 1,
    };
    edit(&mut raw);
    let ctx = ReleaseResolveCtx { cover: None, is_compilation: false, album_year };
    let actions = crate::DbStorageActions {
        offered: crate::DbStorageActionFlags {
            upload: false,
            pin: false,
            unpin: false,
            move_to_this_device: false,
            cancel_only: false,
        },
        transitioning: false,
    };
    ReleaseDetail::from_raw(raw, download, actions, &ctx, bae_people::people::RatingSystem::Stars)
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
