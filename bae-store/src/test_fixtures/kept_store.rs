//! A store kept between runs: created and grown once, closed, its folder and
//! device keychain kept, and opened again from a copy of both by its id, as a
//! test reopens a store another process made.
//!
//! The keychain is coven's in-memory one, never the person's: what a store
//! needs of it to open again (its device id and keys) goes to a file beside
//! the folder.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Instant;

use coven::{ClockRef, Coven, StoreId};

use crate::{configure_store, Schema, Store, TestSources};

/// A new store on `schema` with its folder under `app_dir`, and the keychain
/// entries it opens with saved at `keychain`. Close it before copying the
/// folder.
pub async fn create_kept(
    schema: Schema,
    app_dir: &Path,
    keychain: &Path,
    clock: ClockRef,
) -> Store {
    let installation = Coven::in_memory(coven_app_dir(app_dir));
    let sources = TestSources::on(&clock);
    let handle = configure_store(sources.builder(&installation), schema)
        .create_store("Kept Library", "Test Device", &[], |_, _| Ok(()))
        .await
        .expect("create the kept store");
    installation.save_device_keychain(keychain).expect("save the kept keychain");
    sources.store(handle)
}

/// The store `id` that `create_kept` made, in a copy of its closed folder at
/// `app_dir`, over a copy of the keychain file saved at `keychain`.
pub async fn open_kept(
    schema: Schema,
    app_dir: &Path,
    keychain: &Path,
    id: StoreId,
    clock: ClockRef,
) -> Store {
    let installation = Coven::in_memory_with_saved_keychain(coven_app_dir(app_dir), keychain)
        .expect("load the kept keychain");
    let sources = TestSources::on(&clock);
    let handle = configure_store(sources.builder(&installation), schema)
        .open(id, |_| ())
        .await
        .expect("open the kept store");
    sources.store(handle)
}

fn coven_app_dir(app_dir: &Path) -> PathBuf {
    bae_app_dir::AppDir::at(app_dir).expect("create the app directory").coven_app_dir()
}

/// Writes the rows of `start..end` into a store, with originals for the files
/// it names under the folder given: a kept seed's own rows.
pub type SeedFn =
    for<'a> fn(&'a Store, &'a Path, u64, u64) -> Pin<Box<dyn Future<Output = ()> + 'a>>;

/// Stores on one schema, grown by one seed function to each of `sizes`, made
/// once and kept under `target/bench-seed`, so that every run opens a copy.
pub struct KeptSeeds {
    pub schema: Schema,
    /// Bumped when `seed` writes different rows, so the seeds are made again.
    pub version: u32,
    /// Ascending; each size is a copy of the one before, grown to its size.
    pub sizes: &'static [u64],
    pub seed: SeedFn,
}

impl KeptSeeds {
    /// The folder holding the kept seeds for this schema, this seed and this
    /// coven: a change to the schema's SQL, to `version` or to the coven pin
    /// names another folder, and the seeds are made again there.
    fn root(&self) -> PathBuf {
        use sha2::{Digest, Sha256};
        let mut key = Sha256::new();
        for sql in self.schema.sql() {
            key.update(sql.len().to_le_bytes());
            key.update(sql);
        }
        key.update(self.version.to_le_bytes());
        for size in self.sizes {
            key.update(size.to_le_bytes());
        }
        let mut pin: Vec<&str> = include_str!("../../../Cargo.lock")
            .lines()
            .filter(|line| line.contains("bae-fm/coven"))
            .collect();
        pin.sort_unstable();
        pin.dedup();
        assert!(!pin.is_empty(), "Cargo.lock names no coven pin");
        for line in pin {
            key.update(line);
        }
        let key: String = key.finalize().iter().take(8).map(|byte| format!("{byte:02x}")).collect();
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/bench-seed").join(key)
    }

    /// The kept seed of size `n`, a folder holding the closed store's folder
    /// (`app`), its keychain file and its id, made if it is not there. It
    /// appears whole or not at all.
    async fn kept(&self, n: u64) -> PathBuf {
        let root = self.root();
        let mut previous: Option<(u64, PathBuf)> = None;
        for size in self.sizes.iter().copied().take_while(|size| *size <= n) {
            let dir = root.join(size.to_string());
            if !dir.exists() {
                let started = Instant::now();
                let building = root.join(format!("{size}.building"));
                if building.exists() {
                    std::fs::remove_dir_all(&building).expect("clear an unfinished seed");
                }
                std::fs::create_dir_all(&building).expect("create the seed folder");
                let app = building.join("app");
                let keychain = building.join("keychain");
                let (store, from) = match &previous {
                    Some((from, before)) => {
                        copy_dir(&before.join("app"), &app);
                        let id = read_store_id(before);
                        std::fs::copy(before.join("keychain"), &keychain)
                            .expect("copy the keychain");
                        let store = open_kept(self.schema, &app, &keychain, id, kept_clock()).await;
                        (store, *from)
                    }
                    None => (create_kept(self.schema, &app, &keychain, kept_clock()).await, 0),
                };
                (self.seed)(&store, &root.join("originals"), from, size).await;
                store.close().await.expect("close the seeded store");
                std::fs::write(building.join("store-id"), store.store_id().to_string())
                    .expect("write the store id");
                std::fs::rename(&building, &dir).expect("keep the seed");
                eprintln!("SEEDED n={size} in {:.1} s", started.elapsed().as_secs_f64());
            }
            previous = Some((size, dir));
        }
        previous.expect("sizes holds n").1
    }

    /// A copy of the kept seed of size `n`, opened. The `TempDir` owns the
    /// copy, so it must outlive the store.
    pub async fn seeded_copy(&self, n: u64) -> (Store, tempfile::TempDir) {
        let seed = self.kept(n).await;
        let tmp = tempfile::TempDir::new().expect("create the copy's folder");
        let app = tmp.path().join("app");
        let keychain = tmp.path().join("keychain");
        copy_dir(&seed.join("app"), &app);
        std::fs::copy(seed.join("keychain"), &keychain).expect("copy the keychain");
        let store =
            open_kept(self.schema, &app, &keychain, read_store_id(&seed), kept_clock()).await;
        (store, tmp)
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create the copy's folder");
    for entry in std::fs::read_dir(from).expect("read the seed folder") {
        let entry = entry.expect("read a seed entry");
        let target = to.join(entry.file_name());
        let kind = entry.file_type().expect("read a seed entry's kind");
        if kind.is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            assert!(kind.is_file(), "{} is neither a file nor a folder", entry.path().display());
            std::fs::copy(entry.path(), target).expect("copy a seed file");
        }
    }
}

fn kept_clock() -> ClockRef {
    std::sync::Arc::new(coven::FixedClock::new(super::fixed_now().into()))
}

fn read_store_id(seed: &Path) -> StoreId {
    let text = std::fs::read_to_string(seed.join("store-id")).expect("read the store id");
    StoreId(uuid::Uuid::parse_str(text.trim()).expect("parse the store id"))
}
