use super::*;
use std::future::Future;
use std::pin::{pin, Pin};
use std::task::{Context, Poll, Waker};

/// A stream and the writer its feed holds, the stream pulled on `runtime`.
fn fed_stream(
    runtime: &tokio::runtime::Runtime,
) -> (Arc<KeyedValueStream>, StreamWriter, Arc<StreamValues>) {
    let values = StreamValues::new();
    let stream = Arc::new(KeyedValueStream::new(
        values.clone(),
        CancellationToken::new(),
        OperationRuntime::new(runtime.handle().clone(), bae_diagnostics::Diagnostics::noop()),
    ));
    (stream, StreamWriter::new(values.clone()), values)
}

fn share(fraction: f64) -> BridgeFastValue {
    BridgeFastValue::FileShare { fraction }
}

/// Poll `pull` once, as a host's pull would be before anything wakes it.
fn poll_once<F: Future>(pull: Pin<&mut F>) -> Poll<F::Output> {
    pull.poll(&mut Context::from_waker(Waker::noop()))
}

/// What a pull answers now; `None` while it would wait.
fn pulled_now(values: &StreamValues) -> Option<Result<Vec<Vec<u8>>, BridgeError>> {
    match poll_once(pin!(values.next())) {
        Poll::Ready(pulled) => Some(pulled),
        Poll::Pending => None,
    }
}

fn keys(names: &[&str]) -> Option<Result<Vec<Vec<u8>>, BridgeError>> {
    Some(Ok(names.iter().map(|name| name.as_bytes().to_vec()).collect()))
}

/// A key moving holds its latest value and is not pulled; only a key
/// starting or stopping is.
#[test]
fn only_a_key_starting_or_stopping_is_pulled() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (stream, writer, values) = fed_stream(&runtime);

    writer.set("a", share(0.1));
    assert_eq!(pulled_now(&values), keys(&["a"]));
    writer.set("a", share(0.5));
    assert_eq!(stream.latest(b"a".to_vec()), Some(share(0.5)));
    assert_eq!(pulled_now(&values), None, "a value moving is not pulled");

    writer.remove(b"a");
    writer.remove(b"a");
    assert_eq!(stream.latest(b"a".to_vec()), None);
    assert_eq!(pulled_now(&values), keys(&["a"]));
}

/// Keys that start or stop between two pulls are pulled once each, in byte
/// order; one that came and went is pulled once, and `latest` says it holds
/// nothing.
#[test]
fn the_keys_changed_since_the_last_pull_come_in_one_pull() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (stream, writer, values) = fed_stream(&runtime);

    writer.set("b", share(0.1));
    writer.set("a", share(0.2));
    writer.remove(b"b");

    assert_eq!(pulled_now(&values), keys(&["a", "b"]));
    assert_eq!(stream.latest(b"b".to_vec()), None);
    assert_eq!(pulled_now(&values), None);
}

/// A reset holds exactly the keys it names, and the pull names the ones that
/// started or stopped, not the ones that only moved.
#[test]
fn a_reset_pulls_the_keys_that_came_and_went() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (stream, writer, values) = fed_stream(&runtime);
    writer.reset(HashMap::from([(b"a".to_vec(), share(0.1)), (b"b".to_vec(), share(0.2))]));
    pulled_now(&values);

    writer.reset(HashMap::from([(b"b".to_vec(), share(0.3)), (b"c".to_vec(), share(0.4))]));

    assert_eq!(stream.latest(b"a".to_vec()), None);
    assert_eq!(stream.latest(b"b".to_vec()), Some(share(0.3)));
    assert_eq!(stream.latest(b"c".to_vec()), Some(share(0.4)));
    assert_eq!(pulled_now(&values), keys(&["a", "c"]));
}

/// A pull waiting for a key answers once one starts.
#[test]
fn a_waiting_pull_answers_when_a_key_starts() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (_stream, writer, values) = fed_stream(&runtime);
    let mut pull = pin!(values.next());
    assert!(poll_once(pull.as_mut()).is_pending());

    writer.set("a", share(0.5));

    assert_eq!(poll_once(pull.as_mut()), Poll::Ready(Ok(vec![b"a".to_vec()])));
}

/// The host's pull runs on the stream's runtime and answers what changed.
#[test]
fn the_host_pulls_on_the_streams_runtime() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (stream, writer, _values) = fed_stream(&runtime);
    writer.set("a", share(0.5));

    let pulled = runtime.block_on(stream.clone().next());

    assert_eq!(pulled, Ok(vec![b"a".to_vec()]));
}

/// Once its feed ends the stream holds nothing, and the keys it held are
/// pulled.
#[test]
fn a_stream_empties_when_its_feed_ends() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (stream, writer, values) = fed_stream(&runtime);
    writer.set("a", share(1.0));
    pulled_now(&values);

    drop(writer);

    assert_eq!(stream.latest(b"a".to_vec()), None);
    assert_eq!(pulled_now(&values), keys(&["a"]));
}

/// A failed read empties the stream; its keys are pulled, then the failure,
/// once; the next read fills it again.
#[test]
fn a_failed_read_is_pulled_after_the_keys_it_emptied() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (stream, writer, values) = fed_stream(&runtime);
    writer.set("a", share(0.5));
    pulled_now(&values);

    writer.fail(BridgeError::internal("the read failed"));

    assert_eq!(stream.latest(b"a".to_vec()), None);
    assert_eq!(pulled_now(&values), keys(&["a"]));
    assert_eq!(pulled_now(&values), Some(Err(BridgeError::internal("the read failed"))));
    assert_eq!(pulled_now(&values), None, "a failure is pulled once");

    writer.set("b", share(0.1));
    assert_eq!(pulled_now(&values), keys(&["b"]));
}

/// Cancelling or dropping the stream stops its feed.
#[test]
fn cancelling_or_dropping_a_stream_stops_its_feed() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (stream, _writer, _values) = fed_stream(&runtime);
    let stop = stream.stop.clone();
    stream.cancel();
    assert!(stop.is_cancelled());

    let (stream, _writer, _values) = fed_stream(&runtime);
    let stop = stream.stop.clone();
    drop(stream);
    assert!(stop.is_cancelled());
}

/// Cancelling a stream ends its feed: it holds nothing, the keys that held a
/// value are pulled, every pull after answers that it was cancelled, and it
/// takes nothing its writer writes or fails after.
#[test]
fn a_cancelled_stream_holds_nothing_and_takes_no_write() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (stream, writer, values) = fed_stream(&runtime);
    writer.set("a", share(0.5));
    pulled_now(&values);

    stream.cancel();
    assert_eq!(stream.latest(b"a".to_vec()), None);
    assert_eq!(pulled_now(&values), keys(&["a"]));
    assert_eq!(pulled_now(&values), Some(Err(BridgeError::Cancelled)));

    writer.set("b", share(0.1));
    writer.fail(BridgeError::internal("after the cancel"));
    drop(writer);
    assert_eq!(stream.latest(b"b".to_vec()), None);
    assert_eq!(pulled_now(&values), Some(Err(BridgeError::Cancelled)));
}

/// A key at rest is not read every frame, so the pull names it when it starts
/// or stops moving and whenever its value changes; a moving key's value
/// changing is not named.
#[test]
fn a_key_at_rest_is_pulled_when_it_flips_or_its_value_changes() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (stream, writer, values) = fed_stream(&runtime);

    let rests = |value| writer.reset_resting(HashMap::from([(b"a".to_vec(), value)]), |_, _| true);
    rests(share(0.1));
    assert!(!stream.moving(b"a".to_vec()));
    assert_eq!(pulled_now(&values), keys(&["a"]));
    rests(share(0.1));
    assert_eq!(pulled_now(&values), None, "the same value at rest tells nothing");

    rests(share(0.2));
    assert_eq!(pulled_now(&values), keys(&["a"]), "a seek while at rest is pulled");

    writer.set("a", share(0.3));
    assert!(stream.moving(b"a".to_vec()));
    assert_eq!(pulled_now(&values), keys(&["a"]), "starting to move is pulled");
    writer.set("a", share(0.4));
    assert_eq!(pulled_now(&values), None, "a moving value is not pulled");

    rests(share(0.5));
    assert!(!stream.moving(b"a".to_vec()));
    assert_eq!(pulled_now(&values), keys(&["a"]), "coming to rest is pulled");
    assert!(!stream.moving(b"missing".to_vec()));
}

fn download(bytes_done: u64) -> BridgeFastValue {
    BridgeFastValue::Download {
        progress: crate::types::BridgeDownloadTransferProgress {
            bytes_done,
            bytes_total: 1000,
            fraction: bytes_done as f64 / 1000.0,
            throughput_bps: None,
        },
    }
}

/// A value the feed says only when it changes (a download's bytes) rests once
/// it has gone [`QUIET`] unsaid, and moves again when it changes. A view is
/// told of each flip, and of a change while at rest.
#[test]
fn a_value_unchanged_for_the_quiet_time_rests_and_moves_again_when_it_changes() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (stream, writer, values) = fed_stream(&runtime);
    let start = Instant::now();

    writer.set_at("a", download(100), start);
    assert!(stream.moving(b"a".to_vec()));
    assert_eq!(pulled_now(&values), keys(&["a"]));

    // Not quiet long enough: still moving, and the next rest is due then.
    let almost = start + QUIET - Duration::from_millis(1);
    assert_eq!(writer.rest_quiet_at(almost), Some(start + QUIET));
    assert!(stream.moving(b"a".to_vec()));
    assert_eq!(pulled_now(&values), None);

    // A paused download: nothing said for the quiet time.
    assert_eq!(writer.rest_quiet_at(start + QUIET), None);
    assert!(!stream.moving(b"a".to_vec()));
    assert_eq!(pulled_now(&values), keys(&["a"]), "coming to rest is pulled");
    assert_eq!(stream.latest(b"a".to_vec()), Some(download(100)));

    // Saying the same value again changes nothing.
    writer.set_at("a", download(100), start + QUIET * 2);
    assert!(!stream.moving(b"a".to_vec()));
    assert_eq!(pulled_now(&values), None);

    // Bytes move again.
    writer.set_at("a", download(200), start + QUIET * 3);
    assert!(stream.moving(b"a".to_vec()));
    assert_eq!(pulled_now(&values), keys(&["a"]), "starting to move is pulled");
    assert_eq!(writer.rest_quiet_at(start + QUIET * 4), None);
    assert!(!stream.moving(b"a".to_vec()));
}

/// Each key rests on its own clock: one that keeps moving does not hold a
/// stalled one awake, and a reset that repeats a key's value leaves it be.
#[test]
fn each_key_rests_on_its_own_clock() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (stream, writer, values) = fed_stream(&runtime);
    let start = Instant::now();
    let half = QUIET / 2;

    writer.reset_at(
        HashMap::from([(b"stalled".to_vec(), share(0.1)), (b"going".to_vec(), share(0.1))]),
        start,
    );
    assert_eq!(pulled_now(&values), keys(&["going", "stalled"]));
    writer.reset_at(
        HashMap::from([(b"stalled".to_vec(), share(0.1)), (b"going".to_vec(), share(0.2))]),
        start + half,
    );
    assert_eq!(pulled_now(&values), None);

    assert_eq!(writer.rest_quiet_at(start + QUIET), Some(start + half + QUIET));
    assert!(!stream.moving(b"stalled".to_vec()));
    assert!(stream.moving(b"going".to_vec()));
    assert_eq!(pulled_now(&values), keys(&["stalled"]));

    assert_eq!(writer.rest_quiet_at(start + half + QUIET), None);
    assert!(!stream.moving(b"going".to_vec()));
    assert_eq!(pulled_now(&values), keys(&["going"]));
}

/// The frames a view following a paused download asks for: FrameFollowing
/// requests one per 60 Hz frame while the key is moving and none at rest.
/// A download that stops at 1 s and stays paused for 10 s asks for every
/// frame of those 10 s with the key always moving, and for the frames of the
/// quiet time alone once it rests.
#[test]
fn a_paused_download_asks_for_frames_only_for_the_quiet_time() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (stream, writer, _values) = fed_stream(&runtime);
    let start = Instant::now();
    let frame = Duration::from_micros(16_667);

    writer.set_at("a", download(500), start);
    let mut requested = 0;
    for n in 0..600 {
        let now = start + frame * n;
        writer.rest_quiet_at(now);
        if stream.moving(b"a".to_vec()) {
            requested += 1;
        }
    }
    assert_eq!(requested, 60, "600 frames over 10 s; only the first second asks");
}
