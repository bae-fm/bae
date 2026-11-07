use std::sync::mpsc::{channel, Sender, TryRecvError};
use std::sync::Arc;

use coven::task::{Bug, BugSink};

use super::WorkerThread;

/// A bug sink that hands each bug to the test. Its channel closes once every
/// holder has let it go.
struct SentBugs(Sender<Bug>);

impl BugSink for SentBugs {
    fn report(&self, bug: &Bug) {
        self.0.send(bug.clone()).expect("the test is listening");
    }
}

fn queue_gone() -> Bug {
    Bug { operation: "test worker", message: "the queue is gone".to_string() }
}

/// A panic in a worker's body reaches the bug sink as it happens, on its
/// thread, with nothing joining it. Sabotage: run the body uncaught, and the
/// panic waits in the join handle for a join nobody makes.
#[test]
fn a_panic_in_the_body_is_reported_as_it_happens() {
    let (heard, bugs) = channel();
    let worker = WorkerThread::<()>::spawn("test worker", Arc::new(SentBugs(heard)), |_, _| {
        panic!("the queue is gone")
    });
    // The worker is let go unjoined: only its thread can still report.
    drop(worker);
    assert_eq!(bugs.recv().ok(), Some(queue_gone()));
}

/// The join of a thread whose body panicked reports nothing more: the panic
/// was reported once, as it happened.
#[test]
fn the_join_does_not_report_the_panic_again() {
    let (heard, bugs) = channel();
    let worker = WorkerThread::<()>::spawn("test worker", Arc::new(SentBugs(heard)), |_, _| {
        panic!("the queue is gone")
    });
    assert_eq!(bugs.recv().ok(), Some(queue_gone()));
    worker.stop_and_join(|_| {});
    assert_eq!(bugs.try_recv(), Err(TryRecvError::Empty));
}
