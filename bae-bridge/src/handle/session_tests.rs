//! How a released handle's library close ends when it panics.

use std::sync::{Arc, Mutex};

use super::{Session, SessionState};

/// A library no test asks for its standing.
struct Unasked;

impl crate::OpenStanding for Unasked {
    fn standing(
        self: Arc<Self>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<crate::BridgeLibraryStanding, crate::BridgeError>,
                > + Send,
        >,
    > {
        unreachable!("no test asks this library for its standing")
    }
}

fn store() -> coven::StoreId {
    coven::StoreId("00000000-0000-0000-0000-000000000001".parse().expect("a store id"))
}

/// A session open on a runtime of its own, in `sessions`' record of open
/// libraries.
fn open_session(sessions: &Arc<crate::host::LibrarySessions>) -> Session {
    Session(Mutex::new(SessionState::Open {
        runtime: tokio::runtime::Runtime::new().expect("build the session's runtime"),
        record: sessions.open(store(), Arc::new(Unasked)),
    }))
}

/// A close that breaks its guarantee partway.
async fn panicking_close() -> super::Closing {
    if true {
        panic!("the close broke its guarantee");
    }
    Ok(())
}

/// A reporter that keeps what it is told, and the record.
fn reporting() -> (bae_diagnostics::Diagnostics, Arc<bae_diagnostics::RecordingBugReporter>) {
    let reports = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    (bae_diagnostics::Diagnostics::noop().reporting_bugs_to(reports.clone()), reports)
}

/// Released on a runtime thread, the close runs on a thread of its own; its
/// panic is caught there and reported, so the open that later waits for that
/// close is not failed by it.
#[test]
fn a_released_close_that_panics_on_its_thread_is_reported_not_raised_into_the_next_open() {
    let sessions = crate::host::LibrarySessions::new();
    let session = open_session(&sessions);
    let (diagnostics, reports) = reporting();
    let caller = tokio::runtime::Runtime::new().expect("build the caller's runtime");

    caller.block_on(async {
        session.end_released(panicking_close(), &diagnostics);
    });

    assert!(
        caller.block_on(sessions.open_standing(store())).is_none(),
        "the closing library left the record"
    );
    sessions.wait(store());

    let reports = reports.reports();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert!(reports[0].1.contains("the close broke its guarantee"), "{reports:?}");
}

/// Released off any runtime, the close runs inside the handle's `Drop`; its
/// panic is caught there and reported, never unwinding out of the `Drop`.
#[test]
fn a_released_close_that_panics_inline_is_reported_not_unwound() {
    let sessions = crate::host::LibrarySessions::new();
    let session = open_session(&sessions);
    let (diagnostics, reports) = reporting();

    session.end_released(panicking_close(), &diagnostics);

    let reports = reports.reports();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert!(reports[0].1.contains("the close broke its guarantee"), "{reports:?}");
}
