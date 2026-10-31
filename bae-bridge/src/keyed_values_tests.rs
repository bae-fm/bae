use super::*;

/// Every batch of keys the stream says started or stopped.
#[derive(Default)]
struct HeardKeys(Mutex<Vec<Vec<Vec<u8>>>>);

struct Activity(Arc<HeardKeys>);

impl KeyActivityCallback for Activity {
    fn on_keys_changed(&self, keys: Vec<Vec<u8>>) {
        self.0 .0.lock().unwrap().push(keys);
    }
}

/// A stream, the writer its feed holds, and what its activity heard.
fn fed_stream() -> (KeyedValueStream, StreamWriter, Arc<HeardKeys>) {
    let heard = Arc::new(HeardKeys::default());
    let values = StreamValues::new(Box::new(Activity(heard.clone())), Diagnostics::noop());
    let stream = KeyedValueStream::new(values.clone(), CancellationToken::new());
    (stream, StreamWriter::new(values), heard)
}

fn share(fraction: f64) -> BridgeFastValue {
    BridgeFastValue::FileShare { fraction }
}

fn heard(keys: &HeardKeys) -> Vec<Vec<Vec<u8>>> {
    std::mem::take(&mut keys.0.lock().unwrap())
}

/// A key moving holds its latest value and tells nothing; only a key
/// starting or stopping is told.
#[test]
fn only_a_key_starting_or_stopping_is_told() {
    let (stream, writer, keys) = fed_stream();

    writer.set("a", share(0.1));
    writer.set("a", share(0.5));
    assert_eq!(stream.latest(b"a".to_vec()), Some(share(0.5)));
    assert_eq!(heard(&keys), vec![vec![b"a".to_vec()]]);

    writer.remove(b"a");
    writer.remove(b"a");
    assert_eq!(stream.latest(b"a".to_vec()), None);
    assert_eq!(heard(&keys), vec![vec![b"a".to_vec()]]);
}

/// A reset holds exactly the keys it names, and tells the ones that started
/// or stopped, not the ones that only moved.
#[test]
fn a_reset_tells_the_keys_that_came_and_went() {
    let (stream, writer, keys) = fed_stream();
    writer.reset(HashMap::from([
        (b"a".to_vec(), share(0.1)),
        (b"b".to_vec(), share(0.2)),
    ]));
    heard(&keys);

    writer.reset(HashMap::from([
        (b"b".to_vec(), share(0.3)),
        (b"c".to_vec(), share(0.4)),
    ]));

    assert_eq!(stream.latest(b"a".to_vec()), None);
    assert_eq!(stream.latest(b"b".to_vec()), Some(share(0.3)));
    assert_eq!(stream.latest(b"c".to_vec()), Some(share(0.4)));
    assert_eq!(heard(&keys), vec![vec![b"a".to_vec(), b"c".to_vec()]]);
}

/// Once its feed ends the stream holds nothing, and says so.
#[test]
fn a_stream_empties_when_its_feed_ends() {
    let (stream, writer, keys) = fed_stream();
    writer.set("a", share(1.0));
    heard(&keys);

    drop(writer);

    assert_eq!(stream.latest(b"a".to_vec()), None);
    assert_eq!(heard(&keys), vec![vec![b"a".to_vec()]]);
}

/// Cancelling or dropping the stream stops its feed.
#[test]
fn cancelling_or_dropping_a_stream_stops_its_feed() {
    let (stream, _writer, _keys) = fed_stream();
    let stop = stream.stop.clone();
    stream.cancel();
    assert!(stop.is_cancelled());

    let (stream, _writer, _keys) = fed_stream();
    let stop = stream.stop.clone();
    drop(stream);
    assert!(stop.is_cancelled());
}

/// Cancelling a stream ends its feed: it holds nothing, says which keys
/// stopped, and takes nothing its writer writes after.
#[test]
fn a_cancelled_stream_holds_nothing_and_takes_no_write() {
    let (stream, writer, keys) = fed_stream();
    writer.set("a", share(0.5));
    heard(&keys);

    stream.cancel();
    assert_eq!(stream.latest(b"a".to_vec()), None);
    assert_eq!(heard(&keys), vec![vec![b"a".to_vec()]]);

    writer.set("b", share(0.1));
    drop(writer);
    assert_eq!(stream.latest(b"b".to_vec()), None);
    assert!(heard(&keys).is_empty());
}

/// Tells the host nothing: its callback throws, which uniffi raises in Rust
/// as a panic.
struct ThrowingActivity;

impl KeyActivityCallback for ThrowingActivity {
    fn on_keys_changed(&self, _keys: Vec<Vec<u8>>) {
        panic!("Callback interface failure: the host threw");
    }
}

/// A host whose callback throws is reported, and the stream goes on holding
/// what its feed writes.
#[test]
fn a_host_callback_that_throws_is_reported_and_the_stream_keeps_its_values() {
    let bugs = Arc::new(bae_core::diagnostics::RecordingBugReporter::default());
    let values = StreamValues::new(
        Box::new(ThrowingActivity),
        Diagnostics::noop().reporting_bugs_to(bugs.clone()),
    );
    let stream = KeyedValueStream::new(values.clone(), CancellationToken::new());
    let writer = StreamWriter::new(values);

    writer.set("a", share(0.5));

    assert_eq!(stream.latest(b"a".to_vec()), Some(share(0.5)));
    let reports = bugs.reports();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!(reports[0].0, "tell the host which keys changed");
    drop(writer);
    assert_eq!(stream.latest(b"a".to_vec()), None);
    assert_eq!(
        bugs.reports().len(),
        2,
        "the writer's drop's throw is reported too"
    );
}

/// A writer dropped while a panic unwinds empties the stream without
/// calling the host, whose throw there would abort the process.
#[test]
fn a_writer_dropped_while_a_panic_unwinds_does_not_call_the_host() {
    let (stream, writer, keys) = fed_stream();
    writer.set("a", share(0.5));
    heard(&keys);

    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _writer = writer;
        panic!("the feed broke while it held its writer");
    }));

    assert!(unwound.is_err());
    assert_eq!(stream.latest(b"a".to_vec()), None);
    assert!(
        heard(&keys).is_empty(),
        "the host heard nothing during the unwind"
    );
}
