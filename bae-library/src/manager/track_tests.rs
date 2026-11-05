//! Tracks and the queue: the library's track order, the queue's lanes, a
//! track's display and audio on another device, and resolving ids to tracks.

use super::*;
use crate::manager::tests::*;
use bae_records::LibraryImageType;
use bae_records::{DbScrubPoints, ScrubPoint};
use chrono::Utc;

/// A track's display carries the cover reference the UI caches its art under,
/// so it has to carry the `covers` row's version, not just the release id — and
/// it has to be absent when the release has no cover at all, rather than naming
/// a row that isn't there.
#[tokio::test]
async fn track_display_carries_the_cover_version() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = bae_records::DbTrack::new_test(&release.id, TRACK_A, "Track Title", 1);
    let track_artist = bae_records::DbTrackArtist::new(
        &track.id,
        &bae_util::test_ids::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        0,
        Utc::now(),
    );
    manager.records.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager.records.insert_track(&track).await.unwrap();
    manager.records.insert_track_artist(&track_artist).await.unwrap();

    // No cover row yet: nothing to reference.
    let display = manager.get_track_display(&track.id).await.unwrap();
    assert_eq!(display.cover_image, None);

    store_test_cover(&manager, &release.id, b"cover").await;
    let version = manager
        .records
        .cover_version(&release.id)
        .await
        .unwrap()
        .expect("the stored cover has a version");
    let display = manager.get_track_display(&track.id).await.unwrap();
    assert_eq!(
        display.cover_image,
        Some(bae_records::ImageRef {
            id: release.id.clone(),
            version,
            image_type: LibraryImageType::Cover,
        })
    );
}

/// Seed an album with two releases, each holding two tracks inserted in
/// playing order, so the library order is deterministic.
async fn seed_two_release_library(manager: &LibraryManager) -> (String, String) {
    use bae_records::DbTrack;
    let mut album = create_test_album();
    album.id = "1250a7bb-41ed-4500-8ab4-04f5d3461e30".to_string();
    let mut rel1 = create_test_release(&album.id);
    rel1.id = REL_1.to_string();
    let mut rel2 = create_test_release(&album.id);
    rel2.id = REL_2.to_string();
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &rel1).await;
    insert_release(manager, &rel2).await;

    let track = |release_id: &str, id: &str, side: u8, number: u32| {
        let t = DbTrack {
            side: std::num::NonZeroU8::new(side).expect("sides count from one"),
            ..DbTrack::new_test(release_id, id, "Track Title", number)
        };
        let records = &manager.records;
        async move { records.insert_track(&t).await.unwrap() }
    };
    // rel-1: side 1 then side 2; rel-2: two side-1 tracks.
    track(REL_1, "48ae00a1-d7a5-443c-8240-f999fc4ddfcc", 1, 1).await;
    track(REL_1, "48ae03a1-d7a5-4955-8240-fc99fc4de4e5", 2, 1).await;
    track(REL_2, "cc4180bc-58f5-456f-8116-f9b2099f5b7f", 1, 1).await;
    track(REL_2, "cc4181bc-58f5-4722-8116-fab2099f5d32", 1, 2).await;
    (rel1.id, rel2.id)
}

/// `get_all_track_ids` returns every library track in the deterministic base
/// order — by release, then side, track number, id.
#[tokio::test]
async fn test_get_all_track_ids_returns_library_in_base_order() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_two_release_library(&manager).await;
    let all = manager.get_all_track_ids().await.unwrap();
    assert_eq!(
        all,
        vec![
            "48ae00a1-d7a5-443c-8240-f999fc4ddfcc",
            "48ae03a1-d7a5-4955-8240-fc99fc4de4e5",
            "cc4180bc-58f5-456f-8116-f9b2099f5b7f",
            "cc4181bc-58f5-4722-8116-fab2099f5d32"
        ]
    );
}

/// A library play's lane holds every library track; unshuffled, the tracks
/// after the playing one are in the library's own order — by release, then
/// position — the order a release plays its own tracks in.
#[tokio::test]
async fn test_library_play_unshuffles_into_the_library_order() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let (rel1, _rel2) = seed_two_release_library(&manager).await;
    let tables = bae_playback::tables::PlaybackQueueTables::new(manager.store.clone());
    let library_order = [
        "48ae00a1-d7a5-443c-8240-f999fc4ddfcc",
        "48ae03a1-d7a5-4955-8240-fc99fc4de4e5",
        "cc4180bc-58f5-456f-8116-f9b2099f5b7f",
        "cc4181bc-58f5-4722-8116-fab2099f5d32",
    ];

    tables
        .fill(bae_playback::tables::ContextFill::Release {
            release_id: rel1,
            start: bae_playback::tables::ReleaseStart::First,
        })
        .await
        .unwrap();
    assert_eq!(tables.context_order().await.unwrap(), library_order[..2]);

    tables.fill(bae_playback::tables::ContextFill::LibraryShuffled).await.unwrap();
    tables.set_shuffle(false).await.unwrap();
    let lane = tables.context_order().await.unwrap();
    let playing = lane[0].clone();
    let rest: Vec<&str> = library_order.iter().copied().filter(|t| *t != playing).collect();
    assert_eq!(lane[1..], rest[..]);
}

// ── Queue windowing tests ───────────────────────────────────────────

/// Insert one release with `count` sequentially-numbered tracks
/// (`track-0`..`track-{count-1}`); return their ids in track order.
async fn seed_release_tracks(manager: &LibraryManager, count: usize) -> Vec<String> {
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    let mut track_ids = Vec::with_capacity(count);
    for i in 0..count {
        let track_id = bae_util::test_ids::test_uuid(&format!("track-{i}"));
        let track = bae_records::DbTrack::new_test(
            &release.id,
            &track_id,
            &format!("Track Title {i}"),
            u32::try_from(i + 1).expect("a test track number fits u32"),
        );
        manager.records.insert_track(&track).await.unwrap();
        track_ids.push(track_id);
    }
    track_ids
}

/// Play the seeded release from its first track.
async fn play_seeded(manager: &LibraryManager, first_track: &str) {
    let tables = bae_playback::tables::PlaybackQueueTables::new(manager.store.clone());
    let release_id =
        manager.records.find_track_by_id(first_track).await.unwrap().unwrap().release_id;
    tables
        .fill(bae_playback::tables::ContextFill::Release {
            release_id,
            start: bae_playback::tables::ReleaseStart::First,
        })
        .await
        .unwrap();
}

/// The context lane's tail reads in order after the playing track, however
/// long it is; the queue value says only that it has one.
#[tokio::test]
async fn the_context_tail_reads_in_order_after_the_playing_track() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let track_ids = seed_release_tracks(&manager, 151).await;
    play_seeded(&manager, &track_ids[0]).await;

    let value = bae_playback::tables::queue_value(manager.store()).await.unwrap();
    assert!(value.context.is_some() && value.has_next);
    let upcoming: Vec<String> = bae_playback::tables::queue_upcoming(manager.store())
        .await
        .unwrap()
        .into_iter()
        .map(|item| item.track_id)
        .collect();
    assert_eq!(upcoming, track_ids[1..].to_vec());
}

/// The manual lane is explicit and user-curated, not library-scaled — the
/// queue value carries it in full.
#[tokio::test]
async fn queue_value_resolves_manual_lane_in_full_regardless_of_window() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let track_ids = seed_release_tracks(&manager, 110).await;
    let manual_count = 103;
    bae_playback::tables::PlaybackQueueTables::new(manager.store.clone())
        .add_manual(track_ids[..manual_count].to_vec(), bae_playback::tables::ManualPlace::End)
        .await
        .unwrap();

    assert_eq!(
        bae_playback::tables::queue_value(manager.store()).await.unwrap().manual.len(),
        manual_count,
        "the manual lane is never windowed"
    );
}

/// A reordered Up Next shows the new order with each entry in its place, a
/// repeated track once per entry.
#[tokio::test]
async fn a_reordered_queue_shows_each_entry_in_its_new_place() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let track_ids = seed_release_tracks(&manager, 3).await;
    let tables = bae_playback::tables::PlaybackQueueTables::new(manager.store.clone());
    let mut queued = track_ids.clone();
    queued.push(track_ids[0].clone());
    tables.add_manual(queued, bae_playback::tables::ManualPlace::End).await.unwrap();
    let entries: Vec<String> = bae_playback::tables::queue_value(manager.store())
        .await
        .unwrap()
        .manual
        .iter()
        .map(|item| item.entry_id.clone())
        .collect();
    let id = |index: usize| entries[index].parse::<bae_playback::QueueEntryId>().unwrap();
    tables.reorder(id(2), Some(id(0))).await.unwrap();

    let shown: Vec<(String, String)> = bae_playback::tables::queue_value(manager.store())
        .await
        .unwrap()
        .manual
        .into_iter()
        .map(|item| (item.entry_id, item.track_id))
        .collect();
    assert_eq!(
        shown,
        vec![
            (entries[2].clone(), track_ids[2].clone()),
            (entries[0].clone(), track_ids[0].clone()),
            (entries[1].clone(), track_ids[1].clone()),
            (entries[3].clone(), track_ids[0].clone()),
        ]
    );
}

/// Queueing an album expands to its PRIMARY release's tracks, not the
/// earliest-imported one. When the user picks a non-default primary (e.g. a
/// later remaster over the original vinyl rip), enqueueing the album must
/// play the chosen release.
#[tokio::test]
async fn resolve_to_track_ids_expands_album_to_primary_release() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    // release1 is the album's first release; release2 is then made primary.
    let mut release1 = create_test_release(&album.id);
    release1.created_at = Utc::now() - chrono::Duration::days(1);
    let release2 = create_test_release(&album.id);

    manager.records.insert_album(&album).await.unwrap();
    insert_release(&manager, &release1).await;
    insert_release(&manager, &release2).await;

    let old = bae_records::DbTrack::new_test(
        &release1.id,
        "48ae00a1-d7a5-443c-8240-f999fc4ddfcc",
        "Old A",
        1,
    );
    let p1 = bae_records::DbTrack::new_test(
        &release2.id,
        "cc4180bc-58f5-456f-8116-f9b2099f5b7f",
        "New A",
        1,
    );
    let p2 = bae_records::DbTrack::new_test(
        &release2.id,
        "cc4181bc-58f5-4722-8116-fab2099f5d32",
        "New B",
        2,
    );
    manager.records.insert_track(&old).await.unwrap();
    manager.records.insert_track(&p1).await.unwrap();
    manager.records.insert_track(&p2).await.unwrap();

    manager.set_album_primary_release(&album.id, &release2.id).await.unwrap();

    let resolved = manager.resolve_to_track_ids(std::slice::from_ref(&album.id)).await.unwrap();
    assert!(resolved.contains(&"cc4180bc-58f5-456f-8116-f9b2099f5b7f".to_string()));
    assert!(resolved.contains(&"cc4181bc-58f5-4722-8116-fab2099f5d32".to_string()));
    assert!(
        !resolved.contains(&"48ae00a1-d7a5-443c-8240-f999fc4ddfcc".to_string()),
        "must not expand to the non-primary release's tracks"
    );
    assert_eq!(resolved.len(), 2);
}

#[tokio::test]
async fn resolve_to_track_ids_rejects_unknown_id() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let err = manager.resolve_to_track_ids(&["missing-id".to_string()]).await.unwrap_err();
    assert!(
        matches!(err, LibraryError::TrackMapping(message) if message.contains("missing-id")),
        "unknown ids must fail instead of being treated as track ids"
    );
}

/// Scrub points sync with their segment: another device playing the track
/// from storage resolves the same points, so its scrubs jump the same way.
#[test]
fn scrub_points_reach_another_device_with_their_segment() {
    crate::test_libraries::run_two_device_test(check_scrub_points_sync);
}

async fn check_scrub_points_sync(devices: &mut crate::test_libraries::TwoDevices) {
    let manager = devices.a().manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = bae_records::DbTrack::new_test(&release.id, TRACK_1, "Track Title", 1);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    manager.records.insert_track(&track).await.unwrap();
    add_track_audio_sources(
        manager,
        &track,
        &[("Album Title.flac", bae_records::source_audio::SourceAudioLayout::File)],
    )
    .await;
    let format = manager
        .records
        .find_audio_format_by_track_id(&track.id)
        .await
        .unwrap()
        .expect("the track's audio format");
    let segment =
        manager.records.get_audio_segments_for_format(&format.id).await.unwrap().remove(0);
    let points = [
        ScrubPoint { sample: 0, byte: 8_288, exact_from: 0 },
        ScrubPoint { sample: 442_368, byte: 1_760_512, exact_from: 442_368 },
        ScrubPoint { sample: 884_736, byte: 3_512_840, exact_from: 884_736 },
    ];
    manager
        .records
        .insert_scrub_points_for_test(&DbScrubPoints {
            audio_segment_id: segment.id.clone(),
            points: points.to_vec(),
        })
        .await
        .unwrap();

    devices
        .converge(
            "SELECT audio_format_segment_id || ' ' || hex(points) \
             FROM audio_segment_scrub_points",
        )
        .await;
    let resolved = devices.b().manager().resolve_track_audio(&track.id).await.unwrap();
    assert_eq!(resolved.segments[0].scrub_points.as_slice(), points);
}

const REL_2: &str = "dcdebf05-2d41-4dfc-8823-024993c9d00f"; // was "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b"
