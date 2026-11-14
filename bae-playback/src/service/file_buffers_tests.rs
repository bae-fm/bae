use super::*;
use crate::service::tests::*;

/// A track whose audio reads from two files — its pregap at the end of the
/// file before — fetches from both as the playing track, so a preload yields
/// to it in either.
#[tokio::test]
async fn every_file_the_playing_track_reads_fetches_as_the_playing_one() {
    let arbiter = FetchArbiter::new();
    let buffers = FileBuffers::new(
        tokio::runtime::Handle::current(),
        arbiter.clone(),
        tokio::sync::watch::channel(None).0,
        bae_store::FileLeases::for_test_default(),
    );
    let pregap = create_sparse_buffer(1_024);
    let main = create_sparse_buffer(1_024);
    let mut prepared = test_prepared_track_with_file("track", "main-file", main.clone());
    prepared.segments.insert(
        0,
        PreparedAudioSegment {
            role: DbAudioSegmentRole::AudioPregap,
            file_id: "pregap-file".to_string(),
            buffer: pregap.clone(),
            span: bae_records::SegmentSpan::whole_file(),
            scrub_points: bae_records::ScrubPoints::new(Vec::new()),
        },
    );

    buffers.mark_foreground(&prepared);

    assert!(
        arbiter.fetches_as_playing(pregap.id()),
        "the pregap's file fetches as the playing one"
    );
    assert!(arbiter.fetches_as_playing(main.id()), "the main file fetches as the playing one");
}
