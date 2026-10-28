//! Restore this Mac's active library on new devices, timing each restore and
//! printing coven's per-step timing lines (`RUST_LOG`, info by default).
//!
//! Opens the active library from the live `~/.bae` with the system keychain,
//! as the macOS app does, and takes the restore code its Add Device QR code
//! carries (`restore_code_for_transfer`). Each run restores that code into a
//! fresh temporary app directory with an in-memory keychain, so the new
//! device's keys never reach the system keychain, then removes the new device
//! from the library through the source handle, timing that too. Every run
//! registers a real device in the library and sends real provider requests.
//!
//! The library's keys live in the data-protection keychain under the
//! `*.fm.bae.desktop` access group, so the binary must run signed into that
//! group, and the macOS app must not hold the library open: coven refuses a
//! second writable open with `AlreadyOpen`. To sign it, put the built binary
//! in an app bundle (`RestoreBench.app/Contents/MacOS/restore_bench`) with the
//! macOS app's `embedded.provisionprofile` in `Contents/`, add the runpath
//! `@executable_path/../Frameworks`, copy `bae-ffmpeg/dist/lib/*.dylib` into
//! `Contents/Frameworks`, and sign the dylibs and then the bundle with the
//! development identity and an entitlements file holding the app's
//! `keychain-access-groups` and application identifier, without
//! `--options runtime`.
//!
//! With `BAE_RESTORE_CODE` set to a restore code, such as the text of the Add
//! Device QR code, the bench restores that code and opens no library: it
//! needs neither the system keychain nor the app closed, and removes no
//! device. A code that carries no S3 key takes one from
//! `BAE_RESTORE_S3_KEY_ID` and `BAE_RESTORE_S3_SECRET`.
//!
//! Usage: restore_bench [--runs 3] [--keep-devices]

use std::time::Instant;

fn main() {
    let mut runs: usize = 3;
    let mut remove_devices = true;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--runs" => {
                runs = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| {
                        eprintln!("--runs requires a number");
                        std::process::exit(2);
                    })
            }
            "--keep-devices" => remove_devices = false,
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
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
    let store_id = match bae_core::config::Config::active_library_id(&app_dir) {
        Ok(Some(id)) => match uuid::Uuid::parse_str(&id) {
            Ok(id) => coven::StoreId(id),
            Err(error) => {
                eprintln!("invalid active-library id: {error}");
                std::process::exit(1);
            }
        },
        Ok(None) => {
            eprintln!("no active library under {}", home.display());
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("read active-library pointer failed: {error}");
            std::process::exit(1);
        }
    };
    let runtime = tokio::runtime::Runtime::new().expect("build tokio runtime");
    if let Ok(code) = std::env::var("BAE_RESTORE_CODE") {
        let s3 = match (
            std::env::var("BAE_RESTORE_S3_KEY_ID"),
            std::env::var("BAE_RESTORE_S3_SECRET"),
        ) {
            (Ok(access_key_id), Ok(secret)) => Some(coven::S3Credentials {
                access_key_id,
                secret_access_key: coven::SecretText::new(secret),
            }),
            _ => None,
        };
        if let Err(error) = runtime.block_on(restore_runs(&code, s3, runs, None)) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = runtime.block_on(bench(&app_dir, store_id, runs, remove_devices)) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn bench(
    app_dir: &bae_core::config::AppDir,
    store_id: coven::StoreId,
    runs: usize,
    remove_devices: bool,
) -> Result<(), String> {
    let opening = Instant::now();
    let source = bae_core::library::configure_store(
        bae_core::library::Keychain::System.store_builder(app_dir),
    )
    .open(store_id)
    .await
    .map_err(|error| format!("open the active library failed: {error}"))?;
    eprintln!(
        "opened {} ({store_id}) in {:?}",
        source
            .store_name()
            .await
            .map_err(|error| error.to_string())?,
        opening.elapsed()
    );
    // Removing a device is an administrative change, which needs the
    // storage connection a sync loop builds; the app always runs one.
    source
        .start_sync()
        .await
        .map_err(|error| format!("start_sync failed: {error}"))?;
    let code = source
        .restore_code_for_transfer()
        .await
        .map_err(|error| format!("restore_code_for_transfer failed: {error}"))?;
    restore_runs(&code, None, runs, remove_devices.then_some(&source)).await?;
    source.close().await.map_err(|error| error.to_string())
}

/// Restore `code` `runs` times, each on a new temporary device, removing each
/// new device through `source` when given.
async fn restore_runs(
    code: &str,
    s3: Option<coven::S3Credentials>,
    runs: usize,
    source: Option<&coven::CovenHandle>,
) -> Result<(), String> {
    let mut times = Vec::new();
    for run in 1..=runs {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let new_app_dir = bae_core::config::AppDir::under_home(directory.path())
            .map_err(|error| error.to_string())?;
        let installation = coven::TestCoven::new();
        let (_cancelling, cancel) = tokio::sync::watch::channel(false);
        let started = Instant::now();
        let restored = bae_core::library::restore_from_code(
            installation.builder(new_app_dir.store_layout()),
            &new_app_dir,
            code,
            s3.clone(),
            &format!("Restore bench {run}"),
            |status| eprintln!("run {run}: {status:?} at {:?}", started.elapsed()),
            &cancel,
        )
        .await
        .map_err(|error| format!("run {run}: restore failed: {error}"))?;
        let restoring = started.elapsed();
        let tracks = restored
            .read(|sql| {
                Ok(sql.query_row("SELECT COUNT(*) FROM tracks", [], |row| {
                    row.get::<_, i64>(0)
                })?)
            })
            .await
            .map_err(|error| error.to_string())?;
        let device = restored.device_id();
        let closing = Instant::now();
        restored.close().await.map_err(|error| error.to_string())?;
        let closing = closing.elapsed();
        let removing = if let Some(source) = source {
            let removing = Instant::now();
            source
                .remove_device(device)
                .await
                .map_err(|error| format!("run {run}: remove_device failed: {error}"))?;
            Some(removing.elapsed())
        } else {
            None
        };
        eprintln!(
            "run {run}: restored {tracks} tracks in {restoring:?}; closed in {closing:?}; \
             removed the device in {removing:?}"
        );
        times.push(restoring);
    }
    eprintln!("restores: {times:?}");
    Ok(())
}
