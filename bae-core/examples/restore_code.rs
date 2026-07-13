//! Emit the active library's restore code (`coven:...`) — the same string macOS
//! Settings → Library → Connect another device produces — by calling coven's own
//! generator against the live config + keychain.
//!
//! The encryption key and Ed25519 signing key live in the data-protection
//! keychain under the `*.fm.bae.desktop` access group, so the built binary must
//! be code-signed into that group to read them. Run from a directory without a
//! `.env` so `seed_dev_keyring` leaves the OS keyring alone (production mode)
//! rather than seeding it from `BAE_*` env vars.

fn main() {
    bae_core::config::init_keyring();

    let library_id = match bae_core::config::Config::active_library_id() {
        Ok(Some(id)) => id,
        Ok(None) => {
            eprintln!("no active library: ~/.bae/active-library does not exist");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("read active-library pointer failed: {e}");
            std::process::exit(1);
        }
    };

    let ids = coven::UuidProvider;
    let config = match bae_core::config::Config::load_registered_library(&library_id, &ids) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("load config failed: {e}");
            std::process::exit(1);
        }
    };
    let dev_mode = bae_core::config::Config::is_dev_mode();

    eprintln!(
        "active library: {} ({}), dev_mode={dev_mode}",
        config.store_name, config.store_id,
    );

    // In dev mode bae's secrets are in `BAE_*` env vars; bridge them into the
    // keyring coven's StoreKeys reads. No-op in production.
    bae_core::config::seed_dev_keyring(&config.store_id);

    // Minting a restore code needs a connected sync manager: coven seeds it
    // from the store's CURRENT membership-head floor, read live from the
    // cloud (not a pure function of local config and keyring state anymore),
    // so this opens the real store and connects its configured provider the
    // same way the running app does at startup, rather than calling a
    // storage-free generator directly.
    let runtime = tokio::runtime::Runtime::new().expect("build tokio runtime");
    let result: Result<String, String> = runtime.block_on(async {
        let handle = coven::Coven::builder(config.to_coven())
            .synced_tables(bae_core::sync::synced_tables())
            .migrations(bae_core::migrations::all())
            .open()
            .map_err(|e| format!("open store failed: {e}"))?;
        handle
            .connect_sync()
            .await
            .map_err(|e| format!("connect_sync failed: {e}"))?;
        handle
            .generate_restore_code()
            .await
            .map_err(|e| format!("generate_restore_code failed: {e}"))
    });

    match result {
        Ok(code) => println!("{code}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
