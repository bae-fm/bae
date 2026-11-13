use super::*;

/// A track on side `side` of the release's record, `duration_ms` long when
/// measured.
fn track(id: &str, side: u8, number: u32, duration_ms: u64) -> bae_records::DbTrackWithArtists {
    let mut track = bae_records::DbTrack::new_test("release-id", id, "Track Title", number);
    track.side = std::num::NonZeroU8::new(side).expect("sides count from one");
    track.duration_ms = duration_ms;
    bae_records::DbTrackWithArtists {
        track,
        artists: Vec::new(),
        shown_artists: Vec::new(),
        works: Vec::new(),
    }
}

/// The detail of a release holding `tracks`.
fn detail(tracks: Vec<bae_records::DbTrackWithArtists>) -> ReleaseDetail {
    detail_in_years(tracks, None, None)
}

/// The detail of a release of `release_year`, in an album of `album_year`.
fn detail_in_years(
    tracks: Vec<bae_records::DbTrackWithArtists>,
    release_year: Option<i32>,
    album_year: Option<i32>,
) -> ReleaseDetail {
    detail_of(tracks, Vec::new(), release_year, album_year)
}

/// The detail of a release holding `files`.
fn detail_with_files(files: Vec<bae_records::DbFile>) -> ReleaseDetail {
    detail_of(Vec::new(), files, None, None)
}

fn detail_of(
    tracks: Vec<bae_records::DbTrackWithArtists>,
    files: Vec<bae_records::DbFile>,
    release_year: Option<i32>,
    album_year: Option<i32>,
) -> ReleaseDetail {
    let mut release = bae_records::DbRelease::new_test("album-id", "release-id");
    release.pressing.year = release_year;
    let raw = bae_records::DbReleaseDetail {
        release,
        mediums: bae_records::DbReleaseMediums(vec![bae_records::DbReleaseMedium::new(
            &coven::UuidIds,
            "release-id",
            std::num::NonZeroU16::MIN,
            bae_types::mediums::ReleaseMedium {
                carrier: Some(bae_types::pressing::Medium::Vinyl),
                sides: std::num::NonZeroU8::new(2).expect("two sides"),
            },
            chrono::Utc::now(),
        )]),
        tracks,
        credits: Vec::new(),
        files,
        records: Vec::new(),
        unstored_audio: Vec::new(),
        pinned: false,
        remote: false,
        upload: None,
        cover_version: None,
    };
    let ctx = ReleaseResolveCtx {
        syncs: false,
        cover: None,
        is_compilation: false,
        album_year,
        my_stars: HashMap::new(),
    };
    ReleaseDetail::from_raw(raw, None, 0, &ctx)
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
    let group_totals: Vec<_> =
        detail.track_groups.iter().map(|group| group.total_duration_ms).collect();
    assert_eq!(group_totals, [300_000, 300_000]);
}

/// A line about the pressing names its year only when it differs from the
/// album's, which is shown above it; the release keeps its year either way.
#[test]
fn a_release_names_its_year_only_when_it_differs_from_its_albums() {
    let reissue = detail_in_years(Vec::new(), Some(2001), Some(1992));
    assert_eq!((reissue.year, reissue.distinct_year), (Some(2001), Some(2001)));

    let first = detail_in_years(Vec::new(), Some(1992), Some(1992));
    assert_eq!((first.year, first.distinct_year), (Some(1992), None));

    let album_unknown = detail_in_years(Vec::new(), Some(2001), None);
    assert_eq!(album_unknown.distinct_year, Some(2001));

    let release_unknown = detail_in_years(Vec::new(), None, Some(1992));
    assert_eq!(release_unknown.distinct_year, None);
}

/// A file of `role` named `name`; audio states a source layout when `layout`
/// is given.
fn file(
    name: &str,
    role: bae_records::ReleaseFileRole,
    layout: Option<bae_records::source_audio::SourceAudioLayout>,
) -> bae_records::DbFile {
    let audio = role == bae_records::ReleaseFileRole::Audio;
    bae_records::DbFile {
        id: format!("id-{name}"),
        release_id: "release-id".to_string(),
        original_filename: name.to_string(),
        file_size: 1_024,
        content_type: if audio {
            bae_util::content_type::ContentType::Flac
        } else {
            bae_util::content_type::ContentType::PlainText
        },
        role,
        source_audio: audio.then(|| bae_records::source_audio::SourceAudioFile {
            layout,
            content_type: bae_util::content_type::ContentType::Flac,
            container: bae_util::audio_container::AudioContainer::from_name("flac"),
            duration_ms: 60_000,
            format: bae_types::audio_format::AudioFormat {
                codec: "FLAC".to_string(),
                sample_rate_hz: 44_100,
                bits_per_sample: Some(16),
                bitrate_kbps: None,
                channels: 2,
            },
        }),
        file: coven::FileId(uuid::Uuid::nil()),
        stored: false,
        created_at: chrono::DateTime::UNIX_EPOCH,
    }
}

/// A release of a FLAC image lists its sheet, its log and its image besides
/// the audio, in the release's file order; the audio no track could be laid
/// out from is listed too.
#[test]
fn a_release_lists_the_files_no_track_plays_from() {
    use bae_records::source_audio::SourceAudioLayout;
    use bae_records::ReleaseFileRole::{Artwork, Audio, Document, TrackSheet};
    let detail = detail_with_files(vec![
        file("disc.flac", Audio, Some(SourceAudioLayout::Cue)),
        file("disc.cue", TrackSheet, None),
        file("rip.log", Document(bae_records::DocumentKind::RipLog), None),
        file("cover.jpg", Artwork, None),
        file("odd.flac", Audio, None),
    ]);

    let names: Vec<_> =
        detail.other_files.iter().map(|file| file.original_filename.as_str()).collect();
    assert_eq!(names, ["disc.cue", "rip.log", "cover.jpg", "odd.flac"]);
    assert_eq!(detail.files.len(), 5);
}
