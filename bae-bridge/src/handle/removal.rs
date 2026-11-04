use super::*;

/// What removing a library from this device loses: `writes` edits and
/// `files` attached files still waiting to upload to its storage.
#[derive(Clone, Copy, uniffi::Record)]
pub struct BridgeUnsentWork {
    pub writes: u64,
    pub files: u64,
}

impl BridgeUnsentWork {
    pub(crate) fn from_core(work: coven::UnsentWork) -> Self {
        Self { writes: work.writes, files: work.files }
    }
}

#[derive(Clone, uniffi::Enum)]
pub enum BridgeLibraryRemoval {
    Removed,
    /// The library left its store, but this device's files for it remain.
    Unfinished {
        removal: BridgeUnfinishedRemoval,
    },
}

/// A recorded removal that never finished, with why. When `files_remain`,
/// the store is gone and `BridgeHost::finish_removals` deletes this device's
/// files for it; until then `BridgeHost::unfinished_removal` reports it,
/// across restarts. Otherwise the removal stopped before the store was
/// deleted: the library is still here and can be removed again.
#[derive(Clone, uniffi::Record)]
pub struct BridgeUnfinishedRemoval {
    pub error: BridgeError,
    pub files_remain: bool,
}

impl BridgeLibraryRemoval {
    /// How a removal ended: done, stopped after the store was deleted with
    /// this device's files left, or failed with nothing deleted.
    pub(crate) fn from_core(
        removal: Result<(), bae_library::LibraryRemovalError>,
    ) -> Result<Self, BridgeError> {
        use bae_library::LibraryRemovalError as E;
        match removal {
            Ok(()) => Ok(Self::Removed),
            Err(error @ E::Unfinished(_)) => Ok(Self::Unfinished {
                removal: BridgeUnfinishedRemoval::new(error.category(), &error, true),
            }),
            Err(error @ (E::Stop(_) | E::Record(_) | E::Store(_) | E::Closing { .. })) => {
                Err(BridgeError::categorized(error.category(), &error))
            }
        }
    }
}

impl BridgeUnfinishedRemoval {
    pub(crate) fn new(
        category: bae_diagnostics::UiErrorCategory,
        why: &dyn std::fmt::Display,
        files_remain: bool,
    ) -> Self {
        Self {
            error: BridgeError::diagnostic(crate::BridgeErrorCategory::from_core(category), why),
            files_remain,
        }
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    /// What removing this library would lose, for the person to confirm
    /// first. Nothing about the library changes.
    pub async fn unsent_before_removal(
        self: std::sync::Arc<Self>,
    ) -> Result<BridgeUnsentWork, BridgeError> {
        self.run_exported("AppHandle.unsent_before_removal", |this| async move {
            Ok(BridgeUnsentWork::from_core(this.services.unsent_before_removal().await?))
        })
        .await
    }

    /// Stop services and leave the store before deleting its local files,
    /// discarding what this device hasn't uploaded. Each step it runs is
    /// written to `closing`'s stream, when the app shows the removal.
    pub async fn remove_library(
        self: std::sync::Arc<Self>,
        closing: Option<std::sync::Arc<crate::BridgeLibraryClosing>>,
    ) -> Result<BridgeLibraryRemoval, BridgeError> {
        let steps = crate::screens::ClosingSteps::of(closing);
        self.runtime
            .clone()
            .run_to_completion(self.clone(), "remove library", move || async move {
                #[cfg(feature = "desktop")]
                steps.stop_servers(&self.desktop).await;
                let removal = self.host.remove_open_library(&self.services, &steps).await;
                if matches!(removal, Ok(()) | Err(bae_library::LibraryRemovalError::Unfinished(_)))
                {
                    self.session.removed();
                }
                BridgeLibraryRemoval::from_core(removal)
            })
            .await
    }
}

#[cfg(test)]
#[path = "removal_tests.rs"]
mod tests;
