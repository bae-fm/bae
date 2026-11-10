#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;
use bae_util::test_channels::HANG_GUARD;
use std::sync::mpsc;

/// A change counted between the read and the wait ends the wait at once.
#[test]
fn a_change_after_the_read_ends_the_wait_at_once() {
    let wake = StreamWake::new();
    let seen = wake.changes();
    wake.notify();
    wake.wait_past(seen);
}

/// A thread waiting for a change wakes when one is counted.
#[test]
fn a_waiter_wakes_on_the_next_change() {
    let wake = StreamWake::new();
    let seen = wake.changes();
    let (woke_tx, woke_rx) = mpsc::channel();
    let waiter = {
        let wake = wake.clone();
        std::thread::spawn(move || {
            wake.wait_past(seen);
            woke_tx.send(()).unwrap();
        })
    };

    wake.wait_until_waiting();
    wake.notify();

    woke_rx.recv_timeout(HANG_GUARD).expect("the waiter never woke");
    waiter.join().unwrap();
}
