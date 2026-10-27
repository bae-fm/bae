//! Headless sync benchmark: open a library from a fixture `.bae` tree, connect
//! its configured cloud provider, let the sync loop run for a fixed duration,
//! and report the stage timings and library row counts before and after.
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
    // The registry and library dirs are read under the fixture's home. The
    // keychain is user-scoped, not home-scoped, so custody still works.
    let home = match home {
        Some(home) => std::path::PathBuf::from(home),
        None => dirs::home_dir().unwrap_or_else(|| {
            eprintln!("could not determine the home directory; pass --home");
            std::process::exit(2);
        }),
    };
    let app_dir = match bae_core::config::AppDir::under_home(&home) {
        Ok(app_dir) => app_dir,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

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

    let library_id = match bae_core::config::Config::active_library_id(&app_dir) {
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
    let store_id = match uuid::Uuid::parse_str(&library_id).map(coven::StoreId) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("invalid active-library id: {e}");
            std::process::exit(1);
        }
    };

    let runtime = tokio::runtime::Runtime::new().expect("build tokio runtime");
    let result: Result<(), String> = runtime.block_on(async {
        let handle =
            bae_core::library::configure_store(coven::Coven::builder(app_dir.store_layout()))
                .open(store_id)
                .await
                .map_err(|e| format!("open store failed: {e}"))?;
        let name = handle.store_name().await.map_err(|e| e.to_string())?;
        eprintln!("bench library: {name} ({store_id})");
        report_store_stats(&handle, "before").await?;
        handle
            .start_sync()
            .await
            .map_err(|e| format!("start_sync failed: {e}"))?;
        eprintln!("running the sync loop for {seconds}s");
        tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
        report_store_stats(&handle, "after").await?;
        handle.close().await.map_err(|e| e.to_string())?;
        Ok(())
    });
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

async fn report_store_stats(handle: &coven::CovenHandle, label: &str) -> Result<(), String> {
    let (albums, releases, tracks) = handle
        .read(|sql| {
            Ok((
                sql.query_row("SELECT COUNT(*) FROM albums", [], |row| {
                    row.get::<_, i64>(0)
                })?,
                sql.query_row("SELECT COUNT(*) FROM releases", [], |row| {
                    row.get::<_, i64>(0)
                })?,
                sql.query_row("SELECT COUNT(*) FROM tracks", [], |row| {
                    row.get::<_, i64>(0)
                })?,
            ))
        })
        .await
        .map_err(|e| e.to_string())?;
    eprintln!("store stats [{label}]: {albums} albums, {releases} releases, {tracks} tracks");
    Ok(())
}
