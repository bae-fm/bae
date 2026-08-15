use crate::diagnostics::{Diagnostics, TelemetryEvent};

/// Register bae's keyring namespace and let coven install the platform store
/// and policy it owns. A registration failure is reported to telemetry and the
/// caller; startup must not continue with an unusable credential store.
pub fn init_keyring(diagnostics: &Diagnostics) -> Result<(), coven::KeyError> {
    coven::set_keyring_service("bae").inspect_err(|_error| {
        diagnostics.event(TelemetryEvent::KeyringInitFailed {});
    })
}

/// Install an in-memory keyring store and set coven's keyring service for tests.
///
/// Coven's custody owners read and write the keyring only after this service is
/// set. Rust tests and debug UI-test app processes cannot use the signed app's
/// OS keyring, so their composition roots call this instead of `init_keyring`.
///
/// Set-once, and it must stay that way: the store is one process-global
/// namespace, so re-installing it on a later call would wipe entries parallel
/// tests already wrote. Tests stay isolated by library id, not by a fresh store
/// each.
#[cfg(any(test, feature = "test-utils", debug_assertions))]
pub fn install_test_keyring() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        coven::install_test_keyring_service("bae").expect("register test keyring service");
    });
}
