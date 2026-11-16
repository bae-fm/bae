use super::*;
use std::collections::VecDeque;

/// A query that answers each of `results` in turn, then ends. Its first run
/// says it started on `started` and then waits for `gate`.
struct Answers {
    results: VecDeque<u8>,
    started: Option<tokio::sync::oneshot::Sender<()>>,
    gate: Option<std::sync::mpsc::Receiver<()>>,
}

impl Answers {
    fn of(results: &[u8]) -> Self {
        Self { results: results.iter().copied().collect(), started: None, gate: None }
    }
}

impl LiveQueryEvent for Answers {
    type Event = u8;
}

impl CovenLiveQuery for Answers {
    fn changed(&self) -> impl Future<Output = ()> + Send + 'static {
        std::future::ready(())
    }

    fn run_changed(&mut self) -> Option<u8> {
        if let Some(started) = self.started.take() {
            started.send(()).expect("the test waits for the run to start");
        }
        if let Some(gate) = self.gate.take() {
            gate.recv().expect("the test opens the gate");
        }
        self.results.pop_front()
    }

    fn has_ended(&self) -> bool {
        self.results.is_empty()
    }
}

#[tokio::test]
async fn it_returns_each_result_then_none_once_the_query_ends() {
    let mut results = LiveResults::new(Answers::of(&[1, 2]), coven::task::HeardBugs::shared());
    assert_eq!(results.next().await, Some(1));
    assert_eq!(results.next().await, Some(2));
    assert_eq!(results.next().await, None);
}

/// A reader that stops waiting while the run is on the blocking pool, as a
/// `select!` does, leaves the run's result for the next read.
#[tokio::test]
async fn a_result_whose_reader_stopped_waiting_goes_to_the_next_read() {
    let (started, run_started) = tokio::sync::oneshot::channel();
    let (open, gate) = std::sync::mpsc::channel();
    let query = Answers { started: Some(started), gate: Some(gate), ..Answers::of(&[1, 2]) };
    let mut results = LiveResults::new(query, coven::task::HeardBugs::shared());
    tokio::select! {
        _ = results.next() => panic!("the run waits for its gate"),
        started = run_started => started.expect("the run starts"),
    }
    open.send(()).expect("the run waits for its gate");
    assert_eq!(results.next().await, Some(1));
    assert_eq!(results.next().await, Some(2));
}

/// A query whose run panics outside the query's own run.
struct Panics;

impl LiveQueryEvent for Panics {
    type Event = u8;
}

impl CovenLiveQuery for Panics {
    fn changed(&self) -> impl Future<Output = ()> + Send + 'static {
        std::future::ready(())
    }

    fn run_changed(&mut self) -> Option<u8> {
        panic!("coven's code around the run panicked")
    }

    fn has_ended(&self) -> bool {
        false
    }
}

/// A run that panics goes to the bug sink and ends the query: the panic
/// poisons the lock that holds the query, and no later read runs it.
#[tokio::test]
async fn a_run_that_panics_ends_the_query() {
    let heard = coven::task::HeardBugs::shared();
    let mut results = LiveResults::new(Panics, heard.clone());
    assert_eq!(results.next().await, None);
    assert_eq!(heard.take().len(), 1);
    assert_eq!(results.next().await, None);
    assert_eq!(heard.take().len(), 0);
}
