use super::*;

/// The detail of a release of `release_year`, in an album of `album_year`.
fn detail_in_years(release_year: Option<i32>, album_year: Option<i32>) -> ReleaseDetail {
    let mut release = bae_records::DbRelease::new_test("album-id", "release-id");
    release.pressing.year = release_year;
    let raw = bae_records::DbReleaseDetail {
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
    ReleaseDetail::from_raw(raw, None, actions, &ctx, bae_people::people::RatingSystem::Stars)
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
