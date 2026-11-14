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
        files: bae_records::DbReleaseFiles {
            files: Vec::new(),
            images: Vec::new(),
            covers: Vec::new(),
            others: Vec::new(),
            count: 0,
            total_size: 0,
        },
        records: Vec::new(),
        unstored_audio: Vec::new(),
        pinned: false,
        remote: false,
        upload: None,
        cover_version: None,
    };
    let ctx = ReleaseResolveCtx { syncs: false, cover: None, is_compilation: false, album_year };
    ReleaseDetail::from_raw(raw, None, 0, &ctx)
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
