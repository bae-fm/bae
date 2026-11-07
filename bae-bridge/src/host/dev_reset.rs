//! The debug Mac app's dev reset, run on the host before any library opens.

use std::sync::Arc;

use crate::{BridgeError, BridgeHost};

#[uniffi::export(async_runtime = "tokio")]
impl BridgeHost {
    /// Wipe this device's bae state and recreate one library from `profile`,
    /// before any library opens; returns the new library's id, for the app to
    /// open. Every library is removed from this device without opening it:
    /// its directory and its store keys go, and its saved restore code is
    /// deleted. Then bae's directory is emptied, and the library is created,
    /// started, given the profile's preferences, connected to its storage and
    /// given the Discogs key, then closed. Each step is written into
    /// `setup`'s stream, counted where it can be; the first failure stops the
    /// reset and is returned. `scripts/reset-bae.py` empties the profile's
    /// bucket before it launches the app, so the new library starts on an
    /// empty bucket of its own.
    pub async fn dev_reset(
        self: Arc<Self>,
        profile: crate::BridgeDevResetProfile,
        device_name: String,
        setup: Arc<crate::BridgeLibrarySetup>,
    ) -> Result<String, BridgeError> {
        let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
        let host = self.clone();
        self.operations
            .clone()
            .run_to_completion(self.clone(), "dev reset", move || async move {
                host.run_dev_reset(profile, &device_name, &steps).await
            })
            .await
    }
}

impl BridgeHost {
    async fn run_dev_reset(
        self: &Arc<Self>,
        profile: crate::BridgeDevResetProfile,
        device_name: &str,
        steps: &crate::screens::SetupSteps,
    ) -> Result<String, BridgeError> {
        use crate::types::BridgeSetupStep as Step;

        steps.uncounted(Step::FinishingRemovals);
        tracing::info!("dev reset: finishing earlier unfinished removals");
        self.finish_removals_now(steps).await?;

        let libraries = self.app_dir.discover_libraries(&self.coven, &self.diagnostics).await?;
        let total = libraries.len() as u64;
        let mut removed = std::collections::HashSet::new();
        for (done, library) in libraries.iter().enumerate() {
            steps.step(Step::RemovingLibraries, done as u64, total);
            tracing::info!(library = %library.id, "dev reset: removing a library");
            let store = crate::from_app::store(&library.id)?;
            self.closed(store).await?;
            self.remove_unopenable_library_now(
                store,
                &crate::screens::ClosingFeed::new(None).steps(),
            )
            .await
            .map_err(|error| {
                BridgeError::categorized(
                    error.category(),
                    format!("dev reset: removing library {store} didn't finish: {error}"),
                )
            })?;
            removed.insert(store);
        }

        let codes: Vec<_> = self
            .saved_restore_codes_now()
            .await?
            .iter()
            .map(coven::SavedRestoreEntry::store_id)
            .filter(|store| removed.contains(store))
            .collect();
        let total = codes.len() as u64;
        for (done, store) in codes.into_iter().enumerate() {
            steps.step(Step::ForgettingRestoreCodes, done as u64, total);
            tracing::info!(library = %store, "dev reset: deleting a saved restore code");
            self.forget_saved_restore_code_now(store).await?;
        }

        steps.uncounted(Step::ClearingAppData);
        tracing::info!("dev reset: emptying bae's directory");
        self.app_dir.clear().map_err(BridgeError::internal)?;

        steps.uncounted(Step::CreatingLibrary);
        let name = crate::from_app::name(&profile.library_name)?;
        tracing::info!(name = %profile.library_name, "dev reset: creating the library");
        let created = Arc::new(crate::OpenedLibrary::new(
            bae_library::create_library(self.builder(), name, &self.diagnostics).await?,
            self.clone(),
        ));
        let library_id = created.library_id();

        steps.uncounted(Step::SettingUpLibrary);
        let app = self
            .diagnostics
            .blocking("start the new library", move || created.open(200, false))
            .await
            .map_err(BridgeError::unfinished)??;
        let set_up = self.set_up_dev_library(&app, profile, device_name, steps).await;
        let closing = app.clone();
        let closed = self
            .diagnostics
            .blocking("close the new library", move || closing.close_library(None))
            .await
            .map_err(BridgeError::unfinished)?;
        set_up?;
        closed?;
        Ok(library_id)
    }

    /// Give the new library the profile's preferences, its storage, and the
    /// Discogs key, as Settings and storage setup would.
    async fn set_up_dev_library(
        &self,
        app: &Arc<crate::AppHandle>,
        profile: crate::BridgeDevResetProfile,
        device_name: &str,
        steps: &crate::screens::SetupSteps,
    ) -> Result<(), BridgeError> {
        tracing::info!(
            identify_automatically = profile.identify_automatically,
            import_when_identified = profile.import_when_identified,
            "dev reset: setting the preferences"
        );
        app.clone().set_identify_automatically(profile.identify_automatically).await?;
        app.clone().set_import_when_identified(profile.import_when_identified).await?;

        steps.uncounted(crate::types::BridgeSetupStep::ConnectingStorage);
        tracing::info!("dev reset: connecting S3 storage");
        app.clone().save_sync_config(profile.sync_config, device_name.to_string(), None).await?;

        tracing::info!("dev reset: saving the Discogs key");
        match app.clone().save_discogs_token(profile.discogs_token).await? {
            crate::types::BridgeDiscogsSaveOutcome::Rejected => {
                Err(BridgeError::config("dev reset: Discogs rejected the profile's token"))
            }
            crate::types::BridgeDiscogsSaveOutcome::Valid
            | crate::types::BridgeDiscogsSaveOutcome::Unvalidated { .. } => Ok(()),
        }
    }
}
