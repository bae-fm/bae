use super::*;
use crate::db::{DbScrubPoint, ScrubPoint};

/// Scrub points sync with their segment: another device playing the track
/// from storage resolves the same points, so its scrubs jump the same way.
#[test]
fn scrub_points_reach_another_device_with_their_segment() {
    crate::sync::test_devices::run_two_device_test(check_scrub_points_sync);
}

async fn check_scrub_points_sync(devices: &crate::sync::test_devices::TwoDevices) {
    let manager = devices.a().manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_1, "Track Title", 1);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    manager.database.insert_track(&track).await.unwrap();
    add_track_audio_sources(
        manager,
        &track,
        &[(
            "Album Title.flac",
            crate::album_detail::SourceAudioLayout::File,
        )],
    )
    .await;
    let format = manager
        .database
        .find_audio_format_by_track_id(&track.id)
        .await
        .unwrap()
        .expect("the track's audio format");
    let segment = manager
        .database
        .get_audio_segments_for_format(&format.id)
        .await
        .unwrap()
        .remove(0);
    let points = [
        ScrubPoint {
            sample: 0,
            byte: 8_288,
        },
        ScrubPoint {
            sample: 442_368,
            byte: 1_760_512,
        },
        ScrubPoint {
            sample: 884_736,
            byte: 3_512_840,
        },
    ];
    manager
        .database
        .insert_scrub_points_for_test(
            &points
                .iter()
                .map(|point| DbScrubPoint {
                    audio_segment_id: segment.id.clone(),
                    point: *point,
                })
                .collect::<Vec<_>>(),
        )
        .await
        .unwrap();

    devices
        .converge(
            "SELECT audio_format_segment_id || ' ' || sample || ' ' || byte \
             FROM audio_segment_scrub_points",
        )
        .await;
    let resolved = devices
        .b()
        .manager()
        .resolve_track_audio(&track.id)
        .await
        .unwrap();
    assert_eq!(resolved.segments[0].scrub_points.as_slice(), points);
}
