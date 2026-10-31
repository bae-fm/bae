use super::*;
use crate::diagnostics::RecordingBugReporter;
use bae_audio::sparse_buffer::{create_sparse_buffer, BufferStop};

async fn fill_that_panics() -> Result<(), FillFailure> {
    panic!("window index out of range")
}

/// A fill that panics fails its buffer, so a reader stops on the failure
/// instead of waiting for bytes, hands the failure to its owner as any read
/// failure is, and is reported as a bug. Sabotage: drop the panic arm in
/// `spawn_fill`, and the reader blocks forever with nothing reported.
#[tokio::test]
async fn a_fill_that_panics_fails_its_buffer_and_is_reported() {
    let bugs = Arc::new(RecordingBugReporter::default());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let buffer = create_sparse_buffer(1_024);
    let (error_tx, error_rx) = tokio::sync::oneshot::channel();

    let fill = spawn_fill(
        diagnostics,
        &buffer,
        Box::new(move |error| {
            error_tx
                .send(error)
                .expect("the test waits for the fill's error");
        }),
        fill_that_panics(),
    );
    fill.ended().await;

    let error = error_rx.await.expect("the fill's owner hears of it");
    assert!(
        matches!(*error, PlaybackError::TaskFailed(ref text) if text.contains("window index out of range")),
        "{error}"
    );
    assert!(matches!(
        buffer.new_reader().read(&mut [0u8; 1]),
        Err(BufferStop::Failed(_))
    ));
    assert_eq!(
        bugs.reports(),
        vec![(
            "fill a playback buffer".to_string(),
            "fill a playback buffer panicked: window index out of range".to_string()
        )]
    );
}
