//! The release detail: its track artists, its files and its index among its
//! album's releases, in order.

use crate::*;
use bae_records::test_fixtures::aggregate_ordering::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::*;
use bae_store::test_fixtures::temp_db;
use bae_types::credit_role::{CreditRoleKind, CreditedAs};

#[tokio::test]
async fn release_detail_orders_track_artists_and_keeps_tracks_without_artists() {
    let (db, _tmp) = release_detail_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let detail = crate::LibraryViews::new(db.clone())
        .find_db_release_detail(RELEASE_A)
        .await
        .unwrap()
        .expect("release detail");

    let tracks: Vec<&bae_records::DbTrackWithArtists> =
        detail.track_groups.iter().flat_map(|group| &group.tracks).collect();
    let track_ids: Vec<&str> = tracks.iter().map(|track| track.id.as_str()).collect();
    assert_eq!(track_ids, vec![TRACK_A, TRACK_B]);

    let artist_names: Vec<&str> =
        tracks[0].artists.iter().map(|artist| artist.artist.name.as_str()).collect();
    assert_eq!(artist_names, vec!["Track Artist Name First", "Track Artist Name Second"]);
    assert!(tracks[1].artists.is_empty());

    // Each track shows its own credits, and one with none its album's: the
    // detail and every other reader take that from the one SQL rule.
    let names = |credits: &[bae_records::credits::LibraryCredit]| {
        credits.iter().map(|credit| credit.artist.name.clone()).collect::<Vec<_>>()
    };
    let album_artists = records.get_artists_for_album(&detail.release.album_id).await.unwrap();
    assert!(!album_artists.is_empty(), "the fixture's album credits an artist");
    assert_eq!(names(&tracks[0].shown_artists), artist_names);
    assert_eq!(names(&tracks[1].shown_artists), names(&album_artists));
    for track in &tracks {
        let shown = records.get_shown_artists_for_track(&track.id).await.unwrap();
        assert_eq!(names(&shown), names(&track.shown_artists), "{}", track.id);
    }
    assert!(records.get_shown_artists_for_track("no-such-track").await.unwrap().is_empty());
}

/// A release's detail lists its files in natural order of their names, case
/// aside, and the read under it walks the index of each release's files in
/// that order, with their pins and uploads.
#[tokio::test]
async fn release_detail_files_are_in_case_insensitive_natural_order() {
    let (db, tmp) = release_detail_db(crate::TEST_SCHEMA).await;
    insert_files_out_of_natural_order(&db, tmp.path()).await;

    let detail = crate::LibraryViews::new(db.clone())
        .find_db_release_detail(RELEASE_A)
        .await
        .unwrap()
        .expect("release detail");
    let names: Vec<&str> =
        detail.files.files.iter().map(|f| f.original_filename.as_str()).collect();
    assert_eq!(names, FILES_IN_NATURAL_ORDER, "the release's detail");

    let plan = db
        .read(|sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", crate::storage_inspector::INSPECTOR_FILES),
                [RELEASE_A],
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("USING INDEX idx_release_files_release_order"))
            && !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "the detail's files walk the index with their pins and uploads: {plan:?}"
    );
}

/// A release's index among its album's releases is how many come before it
/// in the album's order, by place and then id, counted on the album's
/// releases index.
#[tokio::test]
async fn a_releases_index_counts_the_releases_before_it() {
    let (db, _tmp) = aggregate_db(crate::TEST_SCHEMA).await;
    // Places 0, 1, 1: the two at place 1 go by id.
    for (release_id, index) in [
        ("8aa66d48-65a0-42e4-8c1d-e7481e8c1861", 0),
        ("0252dedb-ee39-4547-8803-438dbeb57a64", 1),
        ("64e79a1f-404a-4c34-809a-a3cb44bf1942", 2),
    ] {
        let context = crate::LibraryViews::new(db.clone())
            .find_release_detail_context(release_id)
            .await
            .unwrap()
            .expect("the release exists");
        assert_eq!(context.release_index, index, "{release_id}");
    }

    let plan = db
        .read(|sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", crate::release::RELEASE_INDEX_IN_ALBUM),
                ["8aa66d48-65a0-42e4-8c1d-e7481e8c1861"],
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("idx_releases_album_id")),
        "the count reads the album's releases index: {plan:?}"
    );
}

const CREDIT_ALBUM: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d01";
const CREDIT_RELEASE: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d02";
const CREDITED_WORK: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d03";
const PRODUCER_ONE: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d11";
const PRODUCER_TWO: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d12";
const ARRANGER_OF_TRACKS: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d13";
const ARRANGER_OF_WORK: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d14";
const COMPOSER_OF_WORK: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d15";
const CREDIT_TRACKS: [&str; 6] = [
    "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d21",
    "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d22",
    "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d23",
    "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d24",
    "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d25",
    "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d26",
];

/// A release lists the credits of itself, of its tracks and of the works its
/// tracks perform in a role other than a writing one, by role and artist: two
/// producers of the whole release are one row, an arranger of two tracks one
/// row naming them, and a work's composer is not among them.
#[tokio::test]
async fn a_release_lists_its_credits_by_role_and_artist_with_the_tracks_each_covers() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let source = bae_catalogs::catalog::Catalog::MusicBrainz;
    RecordsFixture::new()
        .artist(PRODUCER_ONE, "Producer Name One")
        .artist(PRODUCER_TWO, "Producer Name Two")
        .artist(ARRANGER_OF_TRACKS, "Arranger Name One")
        .artist(ARRANGER_OF_WORK, "Arranger Name Two")
        .artist(COMPOSER_OF_WORK, "Composer Name")
        .album(CREDIT_ALBUM, "Album Title", None, &[PRODUCER_ONE])
        .release(CREDIT_ALBUM, CREDIT_RELEASE)
        .tracks(
            CREDIT_RELEASE,
            &[
                (CREDIT_TRACKS[0], "Track Title 1"),
                (CREDIT_TRACKS[1], "Track Title 2"),
                (CREDIT_TRACKS[2], "Track Title 3"),
                (CREDIT_TRACKS[3], "Track Title 4"),
                (CREDIT_TRACKS[4], "Track Title 5"),
                (CREDIT_TRACKS[5], "Track Title 6"),
            ],
        )
        .release_role(
            "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d31",
            CREDIT_RELEASE,
            PRODUCER_ONE,
            (source, CreditedAs::of(CreditRoleKind::Producer)),
        )
        .release_role(
            "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d32",
            CREDIT_RELEASE,
            PRODUCER_TWO,
            (source, CreditedAs::of(CreditRoleKind::Producer)),
        )
        .track_role(
            "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d33",
            CREDIT_TRACKS[1],
            ARRANGER_OF_TRACKS,
            (source, CreditedAs::of(CreditRoleKind::Arranger)),
        )
        .track_role(
            "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d34",
            CREDIT_TRACKS[4],
            ARRANGER_OF_TRACKS,
            (source, CreditedAs::of(CreditRoleKind::Arranger)),
        )
        .work(CREDITED_WORK, "Work Title")
        .composer(CREDITED_WORK, COMPOSER_OF_WORK)
        .work_credit(CREDITED_WORK, ARRANGER_OF_WORK, CreditedAs::of(CreditRoleKind::Arranger))
        .performs(CREDIT_TRACKS[2], CREDITED_WORK)
        .write(&bae_records::test_fixtures::test_records(&db))
        .await;

    let detail = crate::LibraryViews::new(db.clone())
        .find_release_detail(CREDIT_RELEASE)
        .await
        .unwrap()
        .unwrap();

    let rows: Vec<(CreditedAs, Vec<&str>, Vec<String>)> = detail
        .credits
        .iter()
        .map(|row| {
            (
                row.role.clone(),
                row.artists.names().iter().map(|credited| credited.name.as_str()).collect(),
                row.tracks.clone(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                CreditedAs::of(CreditRoleKind::Producer),
                vec!["Producer Name One", "Producer Name Two"],
                vec![]
            ),
            (
                CreditedAs::of(CreditRoleKind::Arranger),
                vec!["Arranger Name One"],
                vec!["2".to_string(), "5".to_string()]
            ),
            (
                CreditedAs::of(CreditRoleKind::Arranger),
                vec!["Arranger Name Two"],
                vec!["3".to_string()]
            ),
        ]
    );
}

const SIDED_ALBUM: &str = "b8d2f6e3-4c50-4d79-8e1f-2a3b4c5d6e01";
const SIDED_RELEASE: &str = "b8d2f6e3-4c50-4d79-8e1f-2a3b4c5d6e02";
const SIDED_ENGINEER: &str = "b8d2f6e3-4c50-4d79-8e1f-2a3b4c5d6e03";
const SIDED_TRACKS: [&str; 5] = [
    "b8d2f6e3-4c50-4d79-8e1f-2a3b4c5d6e11",
    "b8d2f6e3-4c50-4d79-8e1f-2a3b4c5d6e12",
    "b8d2f6e3-4c50-4d79-8e1f-2a3b4c5d6e13",
    "b8d2f6e3-4c50-4d79-8e1f-2a3b4c5d6e14",
    "b8d2f6e3-4c50-4d79-8e1f-2a3b4c5d6e15",
];

/// A release holding a CD, then a record of two sides, then a record of one:
/// the CD is a numbered disc, the sides of the records are lettered across
/// both records, each group sums its own tracks, and a credit over the
/// record sides names their letters.
#[tokio::test]
async fn a_release_groups_its_tracks_by_side_across_its_mediums() {
    use bae_types::mediums::{ReleaseMedium, TrackSide};
    use bae_types::pressing::Medium;
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let release = SIDED_RELEASE;
    let medium = |position: u16, carrier, sides: u8| {
        let mut row = bae_records::DbReleaseMedium::new_test(release);
        row.id = format!("{release}-medium-{position}");
        row.position = std::num::NonZeroU16::new(position).unwrap();
        row.medium = ReleaseMedium {
            carrier: Some(carrier),
            sides: std::num::NonZeroU8::new(sides).unwrap(),
        };
        row
    };
    RecordsFixture::new()
        .artist(SIDED_ENGINEER, "Engineer Name")
        .album(SIDED_ALBUM, "Album Title", None, &[SIDED_ENGINEER])
        .release(SIDED_ALBUM, release)
        .tracks(
            release,
            &[
                (SIDED_TRACKS[0], "Track Title 1"),
                (SIDED_TRACKS[1], "Track Title 2"),
                (SIDED_TRACKS[2], "Track Title 3"),
                (SIDED_TRACKS[3], "Track Title 4"),
                (SIDED_TRACKS[4], "Track Title 5"),
            ],
        )
        .track_role(
            "b8d2f6e3-4c50-4d79-8e1f-2a3b4c5d6e21",
            SIDED_TRACKS[2],
            SIDED_ENGINEER,
            (bae_catalogs::catalog::Catalog::MusicBrainz, CreditedAs::of(CreditRoleKind::Engineer)),
        )
        .track_role(
            "b8d2f6e3-4c50-4d79-8e1f-2a3b4c5d6e22",
            SIDED_TRACKS[3],
            SIDED_ENGINEER,
            (bae_catalogs::catalog::Catalog::MusicBrainz, CreditedAs::of(CreditRoleKind::Engineer)),
        )
        .edit(|rows| {
            rows.mediums = vec![
                medium(1, Medium::Cd, 1),
                medium(2, Medium::Vinyl, 2),
                medium(3, Medium::Vinyl, 1),
            ];
            // (medium position, side, number on the side, length)
            let places = [(1, 1, 1, 100_000), (1, 1, 2, 200_000), (2, 1, 1, 300_000)]
                .into_iter()
                .chain([(2, 2, 1, 400_000), (3, 1, 1, 500_000)]);
            for (track, (position, side, number, duration_ms)) in rows.tracks.iter_mut().zip(places)
            {
                track.medium_id = format!("{release}-medium-{position}");
                track.side = std::num::NonZeroU8::new(side).unwrap();
                track.track_number = std::num::NonZeroU32::new(number).unwrap();
                track.duration_ms = duration_ms;
            }
        })
        .write(&bae_records::test_fixtures::test_records(&db))
        .await;

    let detail =
        crate::LibraryViews::new(db.clone()).find_release_detail(release).await.unwrap().unwrap();

    let groups: Vec<(TrackSide, Vec<&str>, u64)> = detail
        .track_groups
        .iter()
        .map(|group| {
            (
                group.side.clone(),
                group.tracks.iter().map(|track| track.position_text.as_str()).collect(),
                group.total_duration_ms,
            )
        })
        .collect();
    let lettered = |side_letter: &str| TrackSide::Sided { side_letter: side_letter.to_string() };
    assert_eq!(
        groups,
        [
            (TrackSide::Disc { disc: 1 }, vec!["1", "2"], 300_000),
            (lettered("A"), vec!["A1"], 300_000),
            (lettered("B"), vec!["B1"], 400_000),
            (lettered("C"), vec!["C1"], 500_000),
        ]
    );
    assert!(detail.track_groups.iter().all(|group| group.shows_time));
    assert_eq!(detail.total_duration_ms, 1_500_000);
    assert_eq!(detail.credits.len(), 1);
    assert_eq!(detail.credits[0].tracks, ["A1", "B1"]);
}

const FILED_ALBUM: &str = "c9e3a7f4-5d61-4e8a-9f20-3b4c5d6e7f01";
const FILED_RELEASE: &str = "c9e3a7f4-5d61-4e8a-9f20-3b4c5d6e7f02";

/// A release's files come back with the lists its screens show beside them:
/// every image, the images a cover can be made from, the files no track
/// plays from, and their count and sizes summed, each file counted once.
#[tokio::test]
async fn a_release_lists_its_files_with_the_images_and_covers_among_them() {
    use bae_records::source_audio::{SourceAudioFile, SourceAudioLayout};
    use bae_records::ReleaseFileRole::{Artwork, Audio, Document, TrackSheet};
    use bae_util::content_type::ContentType;
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(SIDED_ENGINEER, "Artist Name")
        .album(FILED_ALBUM, "Album Title", None, &[SIDED_ENGINEER])
        .release(FILED_ALBUM, FILED_RELEASE)
        .write(&bae_records::test_fixtures::test_records(&db))
        .await;
    let source = |layout| SourceAudioFile {
        layout,
        format: bae_types::audio_format::AudioFormat {
            codec: "FLAC".to_string(),
            sample_rate_hz: 44_100,
            bits_per_sample: Some(16),
            bitrate_kbps: None,
            channels: 2,
        },
        content_type: ContentType::Flac,
        container: bae_util::audio_container::AudioContainer::from_name("flac"),
        duration_ms: 60_000,
    };
    for (index, (name, mime, role, layout, size)) in [
        ("disc.flac", "audio/flac", Audio, Some(Some(SourceAudioLayout::Cue)), 1_000),
        ("odd.flac", "audio/flac", Audio, Some(None), 2_000),
        ("disc.cue", "application/x-cue", TrackSheet, None, 3_000),
        ("rip.log", "text/plain", Document(bae_records::DocumentKind::RipLog), None, 4_000),
        ("cover.jpg", "image/jpeg", Artwork, None, 5_000),
        ("back.png", "image/png", Artwork, None, 6_000),
        ("scan.bmp", "image/bmp", Artwork, None, 7_000),
    ]
    .into_iter()
    .enumerate()
    {
        let mut file = NewReleaseFile::new(
            FILED_RELEASE,
            name,
            ContentType::from_mime(mime),
            role,
            bae_util::test_ids::test_uuid(&format!("filed-{index}")),
            chrono::Utc::now(),
        );
        file.source_audio = layout.map(source);
        insert_file(&db, tmp.path(), file, size).await;
    }

    let detail = crate::LibraryViews::new(db.clone())
        .find_release_detail(FILED_RELEASE)
        .await
        .unwrap()
        .unwrap();

    let names = |files: &[crate::album_detail::FileDetail]| -> Vec<String> {
        files.iter().map(|file| file.original_filename.clone()).collect()
    };
    assert_eq!(
        names(&detail.files),
        ["back.png", "cover.jpg", "disc.cue", "disc.flac", "odd.flac", "rip.log", "scan.bmp"]
    );
    assert_eq!(names(&detail.image_files), ["back.png", "cover.jpg", "scan.bmp"]);
    assert_eq!(names(&detail.cover_files), ["back.png", "cover.jpg"]);
    assert_eq!(
        names(&detail.other_files),
        ["back.png", "cover.jpg", "disc.cue", "odd.flac", "rip.log", "scan.bmp"]
    );
    assert_eq!(detail.summary.file_count, 7);
    assert_eq!(detail.summary.total_size, 28_000);
}

const ONE_GROUP_RELEASE: &str = "d0f4b8a5-6e72-4f9b-8a31-4c5d6e7f8a02";

/// A release of one group shows no time beside it, and a credit over every
/// track prints no tracks, however many credits add up to that, while a
/// qualifier keeps two credits of one role apart and one artist in two roles
/// is two rows.
#[tokio::test]
async fn a_release_of_one_group_shows_no_group_time_and_its_credits_by_qualifier() {
    use bae_types::credit_role::RoleQualifier;
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let source = bae_catalogs::catalog::Catalog::MusicBrainz;
    let mut assistant = CreditedAs::of(CreditRoleKind::Engineer);
    assistant.qualifiers = vec![RoleQualifier::Stated("assistant".to_string())];
    let ids = |n: u32| format!("d0f4b8a5-6e72-4f9b-8a31-4c5d6e7f8a{n:02}");
    RecordsFixture::new()
        .artist(&ids(10), "Mixer Name")
        .artist(&ids(11), "Engineer Name")
        .artist(&ids(12), "Assistant Name")
        .album(&ids(1), "Album Title", None, &[ids(10).as_str()])
        .release(&ids(1), ONE_GROUP_RELEASE)
        .tracks(ONE_GROUP_RELEASE, &[(&ids(20), "Track Title 1"), (&ids(21), "Track Title 2")])
        .track_role(&ids(30), &ids(20), &ids(10), (source, CreditedAs::of(CreditRoleKind::Mixer)))
        .track_role(&ids(31), &ids(21), &ids(10), (source, CreditedAs::of(CreditRoleKind::Mixer)))
        .release_role(
            &ids(32),
            ONE_GROUP_RELEASE,
            &ids(11),
            (source, CreditedAs::of(CreditRoleKind::Engineer)),
        )
        .release_role(&ids(33), ONE_GROUP_RELEASE, &ids(12), (source, assistant.clone()))
        .write(&bae_records::test_fixtures::test_records(&db))
        .await;

    let detail = crate::LibraryViews::new(db.clone())
        .find_release_detail(ONE_GROUP_RELEASE)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(detail.track_groups.len(), 1);
    assert!(!detail.track_groups[0].shows_time);
    let rows: Vec<(CreditedAs, Vec<&str>, Vec<String>)> = detail
        .credits
        .iter()
        .map(|row| {
            (
                row.role.clone(),
                row.artists.names().iter().map(|credited| credited.name.as_str()).collect(),
                row.tracks.clone(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (CreditedAs::of(CreditRoleKind::Engineer), vec!["Engineer Name"], vec![]),
            (assistant, vec!["Assistant Name"], vec![]),
            (CreditedAs::of(CreditRoleKind::Mixer), vec!["Mixer Name"], vec![]),
        ]
    );
}
