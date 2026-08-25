//! Headless sync benchmark: open a library from a fixture `.bae` tree, connect
//! its configured cloud provider, let the sync loop run for a fixed duration,
//! and report what a cycle costs — the stage-timing lines stream to stderr via
//! tracing, and the retained-materialization row stats print before and after.
//!
//! Points at a copied `.bae` tree via `--home <dir>` (a directory containing a
//! `.bae/`), so a frozen fixture library measures a redesign without touching
//! the live library. The fixture's provider config must name an isolated store
//! prefix: a copied library carries the live device identity, and two clients
//! with one identity on one store is a protocol violation.
//!
//! The encryption key and S3 credentials come from the keychain (same service
//! and store id as the library the fixture was copied from), so the binary must
//! run signed into the same access group when the custody is keychain-backed.
//!
//! Usage: sync_bench --home /path/to/fixture-parent [--seconds 120]

use std::io::Write as _;

fn main() {
    let mut home: Option<String> = None;
    let mut seconds: u64 = 120;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--home" => home = args.next(),
            "--seconds" => {
                seconds = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| {
                        eprintln!("--seconds requires a number");
                        std::process::exit(2);
                    })
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    if let Some(home) = &home {
        // Reads of ~/.bae (registry, library dirs) resolve under the fixture.
        // The keychain is user-scoped, not HOME-scoped, so custody still works.
        std::env::set_var("HOME", home);
    }

    let format = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr);
    format.init();

    if let Err(error) = bae_core::config::init_keyring(&bae_core::diagnostics::Diagnostics::noop())
    {
        eprintln!("initialize keyring failed: {error}");
        std::process::exit(1);
    }

    let library_id = match bae_core::config::Config::active_library_id() {
        Ok(Some(id)) => id,
        Ok(None) => {
            eprintln!("no active library under the chosen home");
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
    eprintln!(
        "bench library: {} ({}) at {}",
        config.store_name,
        config.store_id,
        config.library_path().display()
    );

    report_store_stats(&config, "before");

    let runtime = tokio::runtime::Runtime::new().expect("build tokio runtime");
    let result: Result<(), String> = runtime.block_on(async {
        let handle = coven::Coven::builder(
            coven::StoreDir::new(config.library_path().to_path_buf()),
            config.to_coven(),
        )
        .synced_tables(bae_core::sync::synced_tables())
        .coven_migration_policy(coven::CovenMigrationPolicy::ApplyPending)
        .migrations(bae_core::migrations::all())
        .open()
        .map_err(|e| format!("open store failed: {e}"))?;
        handle
            .connect_sync()
            .await
            .map_err(|e| format!("connect_sync failed: {e}"))?;
        eprintln!("connected; running the sync loop for {seconds}s");
        tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
        Ok(())
    });
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }

    report_store_stats(&config, "after");
    let _ = std::io::stderr().flush();
}

/// Store-file size — the coarse growth signal; row-level stats come from
/// `sqlite3` against the same file, which needs no dependency here.
fn report_store_stats(config: &bae_core::config::Config, label: &str) {
    let db_path = config.library_path().join("store.db");
    let db_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    eprintln!(
        "store stats [{label}]: db {:.1} MB at {}",
        db_bytes as f64 / 1_048_576.0,
        db_path.display()
    );
}
