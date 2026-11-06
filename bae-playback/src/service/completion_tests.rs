use super::*;
use crate::service::tests::*;

/// A track's natural completion ships `track_completed` carrying the decode-error
/// count — the quality signal.
#[tokio::test]
async fn track_completion_ships_track_completed_with_decode_error_count() {
    let (diagnostics, transport) = recording_diagnostics();
    let store = seeded_store(&[(
        "c61a9e19-f3ba-4728-842c-c59dbc82e238",
        &["08c80007-b56a-4fc9-8df6-af2967fa09b9"],
    )])
    .await;
    let (mut service, _progress_rx) = playback_service_over(store.store(), diagnostics.clone());

    // A track must be active for the completion to mark its phase.
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c80007-b56a-4fc9-8df6-af2967fa09b9", buffer),
        TrackPhase::Playing,
    );

    service
        .handle_completion_event(
            Arc::new(TrackFmt::for_test("08c80007-b56a-4fc9-8df6-af2967fa09b9")),
            2,
            44_100,
        )
        .await;

    diagnostics.flush().await.expect("flush succeeds");
    let bodies = transport.requests();
    let events: Vec<bae_diagnostics::DiagnosticEvent> = bodies
        .iter()
        .flat_map(|r| {
            serde_json::from_slice::<Vec<bae_diagnostics::DiagnosticEvent>>(&r.body).unwrap()
        })
        .collect();
    let completed = events
        .iter()
        .find(|e| e.name == "track_completed")
        .expect("track completion ships track_completed");
    assert_eq!(completed.fields["decode_errors"], serde_json::json!(2));
    assert_eq!(
        completed.fields["track_id"],
        serde_json::json!("08c80007-b56a-4fc9-8df6-af2967fa09b9")
    );
}

/// A track that drained before its `TrackReady` was handled was heard, so it
/// reports Playing before it completes.
#[tokio::test]
async fn a_completion_before_its_load_resolves_reports_the_track_playing() {
    let (_home, mut service, mut progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;

    let generation = service.next_load_generation();
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track(FINISHED_TRACK, buffer),
        TrackPhase::Loading { generation, target: PlayTarget::Playing },
    );

    service.handle_completion_event(Arc::new(TrackFmt::for_test(FINISHED_TRACK)), 0, 44_100).await;

    let states = drained_states(&mut progress_rx);
    assert!(
        matches!(
            states.first(),
            Some(PlaybackState::Playing { track }) if track.track_id == FINISHED_TRACK
        ),
        "the drained track reports Playing first, got {states:?}"
    );
}
