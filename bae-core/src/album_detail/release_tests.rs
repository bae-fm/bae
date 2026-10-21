use super::*;

/// A track on side `side` of the release, `duration_ms` long when measured.
fn track(
    id: &str,
    side: i32,
    number: i32,
    duration_ms: Option<i64>,
) -> crate::db::DbTrackWithArtists {
    let mut track = crate::db::DbTrack::new_test("release-id", id, "Track Title", Some(number));
    track.side = Some(side);
    track.duration_ms = duration_ms;
    crate::db::DbTrackWithArtists {
        track,
        artists: Vec::new(),
    }
}

/// The detail of a release holding `tracks`.
fn detail(tracks: Vec<crate::db::DbTrackWithArtists>) -> ReleaseDetail {
    let raw = crate::db::DbReleaseDetail {
        release: crate::db::DbRelease::new_test("album-id", "release-id"),
        tracks,
        files: Vec::new(),
        audio_formats: Vec::new(),
        audio_segments: Vec::new(),
        records: Vec::new(),
        unstored_audio: Vec::new(),
    };
    let originals = OriginalsSeen::default();
    let ctx = ReleaseResolveCtx {
        has_cloud_home: false,
        pinned: false,
        originals: &originals,
        cover: None,
        transfer_action: None,
        is_compilation: false,
    };
    ReleaseDetail::from_raw(raw, &[], 0, &ctx).0
}

/// Every track measured: the release's total and each side's are their sums.
#[test]
fn a_fully_measured_release_totals_its_tracks() {
    let detail = detail(vec![
        track("t1", 1, 1, Some(100_000)),
        track("t2", 1, 2, Some(200_000)),
        track("t3", 2, 1, Some(300_000)),
    ]);

    assert_eq!(detail.total_duration_ms, Some(600_000));
    let group_totals: Vec<_> = detail
        .track_groups
        .iter()
        .map(|group| group.total_duration_ms)
        .collect();
    assert_eq!(group_totals, [Some(300_000), Some(300_000)]);
}

/// A total over a track nothing measured is not known: counting the track as
/// zero would show a playing time shorter than the release's. Only a side whose
/// every track is measured keeps its total.
#[test]
fn an_unmeasured_track_leaves_its_release_and_side_without_a_total() {
    let detail = detail(vec![
        track("t1", 1, 1, Some(100_000)),
        track("t2", 1, 2, None),
        track("t3", 2, 1, Some(300_000)),
    ]);

    assert_eq!(detail.total_duration_ms, None);
    let group_totals: Vec<_> = detail
        .track_groups
        .iter()
        .map(|group| group.total_duration_ms)
        .collect();
    assert_eq!(group_totals, [None, Some(300_000)]);
}
