use tracing::warn;

use crate::diagnostics::{Diagnostics, TelemetryEvent};

/// Initialize the keyring credential store.
///
/// coven installs no store of its own — the host does, and must, before any
/// keyring access. With no default store every credential read fails, so each
/// supported platform needs a branch here, and a failure to install one is
/// logged at `error` and shipped as `keyring_init_failed` through `diagnostics`
/// (constructed at process start, so it exists before this runs) rather than
/// passed over.
///
/// Apple platforms use the protected data store with iCloud cloud-sync on, so
/// the encryption key rides iCloud Keychain when the user has it enabled.
///
/// Must be called once at startup before any keyring operation.
pub fn init_keyring(diagnostics: &Diagnostics) {
    // A target with no branch below installs no store, and then every credential
    // read fails at runtime with nothing to point at. That is exactly how iOS
    // shipped — the Apple branch was gated on `macos` alone. A new target now
    // breaks the build instead of the app.
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "android",
        target_os = "windows",
        target_os = "linux",
    )))]
    compile_error!("init_keyring has no keyring-store branch for this target");

    // Linux has no OS keyring, so no store installs and every credential read
    // fails — the same outcome as a failed store creation elsewhere, reported
    // the same way rather than passed over in silence. Tests never reach this;
    // they install a mock store via `install_test_keyring`.
    #[cfg(target_os = "linux")]
    {
        use tracing::error;
        error!(
            "Linux has no OS keyring store. No default store is installed, so every credential \
             read will fail."
        );
        diagnostics.event(TelemetryEvent::KeyringInitFailed {});
    }

    // iOS as well as macOS: apple-native-keyring-store is a dependency on both
    // (see Cargo.toml), and an iOS build with no branch here installs no store
    // at all.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        use std::collections::HashMap;
        use tracing::{error, info};
        let config = HashMap::from([("cloud-sync", "true")]);
        match apple_native_keyring_store::protected::Store::new_with_configuration(&config) {
            Ok(store) => {
                keyring_core::set_default_store(store);
                info!("Keyring initialized (protected store, iCloud sync enabled)");
            }
            Err(e) => {
                warn!("Failed to create protected keyring store: {e}, falling back to local");
                match apple_native_keyring_store::protected::Store::new() {
                    Ok(store) => {
                        keyring_core::set_default_store(store);
                        info!("Keyring initialized (protected store, local only)");
                    }
                    Err(e) => {
                        error!(
                            "Failed to create local protected keyring store: {e}. No default store \
                             is installed, so every credential read will fail."
                        );
                        diagnostics.event(TelemetryEvent::KeyringInitFailed {});
                    }
                }
            }
        }
    }

    #[cfg(target_os = "android")]
    {
        use tracing::{error, info};
        match android_native_keyring_store::Store::new() {
            Ok(store) => {
                keyring_core::set_default_store(store);
                info!("Keyring initialized (Android keystore)");
            }
            Err(e) => {
                error!(
                    "Failed to create Android keyring store: {e}. No default store is installed, \
                     so every credential read will fail."
                );
                diagnostics.event(TelemetryEvent::KeyringInitFailed {});
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        use tracing::{error, info};
        match windows_native_keyring_store::Store::new() {
            Ok(store) => {
                keyring_core::set_default_store(store);
                info!("Keyring initialized (Windows Credential Manager)");
            }
            Err(e) => {
                error!(
                    "Failed to create Windows keyring store: {e}. No default store is installed, \
                     so every credential read will fail."
                );
                diagnostics.event(TelemetryEvent::KeyringInitFailed {});
            }
        }
    }

    // coven namespaces every key entry under the host app's identity, which the
    // host must set before any keyring access — "bae" keeps these entries from
    // colliding with another coven-based app on the same machine. Set-once, so
    // every init path can run it.
    if let Err(error) = coven::set_keyring_service("bae") {
        warn!("Failed to register keyring service: {error}");
    }
}

/// Install an in-memory keyring store and set coven's keyring service for tests.
///
/// coven's `StoreKeys` reads and writes the keyring, and its getters panic unless
/// the service is set. Tests don't run `init_keyring` (which would install the OS
/// store and prompt), so every test needing a key calls this instead.
///
/// Set-once, and it must stay that way: the store is one process-global
/// namespace, so re-installing it on a later call would wipe entries parallel
/// tests already wrote. Tests stay isolated by library id, not by a fresh store
/// each.
#[cfg(any(test, feature = "test-utils"))]
pub fn install_test_keyring() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        keyring_core::set_default_store(
            keyring_core::mock::Store::new().expect("create mock keyring store"),
        );
        coven::set_keyring_service("bae").expect("register test keyring service");
    });
}
