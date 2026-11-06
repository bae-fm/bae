//! Restore a library on new devices, timing each restore and printing
//! coven's per-step timing lines (`RUST_LOG`, info by default).
//!
//! The bench creates its own library on the S3 storage the runner's
//! environment names (`bench_library`), holding a release of `--tracks`
//! tracks, and waits until the sync loop has uploaded them. Each run then
//! restores the code the library's Add Device QR code carries
//! (`restore_code_for_transfer`) into a new temporary app directory with an
//! in-memory keychain, and removes the new device through the library again,
//! timing that too. At the end, also when a step fails, the library's bucket
//! is emptied.
//!
//! With `--downloads <n>`, each new device restores reading the library's
//! history from storage `n` requests at a time. Without it, the new device
//! restores at the default bae loads a library at.
//!
//! Usage: restore_bench [--runs 3] [--tracks 5000] [--downloads <n>]

mod bench_library;

use std::time::Instant;

fn main() {
    let mut runs: usize = 3;
    let mut tracks: usize = 5000;
    let mut downloads = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--runs" => runs = number(args.next(), "--runs"),
            "--tracks" => tracks = number(args.next(), "--tracks"),
            "--downloads" => {
                downloads = Some(
                    args.next()
                        .and_then(|value| value.parse::<std::num::NonZeroUsize>().ok())
                        .unwrap_or_else(|| {
                            eprintln!("--downloads requires a number of at least 1");
                            std::process::exit(2);
                        }),
                )
            }
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
    let ran = bench_library::run("Restore bench", tracks, async |library| {
        bench_library::uploaded(library).await?;
        let code = library
            .restore_code_for_transfer()
            .await
            .map_err(|error| format!("restore_code_for_transfer failed: {error}"))?;
        restore_runs(library, &code, runs, downloads).await
    });
    if let Err(error) = ran {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn number(value: Option<String>, flag: &str) -> usize {
    value.and_then(|value| value.parse().ok()).unwrap_or_else(|| {
        eprintln!("{flag} requires a number");
        std::process::exit(2);
    })
}

/// Restore `code` `runs` times, each on a new temporary device reading at
/// `downloads` at a time when given, and remove each new device through
/// `source`, the library's own device.
async fn restore_runs(
    source: &coven::CovenHandle,
    code: &str,
    runs: usize,
    downloads: Option<std::num::NonZeroUsize>,
) -> Result<(), String> {
    let mut times = Vec::new();
    for run in 1..=runs {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let new_app_dir =
            bae_app_dir::AppDir::under_home(directory.path()).map_err(|error| error.to_string())?;
        let installation = coven::Coven::in_memory(new_app_dir.coven_app_dir());
        let (_cancelling, cancel) = tokio::sync::watch::channel(false);
        let started = Instant::now();
        let builder = installation.builder();
        let device_name = format!("Restore bench {run}");
        let on_status = |status| eprintln!("run {run}: {status:?} at {:?}", started.elapsed());
        let restored = match downloads {
            // bae loads a library from storage at the default limits; a run
            // at another download limit asks coven directly, configured as
            // bae configures its store.
            Some(downloads) => coven::restore_from_code(
                bae_store::configure_store(builder, bae_library::SCHEMA)
                    .max_concurrent_uploads(crate_default_uploads())
                    .max_concurrent_downloads(downloads),
                code,
                None,
                &device_name,
                on_status,
                &cancel,
            )
            .await
            .map_err(|error| format!("run {run}: restore failed: {error}"))?,
            None => bae_library::restore_from_code(
                builder,
                code,
                None,
                &device_name,
                on_status,
                &cancel,
                &bae_diagnostics::Diagnostics::noop(),
            )
            .await
            .map_err(|error| format!("run {run}: restore failed: {error}"))?,
        };
        let restoring = started.elapsed();
        let tracks = restored
            .read(|sql| {
                Ok::<_, coven::CovenError>(sql.query_row(
                    "SELECT COUNT(*) FROM tracks",
                    [],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .await
            .map_err(|error| error.to_string());
        let device = restored.device_id();
        let downloads = restored.transfer_limits().downloads;
        let closing = Instant::now();
        restored.close().await.map_err(|error| error.to_string())?;
        let closing = closing.elapsed();
        let tracks = tracks?;
        let removing = Instant::now();
        remove_device(source, device).await.map_err(|error| format!("run {run}: {error}"))?;
        let removing = removing.elapsed();
        eprintln!(
            "run {run}: restored {tracks} tracks in {restoring:?}, {downloads} downloads at \
             once; closed in {closing:?}; removed the device in {removing:?}"
        );
        times.push(restoring);
    }
    eprintln!("restores: {times:?}");
    Ok(())
}

/// Remove `device` from the library through `source` and wait until the
/// sync loop has published the removal. `source` may not have read the
/// device's registration yet: coven journals the removal, and the pass that
/// reads on decides it.
async fn remove_device(source: &coven::CovenHandle, device: coven::DeviceId) -> Result<(), String> {
    let (removal, _sign_out) = source
        .remove_device(device)
        .await
        .map_err(|error| format!("remove_device failed: {error}"))?;
    // The write journals the removal; the sync loop claims its slot.
    if let Some(removal) = removal {
        let outcome = source
            .operation_outcome(removal)
            .await
            .map_err(|error| format!("the removal's outcome: {error}"))?;
        if let coven::OperationOutcome::Rejected(reason) = outcome {
            return Err(format!("the removal was refused: {reason:?}"));
        }
        source
            .write(move |sql| Ok::<_, coven::CovenError>(sql.acknowledge_operation(removal)?))
            .await
            .map_err(|error| format!("acknowledging the removal: {error}"))?;
    }
    Ok(())
}

/// The upload limit bae loads a library from storage at.
fn crate_default_uploads() -> std::num::NonZeroUsize {
    bae_app_dir::default_transfer_limits().uploads
}
