use super::*;

/// A report that counts how often it was told.
fn counting_report() -> (InvalidationReport, Arc<std::sync::atomic::AtomicUsize>) {
    let told = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = told.clone();
    let report = InvalidationReport::new(move || {
        counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    });
    (report, told)
}

/// A stream cpal says must be rebuilt, or whose device went away, is
/// reported to its owner, which builds a new one on the current default.
/// A glitch or any other error is only logged: rebuilding for those would
/// interrupt a stream that still plays. Sabotage: drop `DeviceNotAvailable`
/// from the reported kinds, and a stream whose device was unplugged is
/// never replaced; leave out the report, and the owner never asks.
#[test]
fn only_a_stream_that_cant_go_on_is_reported_to_its_owner() {
    let (invalidated, invalidations) = mpsc::channel();
    let (report, told) = counting_report();
    let mut handle = stream_error_handler(invalidated, report);

    for kind in [ErrorKind::Xrun, ErrorKind::BackendError] {
        handle(kind.into());
    }
    assert!(invalidations.try_recv().is_err());
    assert_eq!(told.load(std::sync::atomic::Ordering::Relaxed), 0);

    handle(cpal::Error::with_message(ErrorKind::StreamInvalidated, "default device changed"));
    handle(cpal::Error::with_message(ErrorKind::DeviceNotAvailable, "device disconnected"));
    assert_eq!(
        invalidations.try_iter().collect::<Vec<_>>(),
        ["default device changed", "device disconnected"]
    );
    assert_eq!(told.load(std::sync::atomic::Ordering::Relaxed), 2, "the owner is told of each");
}

/// On a Mac, the default output unit moving a stream to a new default
/// device is reported too: the new device would play it at its own rate,
/// so the stream is rebuilt there at the track's, and the old device's
/// format goes back. Sabotage: only log `DeviceChanged`, and the new
/// device converts the track while the old one keeps the track's rate.
#[cfg(target_os = "macos")]
#[test]
fn a_move_to_a_new_default_device_is_reported_on_macos() {
    let (invalidated, invalidations) = mpsc::channel();
    let mut handle = stream_error_handler(invalidated, counting_report().0);

    handle(cpal::Error::with_message(ErrorKind::DeviceChanged, "default output device changed"));

    assert_eq!(invalidations.try_iter().collect::<Vec<_>>(), ["default output device changed"]);
}

/// Elsewhere the move needs nothing more: iOS reports one when headphones
/// are unplugged and the route already plays on the speaker.
#[cfg(not(target_os = "macos"))]
#[test]
fn a_move_to_a_new_device_is_only_logged_off_macos() {
    let (invalidated, invalidations) = mpsc::channel();
    let mut handle = stream_error_handler(invalidated, counting_report().0);

    handle(ErrorKind::DeviceChanged.into());

    assert!(invalidations.try_recv().is_err());
}
