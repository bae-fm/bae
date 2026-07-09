use tracing::warn;

/// Initialize the keyring credential store.
///
/// On macOS, uses the protected data store with iCloud cloud-sync enabled,
/// so the encryption key is backed up via iCloud Keychain (if the user has it on).
///
/// Must be called once at startup before any keyring operations.
pub fn init_keyring() {
    #[cfg(target_os = "macos")]
    {
        use std::collections::HashMap;
        use tracing::info;
        let config = HashMap::from([("cloud-sync", "true")]);
        match apple_native_keyring_store::protected::Store::new_with_configuration(&config) {
            Ok(store) => {
                keyring_core::set_default_store(store);
                info!("Keyring initialized (protected store, iCloud sync enabled)");
            }
            Err(e) => {
                warn!("Failed to create protected keyring store: {e}, falling back to local");
                if let Ok(store) = apple_native_keyring_store::protected::Store::new() {
                    keyring_core::set_default_store(store);
                    info!("Keyring initialized (protected store, local only)");
                }
            }
        }
    }

    #[cfg(target_os = "android")]
    {
        use tracing::info;
        if let Ok(store) = android_native_keyring_store::Store::new() {
            keyring_core::set_default_store(store);
            info!("Keyring initialized (Android keystore)");
        } else {
            warn!("Failed to create Android keyring store");
        }
    }

    #[cfg(target_os = "windows")]
    {
        use tracing::info;
        match windows_native_keyring_store::Store::new() {
            Ok(store) => {
                keyring_core::set_default_store(store);
                info!("Keyring initialized (Windows Credential Manager)");
            }
            Err(e) => warn!("Failed to create Windows keyring store: {e}"),
        }
    }

    // coven namespaces every key entry under the host app's identity, which the
    // host must set once before any keyring access. "bae" keeps bae's coven key
    // entries from colliding with any other coven-based app on the same machine.
    // Set-once, so it's safe to run through every init path.
    if let Err(error) = coven::set_keyring_service("bae") {
        warn!("Failed to register keyring service: {error}");
    }
}

/// Install an in-memory keyring store and set coven's keyring service for tests.
///
/// coven's `KeyService` reads and writes the keyring instead of the environment,
/// and its getters panic unless the service is set. Tests don't run
/// `init_keyring` (which would install the OS store and prompt), so this is the
/// startup every test needing the keyring calls: an in-memory store stands in
/// for the OS keyring, and the service is set to "bae" to match production.
/// Genuinely set-once: the mock store is installed on the first call and kept
/// for the rest of the process. Replacing it on a later call (as this used to)
/// wipes entries other parallel tests already wrote — the store is one
/// process-global namespace — which flaked any test reading a key a sibling had
/// just stored. Entries stay isolated by library id (see the test-manager
/// setup) instead of by a fresh store per test.
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
