//! Headless sync benchmark: time how long a library's sync loop takes to
//! upload what it holds, printing coven's per-stage timing lines
//! (`RUST_LOG`, info by default).
//!
//! The bench creates its own library on the S3 storage the runner's
//! environment names (`bench_library`), writes a release of `--tracks`
//! tracks into it before its storage is set up, sets the storage up, and
//! waits at most `--seconds` for the sync loop to upload every write. At the
//! end, also when a step fails, the library's bucket is emptied.
//!
//! Usage: sync_bench [--tracks 5000] [--seconds 120]

mod bench_library;

use std::time::{Duration, Instant};

fn main() {
    let mut tracks: usize = 5000;
    let mut seconds: u64 = 120;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tracks" => {
                tracks = args.next().and_then(|value| value.parse().ok()).unwrap_or_else(|| {
                    eprintln!("--tracks requires a number");
                    std::process::exit(2);
                })
            }
            "--seconds" => {
                seconds = args.next().and_then(|value| value.parse().ok()).unwrap_or_else(|| {
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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let started = Instant::now();
    let ran = bench_library::run("Sync bench", tracks, async |library| {
        eprintln!("created the library and set its storage up in {:?}", started.elapsed());
        let uploading = Instant::now();
        tokio::time::timeout(Duration::from_secs(seconds), bench_library::uploaded(library))
            .await
            .map_err(|elapsed| {
                format!("the sync loop had not uploaded every write: {elapsed} after {seconds}s")
            })??;
        eprintln!("uploaded {tracks} tracks in {:?}", uploading.elapsed());
        Ok(())
    });
    if let Err(error) = ran {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
