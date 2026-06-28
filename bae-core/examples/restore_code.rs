//! Emit the active library's restore code (`coven:...`) — the same string macOS
//! Settings → Library → Connect another device produces — by calling coven's own
//! generator against the live config + keychain.
//!
//! The encryption key and Ed25519 signing key live in the data-protection
//! keychain under the `*.fm.bae.desktop` access group, so the built binary must
//! be code-signed into that group to read them. Run from a directory without a
//! `.env` so it resolves the OS keyring (production mode), not env vars.

fn main() {
    bae_core::config::init_keyring();

    let ids = coven::UuidProvider;
    let config = bae_core::config::Config::load(&ids);
    let dev_mode = bae_core::config::Config::is_dev_mode();

    eprintln!(
        "active library: {} ({}), dev_mode={dev_mode}",
        config.library_name, config.library_id,
    );

    // In dev mode bae's secrets are in `BAE_*` env vars; bridge them into the
    // keyring coven's KeyService reads. No-op in production.
    bae_core::config::seed_dev_keyring(&config.library_id);
    let key_service = coven::KeyService::new(config.library_id.clone());

    match coven::generate_restore_code(&config.to_coven(), &key_service) {
        Ok(code) => println!("{code}"),
        Err(e) => {
            eprintln!("generate_restore_code failed: {e}");
            std::process::exit(1);
        }
    }
}
