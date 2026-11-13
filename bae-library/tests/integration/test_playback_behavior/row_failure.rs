use crate::test_playback_behavior::support::*;
use bae_playback::{PlaybackProgress, PlaybackState};
use bae_util::test_channels::HANG_GUARD;

/// A write of `playback_now` that fails while a track plays stops the track,
/// as any playback failure does, so the row, the audio and the screens agree:
/// whoever follows playback hears the failure and then the stop. Sabotage:
/// forward the failure to the subscribers but not to the service's own
/// follower, and the track plays on with nothing stopping it.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_write_of_the_playback_row_stops_the_track() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let track = fixture.track_ids[0].clone();
        play_and_wait_on(&fixture.playback_handle, &mut fixture.progress_rx, &track).await;
        bae_playback::tables::lose_playback_level_for_test(fixture._temp_dir.path());

        fixture.playback_handle.set_volume(0.5);

        let mut heard = Vec::new();
        let stopped =
            bae_util::test_channels::next_matching(&mut fixture.progress_rx, HANG_GUARD, |event| {
                let stopped = matches!(
                    event,
                    PlaybackProgress::StateChanged { state: PlaybackState::Stopped }
                );
                heard.push(event);
                stopped.then_some(())
            })
            .await;
        assert!(stopped.is_some(), "the track plays on: {heard:?}");
        assert!(
            heard.iter().any(|event| matches!(
                event,
                PlaybackProgress::PlaybackError {
                    reason: bae_playback::PlaybackErrorReason::Diagnostic {
                        failure: bae_playback::PlaybackDiagnostic::Internal,
                        ..
                    }
                }
            )),
            "{heard:?}"
        );
    })
    .await;
}
