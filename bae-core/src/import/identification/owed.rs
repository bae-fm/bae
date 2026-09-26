//! Importing what the automatic admission identified, when the person asked
//! for that.
//!
//! The decision that a candidate is imported is made where its verdict is
//! written: an automatic run that settles, while "Import automatically when
//! identified" is on, on a verdict needing nothing from anyone stores that the
//! verdict owes an import, in the same write. So a candidate identified before
//! the setting was on owes nothing, and neither does one a person's own run
//! identified. What is owed is then paid here, from the stored row rather than
//! from the event that wrote it: right after the verdict lands, whenever
//! something changes what the candidate is, and whenever the queue reads
//! everything afresh — a finished scan, which is also the launch, and a
//! settings change. Paying is idempotent, because the row is the record: an
//! attempt that ends, or a decision not to import, removes it, and an import
//! already running for the candidate is left to end it.

use super::*;
use crate::import::handle::{OwedImport, OwedImportDeclined};

/// Pay what `key`'s verdict owes, as the candidate stands now.
pub(super) async fn pay_owed_import(
    context: &Context,
    config: &watch::Receiver<crate::config::Config>,
    key: &str,
) {
    let (enabled, destination) = {
        let config = config.borrow();
        (
            config.prefs.identification.imports_when_identified(),
            config.import_destination(),
        )
    };
    match context.import.import_owed(key, enabled, destination).await {
        Ok(OwedImport::NotOwed) => {}
        Ok(OwedImport::AlreadyImporting) => {
            debug!("identification: {key} owes an import and one is already running for it");
        }
        Ok(OwedImport::BeingIdentified) => {
            info!(
                "identification: {key} owes an import but is being identified again; \
                 the verdict that run stores decides"
            );
        }
        Ok(OwedImport::Declined(reason)) => {
            let why = match reason {
                OwedImportDeclined::SettingOff => "importing when identified is off",
                OwedImportDeclined::Edited => "its draft changed after it was identified",
                OwedImportDeclined::NotReady => {
                    "it is not ready to import unattended as it stands"
                }
            };
            info!("identification: not importing {key} automatically: {why}");
        }
        Ok(OwedImport::Started { import_id }) => {
            info!(
                "identification: importing {key} automatically as {import_id} \
                 ({:?}, pinned: {})",
                destination.storage_mode, destination.pin
            );
        }
        Ok(OwedImport::FailedToStart { error }) => {
            warn!("identification: the automatic import of {key} could not start: {error}");
        }
        Err(error) => {
            warn!(
                "identification: could not decide {key}'s owed import ({error}); \
                 it stays owed until the queue reads it again"
            );
        }
    }
}

/// Pay what every candidate the queue is responsible for owes. A candidate
/// the queue is not responsible for — set aside, taken into a grouping, gone —
/// keeps what it owes until it is one of them again, when what it has become
/// decides.
pub(super) async fn pay_owed_imports(
    context: &Context,
    config: &watch::Receiver<crate::config::Config>,
) {
    let owed = match context.library_manager.load_owed_imports().await {
        Ok(owed) => owed,
        Err(error) => {
            warn!("identification: could not read the owed imports ({error}); paying none this time");
            return;
        }
    };
    if owed.is_empty() {
        return;
    }
    let candidates = match context.library_manager.load_sweepable_candidates().await {
        Ok(candidates) => candidates,
        Err(error) => {
            warn!(
                "identification: could not read the candidate list ({error}); \
                 paying no owed import this time"
            );
            return;
        }
    };
    // One import per content hash: every candidate sharing it is the same
    // files, and the release the first one becomes is all of theirs.
    let mut paid = std::collections::HashSet::new();
    for candidate in candidates {
        let content_hash = candidate.files.content_hash();
        if owed.contains(&content_hash) && paid.insert(content_hash) {
            pay_owed_import(context, config, &candidate.key()).await;
        }
    }
}
