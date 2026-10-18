use crate::diagnostics::{Diagnostics, TelemetryEvent};

/// Register bae's keyring namespace and let coven install the platform store
/// and policy it owns. A registration failure is reported to telemetry and the
/// caller; startup must not continue with an unusable credential store.
pub fn init_keyring(diagnostics: &Diagnostics) -> Result<(), coven::KeyError> {
    coven::set_keyring_service("bae").inspect_err(|_error| {
        diagnostics.event(TelemetryEvent::KeyringInitFailed {});
    })
}
