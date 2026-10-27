//! Emit the active library's restore code (`coven:...`) — the same string macOS
//! Settings → Library → Connect another device produces — by calling coven's own
//! generator against the live config + keychain.
//!
//! The encryption key and Ed25519 signing key live in the data-protection
//! keychain under the `*.fm.bae.desktop` access group, so the built binary must
//! be code-signed into that group to read them.

fn main() {
    // This example ships no telemetry; a no-op sink satisfies init_keyring.
    if let Err(error) = bae_core::config::init_keyring(&bae_core::diagnostics::Diagnostics::noop())
    {
        eprintln!("initialize keyring failed: {error}");
        std::process::exit(1);
    }

    let Some(home) = dirs::home_dir() else {
        eprintln!("could not determine the home directory");
        std::process::exit(1);
    };
    let app_dir = match bae_core::config::AppDir::under_home(&home) {
        Ok(app_dir) => app_dir,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    let library_id = match bae_core::config::Config::active_library_id(&app_dir) {
        Ok(Some(id)) => id,
        Ok(None) => {
            eprintln!(
                "no active library: {} does not exist",
                app_dir.active_library_pointer().display()
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("read active-library pointer failed: {e}");
            std::process::exit(1);
        }
    };

    let store_id = match uuid::Uuid::parse_str(&library_id).map(coven::StoreId) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("invalid active-library id: {e}");
            std::process::exit(1);
        }
    };
    let runtime = tokio::runtime::Runtime::new().expect("build tokio runtime");
    let result: Result<String, String> = runtime.block_on(async {
        let handle =
            bae_core::library::configure_store(coven::Coven::builder(app_dir.store_layout()))
                .open(store_id)
                .await
                .map_err(|e| format!("open store failed: {e}"))?;
        let name = handle.store_name().await.map_err(|e| e.to_string())?;
        eprintln!("active library: {name} ({store_id})");
        handle
            .unlock_store_key()
            .await
            .map_err(|e| format!("unlock_store_key failed: {e}"))?;
        let code = handle
            .restore_code()
            .await
            .map_err(|e| format!("restore_code failed: {e}"))?;
        handle.close().await.map_err(|e| e.to_string())?;
        Ok(code)
    });

    match result {
        Ok(code) => println!("{code}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
