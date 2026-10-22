use super::*;

/// A track on side `side` of the release's record, `duration_ms` long when
/// measured.
fn track(id: &str, side: u8, number: u32, duration_ms: u64) -> crate::db::DbTrackWithArtists {
    let mut track = crate::db::DbTrack::new_test("release-id", id, "Track Title", number);
    track.side = std::num::NonZeroU8::new(side).expect("sides count from one");
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
        mediums: crate::db::DbReleaseMediums(vec![crate::db::DbReleaseMedium::new(
            "release-id",
            std::num::NonZeroU16::MIN,
            crate::mediums::ReleaseMedium {
                carrier: Some(crate::pressing::Medium::Vinyl),
                sides: std::num::NonZeroU8::new(2).expect("two sides"),
            },
            chrono::Utc::now(),
        )]),
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

/// The release's total and each side's are its tracks' summed lengths.
#[test]
fn a_release_totals_its_tracks() {
    let detail = detail(vec![
        track("t1", 1, 1, 100_000),
        track("t2", 1, 2, 200_000),
        track("t3", 2, 1, 300_000),
    ]);

    assert_eq!(detail.total_duration_ms, 600_000);
    let group_totals: Vec<_> = detail
        .track_groups
        .iter()
        .map(|group| group.total_duration_ms)
        .collect();
    assert_eq!(group_totals, [300_000, 300_000]);
}
