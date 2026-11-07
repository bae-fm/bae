//! A store kept between runs: created and grown once, closed, its folder and
//! device keychain kept, and opened again from a copy of both by its id, as a
//! test reopens a store another process made.
//!
//! The keychain is coven's in-memory one, never the person's: what a store
//! needs of it to open again (its device id and keys) goes to a file beside
//! the folder.

use std::path::{Path, PathBuf};

use coven::{ClockRef, Coven, StoreId};

use crate::{configure_store, Schema, Store};

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
    let handle = configure_store(installation.builder(), schema)
        .clock(clock.clone())
        .create_store("Kept Library")
        .await
        .expect("create the kept store");
    installation.save_device_keychain(keychain).expect("save the kept keychain");
    Store::new(handle, clock, coven::task::HeardBugs::shared())
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
    let handle = configure_store(installation.builder(), schema)
        .clock(clock.clone())
        .open(id, |_| ())
        .await
        .expect("open the kept store");
    Store::new(handle, clock, coven::task::HeardBugs::shared())
}

fn coven_app_dir(app_dir: &Path) -> PathBuf {
    bae_app_dir::AppDir::at(app_dir).expect("create the app directory").coven_app_dir()
}
