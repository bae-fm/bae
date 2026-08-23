use super::*;

// =========================================================================
// Casting to a network receiver (Cast, UPnP, AirPlay)
// =========================================================================

#[cfg(feature = "cast")]
#[uniffi::export]
impl AppHandle {
    /// Whether casting is available at all. Turning it off stops discovery and
    /// disconnects any session in flight; the config subscription then hides the
    /// Cast control — no app keeps its own copy.
    pub fn set_cast_enabled(&self, enabled: bool) -> Result<(), BridgeError> {
        self.services
            .set_cast_enabled(enabled)
            .map_err(BridgeError::config)
    }

    /// Start browsing for Cast devices (call when the device picker opens).
    pub fn start_cast_discovery(&self) {
        self.cast.start_discovery();
    }

    /// Stop browsing for Cast devices (call when the device picker closes).
    pub fn stop_cast_discovery(&self) {
        self.cast.stop_discovery();
    }

    /// The service types a host that browses on bae's behalf must browse for,
    /// paired with the tag to report each result under.
    pub fn get_renderer_service_types(&self) -> Vec<crate::types::BridgeRendererService> {
        bae_core::renderer::RENDERER_SERVICE_TYPES
            .into_iter()
            .map(crate::types::BridgeRendererService::from_core)
            .collect()
    }

    /// Take a renderer service the host's own browser resolved. Only hosts that
    /// browse on bae's behalf call this; where bae reads the network itself, its
    /// own discovery fills the list.
    pub fn renderer_found(&self, service: crate::types::BridgeReportedRenderer) {
        self.cast.renderer_found(service.into_core());
    }

    /// Drop a renderer service the host's browser no longer sees.
    pub fn renderer_lost(
        &self,
        service_type: crate::types::BridgeRendererServiceType,
        instance_name: String,
    ) {
        self.cast
            .renderer_lost(service_type.into_core(), &instance_name);
    }

    pub fn subscribe_cast_devices(
        &self,
        callback: Box<dyn crate::types::CastDevicesCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let cast = self.cast.clone();
        let runtime = self.runtime.handle().clone();
        let task = crate::operation_runtime::spawn(runtime, move || async move {
            let mut devices = cast.subscribe_devices();
            callback.on_value(
                devices
                    .borrow_and_update()
                    .clone()
                    .into_iter()
                    .map(crate::types::BridgeCastDevice::from_core)
                    .collect(),
            );
            while devices.changed().await.is_ok() {
                callback.on_value(
                    devices
                        .borrow_and_update()
                        .clone()
                        .into_iter()
                        .map(crate::types::BridgeCastDevice::from_core)
                        .collect(),
                );
            }
        });
        std::sync::Arc::new(crate::LiveSubscription::new(task))
    }

    /// Stop casting and return playback to local output.
    pub fn stop_casting(&self) {
        self.cast.stop_casting();
    }

    /// Whether playback is currently on a Cast device, and which.
    pub fn get_cast_status(&self) -> crate::types::BridgeCastStatus {
        crate::types::BridgeCastStatus::from_core(self.cast.status())
    }
}

#[cfg(feature = "cast")]
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    /// Cast playback to the device with `device_id`.
    pub async fn cast_to(self: std::sync::Arc<Self>, device_id: String) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.cast.cast_to(&device_id).await.map_err(|error| {
                let detail = error.to_string();
                match error {
                    // AirPlay receivers the sender can't drive get their own localized
                    // picker line, not the generic internal-error one.
                    bae_cast::CastError::AirPlayPinRequired
                    | bae_cast::CastError::AirPlayEncryptionUnsupported => BridgeError::diagnostic(
                        crate::types::BridgeErrorCategory::AirPlayUnsupported,
                        detail,
                    ),
                    _ => BridgeError::internal(detail),
                }
            })
        })
        .await
    }
}

// =========================================================================
// Desktop-only: Import, Cover fetching
// =========================================================================

#[cfg(feature = "desktop")]
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    pub fn get_discogs_token(&self) -> Result<Option<String>, BridgeError> {
        Ok(self.services.get_discogs_token()?)
    }

    /// Register the platform artwork analyzer. Called once at app boot
    /// (e.g. from `BaeApp`'s startup path) by the platforms that have one.
    /// Extraction owns artwork OCR and streams its barcode/text signals to
    /// identify. A platform that never calls this has no artwork analyzer, and
    /// extraction treats artwork as no signal source at all — the barcode signal
    /// reports `Absent` (never read) rather than an empty scan.
    pub fn register_artwork_analyzer(
        &self,
        analyzer: Box<dyn crate::types::ArtworkAnalyzerCallback>,
    ) {
        let adapter = std::sync::Arc::new(crate::signals::ArtworkAnalyzerAdapter::new(analyzer));
        self.services.extraction_register_analyzer(adapter);
    }

    /// Start identifying a folder candidate. Identify subscribes first, then
    /// extraction streams the candidate's `Signals` (disc ID, barcodes,
    /// classified text) that identify looks up and the UI surfaces. Events
    /// flow through the unified import event channel → bus → reducer → store,
    /// and the verdict this reaches is persisted like the background sweep's —
    /// core decides all of that, so this stays one call.
    pub fn auto_identify_folder(&self, candidate_key: String) {
        self.services.identify_folder_candidate(candidate_key);
    }

    /// Start re-identifying an existing library release. Extraction resolves
    /// the release's disc ID and artwork from the library. Events stream
    /// through the same identify channel — the UI consumes them by candidate
    /// key the same way it does for folder imports.
    pub fn auto_identify_release(&self, candidate_key: String, release_id: String) {
        let run = self.services.identify_new_run();
        self.services
            .identify_start(run, candidate_key.clone(), CallPriority::Interactive);
        self.services.extraction_start(
            candidate_key,
            ExtractionSource::Release { release_id },
            CallPriority::Interactive,
        );
    }

    /// Stop a candidate's identify pipeline: cancels the identify driver and
    /// the in-flight signal extraction (artwork OCR) for `candidate_key`. The
    /// inverse of `auto_identify_folder` / `auto_identify_release`; a no-op for
    /// a key with nothing running. Called when the UI tears the candidate down
    /// (the re-identify sheet closing).
    pub fn cancel_auto_identify(&self, candidate_key: String) {
        self.services.identify_cancel(&candidate_key);
        self.services.extraction_cancel(&candidate_key);
    }

    /// Toggle a signal in a candidate's toolbar — include or exclude it from
    /// triangulation. The identify driver flips the signal and re-combines
    /// over the surviving signals, emitting the resulting state through the
    /// same event channel. Idempotent: a no-op when the candidate isn't
    /// running.
    pub fn toggle_signal_for_candidate(
        &self,
        candidate_key: String,
        signal: crate::types::BridgeSignalToggle,
    ) {
        self.services
            .identify_toggle_signal(&candidate_key, signal.into_core());
    }

    /// Re-run a candidate's lookups from the toolbar. A live driver resets to
    /// triangulating and re-dispatches from its retained signals, preserving
    /// exclusions; a candidate showing a resumed stored verdict has no driver,
    /// and a fresh interactive run replaces the stored answer.
    pub fn rerun_identify_for_candidate(&self, candidate_key: String) {
        self.services.rerun_identify(candidate_key);
    }

    /// Decide a candidate's identity — the pressing the user picked, or the
    /// decision to read the folder's own tags. The release's documents land
    /// before the pick does, so the candidate's next value draws whole;
    /// nothing comes back, because that value is what the pane renders.
    /// Identification writes the same record itself when a verdict settles on
    /// exactly one match; this is the path for the choices only a person can
    /// make.
    pub async fn pick_candidate_identity(
        self: std::sync::Arc<Self>,
        candidate_key: String,
        pick: crate::types::BridgeIdentityPick,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_pick_candidate_identity(candidate_key, pick.into_core())
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    /// Re-identify commit. Translates the user's `IdentityChoice` into a fully
    /// cross-linked identity vec + metadata pointer, then writes via
    /// `set_identity` — the outcome is indistinguishable from re-importing the
    /// release with the same choice.
    ///
    /// Returns the album id the release lives on after the commit, which may have
    /// changed if the new identity vec didn't fit the source album
    /// (`set_identity` move semantics). Reseeding metadata is the caller's call:
    /// `reset_metadata_to_source` + `update_release_metadata_user_edit`.
    pub async fn re_identify_release(
        self: std::sync::Arc<Self>,
        release_id: String,
        identity_choice: crate::types::BridgeIdentityChoice,
    ) -> Result<String, BridgeError> {
        self.run_exported(move |this| async move {
            let core_choice = identity_choice.into_core();
            this.services
                .re_identify_release(&release_id, core_choice)
                .await
                .map_err(BridgeError::import)?;
            this.services
                .get_album_id_for_release(&release_id)
                .await
                .map_err(BridgeError::database)
        })
        .await
    }

    pub fn subscribe_release_library_status(
        &self,
        source: crate::types::BridgeMetadataSource,
        release_id: String,
        source_group_id: Option<String>,
        callback: Box<dyn crate::types::ReleaseLibraryStatusCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let services = self.services.clone();
        let runtime = self.runtime.handle().clone();
        let task = crate::operation_runtime::spawn(runtime, move || async move {
            let mut values =
                services.subscribe_release_library_status(bae_core::db::LibraryCheck {
                    release_id,
                    source: source.into_core(),
                    source_group_id,
                });
            loop {
                match values.next().await {
                    Ok(value) => {
                        callback.on_value(crate::types::BridgeLibraryStatus::from_core(value))
                    }
                    Err(error) => callback.on_error(BridgeError::database(error)),
                }
            }
        });
        std::sync::Arc::new(crate::LiveSubscription::new(task))
    }

    pub async fn add_watched_folder(
        self: std::sync::Arc<Self>,
        path: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_add_watched_folder(path)
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    pub async fn remove_watched_folder(
        self: std::sync::Arc<Self>,
        path: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_remove_watched_folder(path)
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    pub async fn refresh_watched_folder(
        self: std::sync::Arc<Self>,
        path: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_refresh_watched_folder(path)
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    pub async fn set_folder_release_decision(
        self: std::sync::Arc<Self>,
        key: crate::types::BridgeFolderReleaseDecisionKey,
        decision: crate::types::BridgeFolderReleaseDecision,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_set_folder_release_decision(key.into_core(), decision.into_core())
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    /// Mark the candidate at `path` skipped or unskipped. Persists the change;
    /// the candidate subscription carries the new row to the import view.
    pub async fn set_candidate_skipped(
        self: std::sync::Arc<Self>,
        path: String,
        skipped: bool,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_set_candidate_skipped(path, skipped)
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    /// What the track sheet `sheet_file_id` on candidate `candidate_key` can be
    /// bound to: the folder's audio, each file offered or refused with the
    /// reason. Empty when the sheet names one file per track rather than one
    /// for the disc — a single choice cannot express that layout — and when the
    /// folder holds no audio.
    ///
    /// Core probes each file to decide, so ask for this when a picker opens
    /// rather than holding it alongside the candidate.
    pub async fn sheet_binding_options(
        self: std::sync::Arc<Self>,
        candidate_key: String,
        sheet_file_id: String,
    ) -> Result<Vec<crate::types::BridgeSheetBindingOption>, BridgeError> {
        self.run_exported(move |this| async move {
            Ok(this
                .services
                .import_sheet_binding_options(candidate_key, sheet_file_id)
                .await
                .map_err(BridgeError::import)?
                .into_iter()
                .map(crate::types::BridgeSheetBindingOption::from_core)
                .collect())
        })
        .await
    }

    /// Bind a candidate's track sheet to one of its audio files, or clear the
    /// binding with `audio_file_id: None`.
    ///
    /// Clearing leaves the sheet describing nothing; it does not restore what
    /// the scan proposed. `audio_file_id` must be one the matching
    /// [`Self::sheet_binding_options`] call offered — a refused one is rejected
    /// here rather than at commit.
    ///
    /// Persists the decision and clears the candidate's stored identify
    /// verdict, because a bound sheet is a different disc. The candidate
    /// subscription carries the new roles to the import view.
    pub async fn set_sheet_binding(
        self: std::sync::Arc<Self>,
        candidate_key: String,
        sheet_file_id: String,
        audio_file_id: Option<String>,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_set_sheet_binding(candidate_key, sheet_file_id, audio_file_id)
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    /// Say which disc of the release one of a candidate's track sheets holds,
    /// or take it out of the tracklist with `BridgeSheetDisc::Ignored`.
    ///
    /// Cue filenames are arbitrary — `CD1.cue` may hold disc two — so the
    /// assignment is the truth about which cue is which disc, and no UI reads
    /// it off a name. Discs count from one.
    ///
    /// Persists the decision and clears the candidate's stored identify
    /// verdict, because a re-assigned or ignored sheet is a different
    /// tracklist. The candidate subscription carries it to the import view.
    pub async fn set_sheet_disc(
        self: std::sync::Arc<Self>,
        candidate_key: String,
        sheet_file_id: String,
        disc: crate::types::BridgeSheetDisc,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_set_sheet_disc(candidate_key, sheet_file_id, disc.into_core())
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    /// Put one of a candidate's files in a role, or put it back in the one the
    /// scan proposed. `choice` must be one of that file's
    /// `BridgeCandidateFile::alternatives`.
    ///
    /// This is what a slot's Exclude action calls: taking a file out of the
    /// tracklist is a fact about the folder, so it is stored rather than kept
    /// in whichever pane happens to be open — a pane that dropped the row
    /// locally would have it back the next time a release was picked.
    ///
    /// Persists the decision and clears the candidate's stored identify
    /// verdict, because a folder with one fewer track is a different disc. The
    /// candidate subscription carries the new roles to the import view.
    pub async fn set_file_role(
        self: std::sync::Arc<Self>,
        candidate_key: String,
        file_id: String,
        choice: crate::types::BridgeFileRoleChoice,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_set_file_role(candidate_key, file_id, choice.into_core())
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    /// Scan every watched folder. The import-candidate subscription carries the
    /// resulting list to the UI.
    pub fn scan_watched_folders(&self) -> Result<(), BridgeError> {
        self.services
            .import_scan_watched_folders()
            .map_err(BridgeError::import)
    }

    /// Search for releases with library status check in one call. Cancelled
    /// when the Swift caller drops the awaiting Task — uniffi forwards
    /// cancellation to the Rust future, which drops the in-flight HTTP
    /// request before it completes.
    pub async fn search_for_candidate(
        self: std::sync::Arc<Self>,
        query: crate::types::BridgeSearchQuery,
    ) -> Result<crate::types::BridgeCandidateSearchResults, BridgeError> {
        self.run_exported(move |this| async move {
            use bae_core::import::SearchQuery;

            let (core_query, tab, bridge_source) = match query {
                crate::types::BridgeSearchQuery::General {
                    artist,
                    album,
                    source,
                } => (
                    SearchQuery::General {
                        artist,
                        album,
                        source: source.into_core(),
                    },
                    crate::types::BridgeSearchQueryKind::General,
                    source,
                ),
                crate::types::BridgeSearchQuery::CatalogNumber {
                    catalog_number,
                    source,
                } => (
                    SearchQuery::CatalogNumber {
                        catalog_number,
                        source: source.into_core(),
                    },
                    crate::types::BridgeSearchQueryKind::CatalogNumber,
                    source,
                ),
                crate::types::BridgeSearchQuery::Barcode { barcode, source } => (
                    SearchQuery::Barcode {
                        barcode,
                        source: source.into_core(),
                    },
                    crate::types::BridgeSearchQueryKind::Barcode,
                    source,
                ),
            };

            let grouped = this
                .services
                .import_search_with_status(core_query)
                .await
                .map_err(BridgeError::import)?;

            Ok(crate::types::BridgeCandidateSearchResults::from_core(
                grouped,
                tab,
                bridge_source,
            ))
        })
        .await
    }

    /// Commit a candidate. Nothing about the release rides in: the pick, the
    /// metadata typed over it, the corrected rows and the chosen cover are all
    /// stored under the candidate, so the commit consumes the very values the
    /// pane drew.
    pub async fn start_import(
        self: std::sync::Arc<Self>,
        candidate_key: String,
        storage_mode: BridgeStorageMode,
        pin: bool,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_start_import(&candidate_key, storage_mode.into_core(), pin)
                .await
                .map(|_| ())
                .map_err(BridgeError::import)
        })
        .await
    }

    /// Project the embedded tags of a folder's audio files into the
    /// editor's user-edit shape. Used by the "Add as Unknown"
    /// affordance: the UI calls this to populate the editor before
    /// the user verifies/edits and commits with
    /// `BridgeIdentityChoice::Unknown`.
    pub async fn preview_file_tags_for_folder(
        self: std::sync::Arc<Self>,
        candidate_key: String,
    ) -> Result<crate::types::BridgeReleaseUserEdit, BridgeError> {
        self.run_exported(move |this| async move {
            let edit = this
                .services
                .import_preview_file_tags_for_folder(candidate_key)
                .await
                .map_err(BridgeError::import)?;
            Ok(crate::types::BridgeReleaseUserEdit::from_core(edit))
        })
        .await
    }

    /// Record the cover the user chose for a candidate. Nothing comes back:
    /// the per-candidate subscription delivers the pane's next value.
    pub async fn set_candidate_cover(
        self: std::sync::Arc<Self>,
        candidate_key: String,
        cover: BridgeCoverSelection,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_set_candidate_cover(&candidate_key, cover.into_core())
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    /// Record one album-level field of the candidate's metadata form as the
    /// user left it.
    pub async fn set_candidate_edit_field(
        self: std::sync::Arc<Self>,
        candidate_key: String,
        field: crate::types::BridgeCandidateEditField,
        value: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_set_candidate_edit_field(&candidate_key, field.into_core(), value)
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    /// Record one mapping-table row as the user left it.
    pub async fn set_candidate_track_edit(
        self: std::sync::Arc<Self>,
        candidate_key: String,
        track: crate::types::BridgeRawTrackEdit,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_set_candidate_track_edit(&candidate_key, track.into_core())
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    /// Take one mapping-table row out of the import.
    pub async fn drop_candidate_track(
        self: std::sync::Arc<Self>,
        candidate_key: String,
        track_id: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_drop_candidate_track(&candidate_key, track_id)
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    /// Apply a user-supplied metadata edit (from the edit-metadata sheet) to a
    /// release. Writes the user's edited values directly without touching
    /// identity, `metadata_source`, or cached source payloads.
    pub async fn update_release_metadata_user_edit(
        self: std::sync::Arc<Self>,
        release_id: String,
        edit: crate::types::BridgeReleaseUserEdit,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            let core_edit = crate::types::BridgeReleaseUserEdit::into_core(edit);
            this.services
                .apply_release_metadata_user_edit(&release_id, &core_edit)
                .await
                .map_err(BridgeError::import)
        })
        .await
    }

    /// Seed the EditMetadataSheet's raw form from a library release's current
    /// metadata. bae-core does the projection (current state → wire edit → raw
    /// form); this is pure type translation around the result.
    pub async fn seed_release_edit(
        self: std::sync::Arc<Self>,
        release_id: String,
    ) -> Result<crate::types::BridgeRawReleaseEdit, BridgeError> {
        self.run_exported(move |this| async move {
            let raw = this
                .services
                .release_edit_seed(&release_id)
                .await
                .map_err(BridgeError::import)?;
            Ok(crate::types::BridgeRawReleaseEdit::from_core(raw))
        })
        .await
    }

    /// Re-project a release's metadata from its `metadata_source` /
    /// `metadata_source_release_id` pointer. Returns the projected
    /// `ReleaseUserEdit` without writing — the editor populates its
    /// form with the result; the user re-edits or saves via
    /// `update_release_metadata_user_edit`. Identity rows and the
    /// metadata-source columns are not touched.
    pub async fn reset_metadata_to_source(
        self: std::sync::Arc<Self>,
        release_id: String,
    ) -> Result<crate::types::BridgeReleaseUserEdit, BridgeError> {
        self.run_exported(move |this| async move {
            let edit = this
                .services
                .reset_metadata_to_source(&release_id)
                .await
                .map_err(BridgeError::import)?;
            Ok(crate::types::BridgeReleaseUserEdit::from_core(edit))
        })
        .await
    }

    pub async fn fetch_remote_covers(
        self: std::sync::Arc<Self>,
        release_id: String,
    ) -> Result<Vec<BridgeRemoteCover>, BridgeError> {
        self.run_exported(move |this| async move {
            let covers = this
                .services
                .import_fetch_remote_covers(&release_id)
                .await
                .map_err(BridgeError::import)?;
            Ok(covers
                .into_iter()
                .map(crate::types::BridgeRemoteCover::from_core)
                .collect())
        })
        .await
    }

    /// Bytes of provider art at `url` — art from Cover Art Archive or Discogs
    /// that isn't in the library yet, so there is no image ref to read it by.
    /// Core owns the network: this is the only image fetch that leaves the
    /// device, and its byte cache is core's. The returned validator identifies
    /// the content, so a UI holding a decoded copy replaces it only when the
    /// bytes at the URL actually change.
    ///
    /// `None` when the source serves no image at that address: cover addresses
    /// are derived from a release's ids, so an offered one can turn out to hold
    /// nothing. That is the slot having no image, not a failed load.
    pub async fn fetch_remote_image_bytes(
        self: std::sync::Arc<Self>,
        url: String,
    ) -> Result<Option<crate::types::BridgeRemoteImage>, BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_fetch_remote_image_bytes(url)
                .await
                .map(|image| image.map(crate::types::BridgeRemoteImage::from_core))
                .map_err(BridgeError::import)
        })
        .await
    }
}

// =========================================================================
// Export (desktop-only)
// =========================================================================

// Gated on the target, not on `desktop`: bae-core gates `mod export` itself on the
// target, so the export queue and the track exporter exist on every non-mobile
// build whether or not this crate's `desktop` feature is on.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    // ── Export queue ─────────────────────────────────────────────────

    /// The current export-queue snapshot.
    pub fn get_output_snapshot(&self) -> crate::types::BridgeOutputSnapshot {
        crate::types::BridgeOutputSnapshot::from_core(self.services.output_snapshot())
    }

    pub fn subscribe_outputs(
        &self,
        callback: Box<dyn crate::types::OutputCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let services = self.services.clone();
        let runtime = self.runtime.handle().clone();
        let task = crate::operation_runtime::spawn(runtime, move || async move {
            let mut values = services.subscribe_output_values();
            callback.on_value(crate::types::BridgeOutputSnapshot::from_core(
                values.borrow_and_update().clone(),
            ));
            while values.changed().await.is_ok() {
                callback.on_value(crate::types::BridgeOutputSnapshot::from_core(
                    values.borrow_and_update().clone(),
                ));
            }
        });
        std::sync::Arc::new(crate::LiveSubscription::new(task))
    }

    /// Enqueue a verbatim release export to `target_dir`. It joins the in-memory
    /// serial output queue; the worker drains it one release at a time. The
    /// storage-summary lookup (resolving the pane row's title/size) happens here;
    /// the deep cloud read + copy runs on the queue worker.
    pub async fn enqueue_export(
        self: std::sync::Arc<Self>,
        release_id: String,
        target_dir: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .enqueue_export(&release_id, std::path::PathBuf::from(target_dir))
                .await
                .map_err(BridgeError::export)
        })
        .await
    }

    /// Enqueue a release-level save to `target_dir` under the preset named by
    /// `preset_id`. The preset is resolved and captured whole at enqueue time,
    /// so a later config edit can't change or break this queued save.
    pub async fn enqueue_release_save(
        self: std::sync::Arc<Self>,
        release_id: String,
        target_dir: String,
        preset_id: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .enqueue_release_save(
                    &release_id,
                    std::path::PathBuf::from(target_dir),
                    &preset_id,
                )
                .await
                .map_err(BridgeError::save)
        })
        .await
    }

    /// Pause or resume the export queue. The in-flight export finishes; the queue
    /// stops starting new ones until resumed.
    pub fn set_outputs_paused(&self, paused: bool) {
        self.services.set_outputs_paused(paused);
    }

    /// Cancel a release's export — drops a queued/failed entry or aborts the
    /// in-flight one (a partial copy never lands its destination file).
    pub fn cancel_output(&self, release_id: String) {
        self.services.cancel_output(&release_id);
    }

    /// Retry every failed export now (flips them back to queued and wakes the
    /// worker).
    pub fn retry_outputs(&self) {
        self.services.retry_outputs();
    }

    // ── Track save ───────────────────────────────────────────────────

    /// Save one track to `output_path` under the preset named by `preset_id`
    /// (must apply to track saves). Always a constructed file — decoded, encoded
    /// to the preset codec, tagged, cover embedded — never a verbatim copy.
    pub async fn save_track(
        self: std::sync::Arc<Self>,
        track_id: String,
        output_path: String,
        preset_id: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .save_track(&track_id, std::path::Path::new(&output_path), &preset_id)
                .await
                .map_err(|e| BridgeError::save(format!("{e}")))
        })
        .await
    }

    /// The default filename stem (no extension) a single-track "Save As…"
    /// suggests for `track_id` under the preset named by `preset_id`, rendered
    /// from that preset's token pattern. Reads only the database — no audio or
    /// cover — while seeding a save panel.
    pub async fn save_track_suggested_name(
        self: std::sync::Arc<Self>,
        track_id: String,
        preset_id: String,
    ) -> Result<String, BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .save_track_suggested_name(&track_id, &preset_id)
                .await
                .map_err(|e| BridgeError::save(format!("{e}")))
        })
        .await
    }
}
