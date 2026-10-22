use super::*;
use crate::import::release_metadata::{AlbumMetadata, ReleaseMetadata};
use crate::import::source_release::{
    CatalogFacts, EntryKind, ReleaseCovers, SourceMedium, TracklistEntry,
};
use crate::import::{ArtistCredit, MetadataRef};
use crate::pressing::{Medium, Pressing, StatedMedia};
use crate::signals::{AudioSource, CdProof};

/// One tracklist row: a track at `position` with `title`, stating `length`.
fn track(position: &str, title: &str, length: Option<u64>) -> TracklistEntry {
    TracklistEntry {
        kind: EntryKind::Track,
        position: (!position.is_empty()).then(|| position.to_string()),
        number: None,
        title: Some(title.to_string()),
        duration_ms: length,
        credits: Vec::new(),
        roles: Vec::new(),
        works: Vec::new(),
        children: Vec::new(),
    }
}

/// `count` tracks positioned `<prefix><n>`, titled `<name> <n>`, each
/// stating `length`.
fn run(prefix: &str, name: &str, count: usize, length: Option<u64>) -> Vec<TracklistEntry> {
    (1..=count)
        .map(|n| track(&format!("{prefix}{n}"), &format!("{name} {n}"), length))
        .collect()
}

fn medium(carrier: Option<Medium>, entries: Vec<TracklistEntry>) -> SourceMedium {
    SourceMedium {
        medium: carrier,
        disc_ids: Vec::new(),
        entries,
    }
}

/// An imaginary release of `catalog` holding `mediums`, credited to one
/// album artist.
fn release(catalog: Catalog, mediums: Vec<SourceMedium>) -> SourceRelease {
    SourceRelease {
        release: MetadataRef::new(catalog, "release-1"),
        source_group_id: None,
        metadata: ReleaseMetadata {
            album: AlbumMetadata {
                title: "Album Title".to_string(),
                artists: vec![ArtistCredit {
                    name: "Artist Name".to_string(),
                    sort_name: None,
                    musicbrainz_artist_id: None,
                    discogs_artist_id: None,
                }],
                year: None,
                first_year: None,
            },
            pressing: Pressing::blank(),
        },
        other_records: Vec::new(),
        covers: ReleaseCovers {
            release: Vec::new(),
            album: Vec::new(),
        },
        archive_release: None,
        archive_groups: Vec::new(),
        mediums,
        catalog: match catalog {
            Catalog::MusicBrainz => CatalogFacts::MusicBrainz {
                links: Vec::new(),
                album_links: crate::import::album_links::AlbumLinks::NotAsked,
            },
            Catalog::Discogs => CatalogFacts::Discogs {
                media: StatedMedia::Undescribed,
                release_roles: Vec::new(),
            },
            other => unreachable!("no release is fetched from {}", other.as_str()),
        },
        notes: Vec::new(),
        unfetched: Vec::new(),
    }
}

fn audio(lengths: Vec<u64>, origin: AudioOrigin, disc_ids: &[&str]) -> ReadAudio {
    ReadAudio {
        lengths: MeasuredLengths::new(lengths).expect("the test audio has tracks"),
        origin,
        disc_ids: disc_ids.iter().map(|id| id.to_string()).collect(),
    }
}

fn proven_cd_rip() -> AudioOrigin {
    AudioOrigin {
        source: Some(AudioSource::CdRip {
            proof: CdProof::RipLog,
            file: None,
        }),
        not_cd_rate: None,
    }
}

fn titles(reading: &Result<ReleaseValues, ReadingRefusal>) -> Vec<&str> {
    reading
        .as_ref()
        .expect("the release reads onto the audio")
        .tracks
        .iter()
        .map(|track| track.title.as_str())
        .collect()
}

const CD1: [u64; 12] = [
    201_000, 202_000, 203_000, 204_000, 205_000, 206_000, 207_000, 208_000, 209_000, 210_000,
    211_000, 212_000,
];
const CD2: [u64; 10] = [
    301_000, 302_000, 303_000, 304_000, 305_000, 306_000, 307_000, 308_000, 309_000, 310_000,
];

fn stated(lengths: &[u64], prefix: &str, name: &str) -> Vec<TracklistEntry> {
    lengths
        .iter()
        .enumerate()
        .map(|(i, &length)| track(&format!("{prefix}{}", i + 1), &format!("{name} {}", i + 1), Some(length)))
        .collect()
}

/// The imaginary mixed box as MusicBrainz lists it: two records (the first
/// stating LP1's lengths, which match CD1's; the second stating none), two
/// CDs stating their lengths, and a Blu-ray.
fn mixed_box_musicbrainz() -> SourceRelease {
    let mut lp1 = stated(&CD1[..6], "A", "LP Side A");
    lp1.extend(stated(&CD1[6..], "B", "LP Side B"));
    let mut lp2 = run("C", "LP Side C", 5, None);
    lp2.extend(run("D", "LP Side D", 5, None));
    release(
        Catalog::MusicBrainz,
        vec![
            medium(Some(Medium::Vinyl), lp1),
            medium(Some(Medium::Vinyl), lp2),
            medium(Some(Medium::Cd), stated(&CD1, "", "CD One")),
            medium(Some(Medium::Cd), stated(&CD2, "", "CD Two")),
            medium(Some(Medium::BluRay), run("", "Film", 1, Some(5_400_000))),
        ],
    )
}

/// The same box as Discogs lists it once split into its mediums: headings
/// over each record and disc, `CD1-1` and `BD-1` positions.
fn mixed_box_discogs() -> SourceRelease {
    let heading = |title: &str| TracklistEntry {
        kind: EntryKind::Heading,
        position: None,
        ..track("", title, None)
    };
    let with_heading = |title: &str, rows: Vec<TracklistEntry>| {
        std::iter::once(heading(title)).chain(rows).collect::<Vec<_>>()
    };
    let mut lp1 = stated(&CD1[..6], "A", "LP Side A");
    lp1.extend(stated(&CD1[6..], "B", "LP Side B"));
    let mut lp2 = run("C", "LP Side C", 5, None);
    lp2.extend(run("D", "LP Side D", 5, None));
    release(
        Catalog::Discogs,
        vec![
            medium(Some(Medium::Vinyl), with_heading("Records", lp1)),
            medium(Some(Medium::Vinyl), lp2),
            medium(Some(Medium::Cd), with_heading("Discs", stated(&CD1, "CD1-", "CD One"))),
            medium(Some(Medium::Cd), stated(&CD2, "CD2-", "CD Two")),
            medium(
                Some(Medium::BluRay),
                with_heading("Film", run("BD-", "Film", 1, Some(5_400_000))),
            ),
        ],
    )
}

/// A CD rip of the box's two discs reads the two CDs' tracks, in either
/// catalog's shape, never a record's.
#[test]
fn a_cd_rip_of_the_mixed_box_reads_its_two_cds() {
    let rip: Vec<u64> = CD1.iter().chain(&CD2).map(|length| length + 400).collect();
    let audio = audio(rip, proven_cd_rip(), &[]);
    for release in [mixed_box_musicbrainz(), mixed_box_discogs()] {
        let reading = release.read(&audio);
        let read = titles(&reading);
        assert_eq!(read.len(), 22);
        assert_eq!(read[0], "CD One 1");
        assert_eq!(read[12], "CD Two 1");
        assert!(read.iter().all(|title| title.starts_with("CD ")), "{read:?}");
        let values = reading.as_ref().expect("the release reads");
        assert_eq!(
            values.mediums,
            [
                crate::mediums::ReleaseMedium::one_sided(Some(Medium::Cd)),
                crate::mediums::ReleaseMedium::one_sided(Some(Medium::Cd)),
            ],
            "the release holds the two CDs the rip covers, and none of the box's records"
        );
        let labels = crate::import::release_values::value_labels(values);
        assert_eq!(labels[0], "1-1");
        assert_eq!(labels[11], "1-12");
        assert_eq!(labels[12], "2-1", "disc 2 numbers its own tracks");
        assert_eq!(labels[21], "2-10");
    }
}

/// The second record states no lengths; the second CD states ones the rip
/// agrees with. Whether or not the audio is proven a CD rip, the CD's stated
/// lengths win.
#[test]
fn unstated_lengths_never_outrank_stated_ones_that_agree() {
    let rip: Vec<u64> = CD2.iter().map(|length| length - 700).collect();
    for origin in [AudioOrigin::default(), proven_cd_rip()] {
        let reading = mixed_box_musicbrainz().read(&audio(rip.clone(), origin, &[]));
        assert_eq!(titles(&reading)[0], "CD Two 1");
    }
}

/// The first record's lengths match the first CD's exactly. Audio proven a
/// CD rip rules the record out; audio that proves nothing leaves the two
/// tied, reading different titles, and the reading names both instead of
/// taking the earlier.
#[test]
fn tied_discs_that_read_differently_are_refused_naming_them() {
    let rip = CD1.to_vec();
    let proven = mixed_box_musicbrainz().read(&audio(rip.clone(), proven_cd_rip(), &[]));
    assert_eq!(titles(&proven)[0], "CD One 1");

    let audio = audio(rip, AudioOrigin::default(), &[]);
    let tied = mixed_box_musicbrainz().read(&audio);
    assert_eq!(tied, Err(ReadingRefusal::Tied { mediums: vec![1, 3] }));
    assert_eq!(
        listing(&tied, &audio),
        SourceTracks::Listed { count: 12 },
        "the tracklist still holds the folder's tracks by count"
    );
    assert!(matches!(
        ImportError::from(tied.unwrap_err()),
        ImportError::TiedMediums { mediums } if mediums == vec![1, 3]
    ));
}

/// A hybrid SACD lists its two layers as two mediums of one tracklist: tied,
/// they read the same values, and that is the one answer.
#[test]
fn identical_layers_of_a_hybrid_disc_are_one_answer() {
    let layer = || run("", "Movement", 4, Some(400_000));
    let release = release(
        Catalog::MusicBrainz,
        vec![
            medium(Some(Medium::Sacd), layer()),
            medium(Some(Medium::Sacd), layer()),
        ],
    );
    let reading = release.read(&audio(vec![400_200; 4], proven_cd_rip(), &[]));
    assert_eq!(titles(&reading), vec!["Movement 1", "Movement 2", "Movement 3", "Movement 4"]);
}

/// A medium MusicBrainz registers one of the folder's Disc IDs against is
/// the folder's, whatever the lengths say.
#[test]
fn a_disc_id_decides_over_lengths() {
    let mut second = medium(Some(Medium::Cd), run("", "Second Disc", 3, Some(500_000)));
    second.disc_ids = vec!["disc-id-two".to_string()];
    let release = release(
        Catalog::MusicBrainz,
        vec![
            medium(Some(Medium::Cd), run("", "First Disc", 3, Some(200_000))),
            second,
        ],
    );
    let reading = release.read(&audio(vec![200_000; 3], proven_cd_rip(), &["disc-id-two"]));
    assert_eq!(titles(&reading)[0], "Second Disc 1");
}

/// A medium that lists no tracks is never covered, so it neither adds to a
/// set nor ties with one.
#[test]
fn an_empty_medium_is_never_covered() {
    let release = release(
        Catalog::MusicBrainz,
        vec![
            medium(Some(Medium::Cd), Vec::new()),
            medium(Some(Medium::Cd), run("", "Song", 2, None)),
        ],
    );
    let reading = release.read(&audio(vec![200_000; 2], AudioOrigin::default(), &[]));
    assert_eq!(titles(&reading), vec!["Song 1", "Song 2"]);
}

/// Audio no set of mediums holds is refused with both counts, the release's
/// being every track it lists.
#[test]
fn audio_no_set_of_mediums_holds_is_refused_with_both_counts() {
    let release = release(
        Catalog::MusicBrainz,
        vec![
            medium(Some(Medium::Cd), run("", "Song", 3, None)),
            medium(Some(Medium::Cd), run("", "Other", 4, None)),
        ],
    );
    let audio = audio(vec![200_000; 2], AudioOrigin::default(), &[]);
    let reading = release.read(&audio);
    assert_eq!(
        reading,
        Err(ReadingRefusal::TrackCount {
            folder: 2,
            release: 7
        })
    );
    assert_eq!(listing(&reading, &audio), SourceTracks::Listed { count: 7 });
}

/// A track with no title in the mediums the audio covers makes the reading
/// unreadable, never a fit.
#[test]
fn an_untitled_track_in_a_covered_medium_is_never_a_fit() {
    let mut rows = run("", "Song", 2, None);
    rows[1].title = None;
    let release = release(Catalog::MusicBrainz, vec![medium(Some(Medium::Cd), rows)]);
    let audio = audio(vec![200_000; 2], AudioOrigin::default(), &[]);
    let reading = release.read(&audio);
    assert!(matches!(reading, Err(ReadingRefusal::Unreadable { .. })), "{reading:?}");
    let listed = listing(&reading, &audio);
    assert_eq!(listed, SourceTracks::Unreadable);
    assert_eq!(
        crate::identify::TracklistFit::of(Some(&listed), 2),
        crate::identify::TracklistFit::Unreadable
    );
}

/// A release that lists no tracks reads as its album alone.
#[test]
fn a_release_listing_no_tracks_reads_its_album_alone() {
    let release = release(Catalog::MusicBrainz, Vec::new());
    let audio = audio(vec![200_000; 2], AudioOrigin::default(), &[]);
    let reading = release.read(&audio);
    let values = reading.as_ref().expect("an album alone reads");
    assert!(values.tracks.is_empty());
    assert_eq!(values.album_title, "Album Title");
    assert_eq!(listing(&reading, &audio), SourceTracks::Nothing);
}

/// The collapsed suite: disc 1 is nine tracks and an index of three
/// sub-tracks, disc 2 ten tracks. A rip of disc 1 with the suite as one file
/// holds ten tracks, and reads disc 1 with the suite collapsed — not disc 2,
/// whose ten tracks the expanded count of disc 1 used to leave alone in the
/// running — and the count the fit reads is the draft's.
#[test]
fn a_collapsed_suite_reads_its_own_disc() {
    let suite = TracklistEntry {
        kind: EntryKind::Index,
        children: vec![
            track("1-10a", "Part One", Some(100_000)),
            track("1-10b", "Part Two", Some(100_000)),
            track("1-10c", "Part Three", Some(100_000)),
        ],
        ..track("1-10", "Suite", None)
    };
    let mut disc_one = run("1-", "First", 9, Some(200_000));
    disc_one.push(suite);
    let release = release(
        Catalog::Discogs,
        vec![
            medium(None, disc_one),
            medium(None, run("2-", "Second", 10, Some(250_000))),
        ],
    );
    let mut rip = vec![200_000; 9];
    rip.push(300_000);
    let audio = audio(rip, AudioOrigin::default(), &[]);
    let reading = release.read(&audio);
    let read = titles(&reading);
    assert_eq!(read[0], "First 1");
    assert_eq!(read[9], "Suite");
    assert_eq!(
        listing(&reading, &audio),
        SourceTracks::Listed {
            count: u32::try_from(read.len()).expect("ten fits")
        }
    );
}
