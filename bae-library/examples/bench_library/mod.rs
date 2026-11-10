//! The library a bench runs over: one it creates for the run on the S3
//! storage the runner's environment names, with an in-memory keychain and a
//! temporary app directory, and deletes from storage when the run ends. No
//! bench touches the person's keychain, their `~/.bae` or their libraries.
//!
//! The environment names the storage:
//!
//! - `BAE_BENCH_S3_BUCKET` and `BAE_BENCH_S3_REGION`;
//! - `BAE_BENCH_S3_ENDPOINT`, for a service other than Amazon S3, and
//!   `BAE_BENCH_S3_PROFILE`, the service's name as coven's `S3ProfileName`
//!   spells it (`RustFs`, `SeaweedFs`, `CephRgw`, `VersityGw`), for a
//!   self-hosted one;
//! - `BAE_BENCH_S3_ACCESS_KEY_ID` and `BAE_BENCH_S3_SECRET_ACCESS_KEY`, a key
//!   that may do in that bucket what a library's member key does.
//!
//! Every S3 library has its bucket to itself (§20.3 of coven's spec), so the
//! bucket must be empty when a run starts, and the run empties it again when
//! it ends. Runs on one bucket go one at a time.

use std::{panic::AssertUnwindSafe, sync::Arc};

/// Where a run's library lives on storage, and the key that reaches it.
struct BenchStorage {
    location: coven::StorageConfig,
    credentials: coven::S3Credentials,
    /// The run's bucket.
    bucket: String,
}

impl BenchStorage {
    /// The storage the environment names: a bucket only this run's library
    /// uses.
    fn from_environment() -> Result<Self, String> {
        let endpoint = optional_variable("BAE_BENCH_S3_ENDPOINT")?
            .map(|endpoint| endpoint.parse())
            .transpose()
            .map_err(|error| format!("BAE_BENCH_S3_ENDPOINT: {error}"))?;
        let profile = optional_variable("BAE_BENCH_S3_PROFILE")?
            .map(|name| serde_json::from_value(serde_json::Value::String(name)))
            .transpose()
            .map_err(|error| format!("BAE_BENCH_S3_PROFILE: {error}"))?
            .map(coven::S3Profile::Named);
        let bucket = variable("BAE_BENCH_S3_BUCKET")?;
        Ok(Self {
            location: coven::StorageConfig::S3 {
                bucket: bucket.clone(),
                region: variable("BAE_BENCH_S3_REGION")?,
                endpoint,
                profile,
            },
            bucket,
            credentials: coven::S3Credentials {
                access_key_id: variable("BAE_BENCH_S3_ACCESS_KEY_ID")?,
                secret_access_key: coven::SecretText::new(variable(
                    "BAE_BENCH_S3_SECRET_ACCESS_KEY",
                )?),
            },
        })
    }
}

fn variable(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|error| format!("{name}: {error}"))
}

fn optional_variable(name: &str) -> Result<Option<String>, String> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(format!("{name}: {error}")),
    }
}

/// The clock and id source the run's library and its storage tools share,
/// made once as an app's composition makes them.
struct BenchComposition {
    clock: coven::ClockRef,
    ids: coven::IdSourceRef,
}

/// A library created for the run, open, with sync running.
struct BenchLibrary {
    handle: coven::CovenHandle,
    // The store's installation and directory, kept until the store is closed.
    _installation: coven::Coven,
    _directory: tempfile::TempDir,
}

/// Wait until the sync loop has uploaded every write and file the library
/// open on `handle` holds.
pub(crate) async fn uploaded(handle: &coven::CovenHandle) -> Result<(), String> {
    let mut unsent = handle.subscribe_unsent_work();
    loop {
        match unsent.next().await {
            Some(Ok(work)) if work.is_empty() => return Ok(()),
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(format!("reading the unsent work failed: {error}")),
            None => return Err("the library closed before its writes were uploaded".into()),
        }
    }
}

impl BenchLibrary {
    /// Create a library named `name` holding a release of `tracks` tracks,
    /// set its storage up on `storage` and start its sync. A failure after
    /// the store exists closes it before returning.
    async fn create(
        composition: &BenchComposition,
        storage: &BenchStorage,
        name: &str,
        tracks: usize,
    ) -> Result<Self, String> {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let app_dir =
            bae_app_dir::AppDir::under_home(directory.path()).map_err(|error| error.to_string())?;
        let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
        let builder = installation
            .builder()
            .clock(composition.clock.clone())
            .id_source(composition.ids.clone());
        let handle = bae_store::configure_store(builder, bae_library::SCHEMA)
            .create_store(name)
            .await
            .map_err(|error| format!("creating the library failed: {error}"))?;
        let library = Self { handle, _installation: installation, _directory: directory };
        match library.fill(storage, name, tracks).await {
            Ok(()) => Ok(library),
            Err(error) => match library.close().await {
                Ok(()) => Err(error),
                Err(closing) => Err(format!("{error}; then closing the library failed: {closing}")),
            },
        }
    }

    async fn fill(&self, storage: &BenchStorage, name: &str, tracks: usize) -> Result<(), String> {
        let track_ids = bae_records::test_fixtures::chunked_tracks::track_ids(tracks);
        self.handle
            .write(move |sql| {
                bae_records::test_fixtures::chunked_tracks::seed_release(&sql, &track_ids)
            })
            .await
            .map_err(|error| format!("writing the library's tracks failed: {error}"))?;
        self.handle
            .setup_s3_storage(
                storage.location.clone(),
                name,
                storage.credentials.access_key_id.clone(),
                storage.credentials.secret_access_key.clone(),
                |_| (),
            )
            .await
            .map_err(|error| format!("setting the library's storage up failed: {error}"))?;
        self.handle.start_sync().await.map_err(|error| format!("start_sync failed: {error}"))
    }

    /// Close the store; its keychain and directory go after it.
    async fn close(self) -> Result<(), String> {
        self.handle.close().await.map_err(|error| error.to_string())
    }
}

/// Create a library named `name` holding a release of `tracks` tracks in
/// the bucket the environment names, run `body` over its open store, and
/// close it. Then empty the bucket, whether creating, `body` or closing
/// failed or panicked. A panic in them unwinds on once the bucket is
/// emptied.
pub(crate) fn run(
    name: &str,
    tracks: usize,
    body: impl AsyncFnOnce(
        &coven::CovenHandle,
        &coven::ClockRef,
        &coven::IdSourceRef,
    ) -> Result<(), String>,
) -> Result<(), String> {
    let storage = BenchStorage::from_environment()?;
    let clock: coven::ClockRef = Arc::new(coven::SystemClock);
    let ids = Arc::new(bae_util::time_ordered_ids::TimeOrderedIds::new(clock.clone()));
    let composition = BenchComposition { clock, ids };
    eprintln!("bench library in bucket {}", storage.bucket);
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    let ran = std::panic::catch_unwind(AssertUnwindSafe(|| {
        runtime.block_on(async {
            let library = BenchLibrary::create(&composition, &storage, name, tracks).await?;
            let ran = body(&library.handle, &composition.clock, &composition.ids).await;
            match (ran, library.close().await) {
                (Ok(()), closed) => closed,
                (Err(error), Ok(())) => Err(error),
                (Err(error), Err(closing)) => {
                    Err(format!("{error}; then closing the library failed: {closing}"))
                }
            }
        })
    }));
    let emptied = match runtime.block_on(coven::empty_s3_bucket(
        storage.location.clone(),
        storage.credentials.clone(),
        composition.clock.clone(),
        composition.ids.clone(),
    )) {
        Ok(emptied) => {
            eprintln!(
                "emptied {}: {} objects deleted, {} unfinished uploads aborted",
                storage.bucket, emptied.objects, emptied.uploads
            );
            Ok(())
        }
        Err(error) => {
            Err(format!("emptying {} failed, so storage still holds it: {error}", storage.bucket))
        }
    };
    match ran {
        Err(panic) => {
            if let Err(error) = emptied {
                eprintln!("{error}");
            }
            std::panic::resume_unwind(panic)
        }
        Ok(Ok(())) => emptied,
        Ok(Err(error)) => Err(match emptied {
            Ok(()) => error,
            Err(emptying) => format!("{error}\n{emptying}"),
        }),
    }
}
