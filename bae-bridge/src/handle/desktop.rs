use super::*;

// =========================================================================
// Casting to a network receiver (Cast, UPnP, AirPlay)
// =========================================================================

forward! {
    #[cfg(feature = "cast")]
    sync this => {
        /// Whether casting is available at all. Turning it off stops discovery and
        /// disconnects any session in flight; the config subscription then hides the
        /// Cast control — no app keeps its own copy.
        fn set_cast_enabled(enabled: bool) -> Result<(), BridgeError> {
            Ok(this.services.set_cast_enabled(enabled)?)
        }

        /// Start browsing for Cast devices (call when the device picker opens).
        fn start_cast_discovery() {
            this.cast.start_discovery();
        }

        /// Stop browsing for Cast devices (call when the device picker closes).
        fn stop_cast_discovery() {
            this.cast.stop_discovery();
        }

        /// Take a renderer service the host's own browser resolved. Only hosts that
        /// browse on bae's behalf call this; where bae reads the network itself, its
        /// own discovery fills the list.
        fn renderer_found(service: crate::types::BridgeReportedRenderer) {
            this.cast.renderer_found(service.into_core());
        }

        /// Drop a renderer service the host's browser no longer sees.
        fn renderer_lost(
            service_type: crate::types::BridgeRendererServiceType,
            instance_name: String,
        ) {
            this.cast
                .renderer_lost(service_type.into_core(), &instance_name);
        }

        /// Stop casting and return playback to local output.
        fn stop_casting() {
            this.cast.stop_casting();
        }
    }
}

#[cfg(feature = "cast")]
#[uniffi::export]
impl AppHandle {
    /// The service types a host that browses on bae's behalf must browse for,
    /// paired with the tag to report each result under. A constant of core's,
    /// not a call into this handle.
    pub fn get_renderer_service_types(&self) -> Vec<crate::types::BridgeRendererService> {
        bae_core::renderer::RENDERER_SERVICE_TYPES
            .into_iter()
            .map(crate::types::BridgeRendererService::from_core)
            .collect()
    }

    pub fn subscribe_cast_devices(
        &self,
        callback: Box<dyn crate::types::CastDevicesCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let cast = self.cast.clone();
        self.subscribe_watch(
            move |_| cast.subscribe_devices(),
            move |devices| {
                callback.on_value(
                    devices
                        .iter()
                        .cloned()
                        .map(crate::types::BridgeCastDevice::from_core)
                        .collect(),
                )
            },
        )
    }
}

forward! {
    #[cfg(feature = "cast")]
    async this => {
        /// Cast playback to the device with `device_id`.
        fn cast_to(device_id: String) -> () {
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
        }
    }
}

// =========================================================================
// Desktop-only: Import, Cover fetching
// =========================================================================

#[cfg(feature = "desktop")]
#[uniffi::export]
impl AppHandle {
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

    pub fn subscribe_release_library_status(
        &self,
        source: crate::types::BridgeMetadataSource,
        release_id: String,
        source_group_id: Option<String>,
        callback: Box<dyn crate::types::ReleaseLibraryStatusCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_live_query(
            move |services| {
                services.subscribe_release_library_status(bae_core::db::LibraryCheck {
                    release_id,
                    source: source.into_core(),
                    source_group_id,
                })
            },
            move |_, value| match value {
                Ok(value) => callback.on_value(crate::types::BridgeLibraryStatus::from_core(value)),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }
}

forward! {
    #[cfg(feature = "desktop")]
    sync this => {
        fn get_discogs_token() -> Result<Option<String>, BridgeError> {
            Ok(this.services.get_discogs_token()?)
        }

        /// Start identification after the person explicitly opens Find online.
        /// Identify subscribes first, then extraction streams
        /// the candidate's `Signals` (disc ID, barcodes, classified text) that
        /// identify looks up and the UI surfaces. Events flow through the unified
        /// import event channel → bus → reducer → store, and the verdict this
        /// reaches is persisted like the background sweep's — core decides all of
        /// that, so this stays one call.
        fn identify_folder_for_lookup(candidate_key: String) {
            this.services.identify_folder_for_lookup(candidate_key);
        }

        /// Start re-identifying an existing library release. Extraction resolves
        /// the release's disc ID and artwork from the library. Events stream
        /// through the same identify channel — the UI consumes them by candidate
        /// key the same way it does for folder imports.
        fn auto_identify_release(candidate_key: String, release_id: String) {
            this.services
                .identify_release_for_lookup(candidate_key, release_id);
        }

        /// Stop a candidate's identify pipeline: cancels the identify driver and
        /// the in-flight signal extraction (artwork OCR) for `candidate_key`. The
        /// inverse of `identify_folder_for_lookup` / `auto_identify_release`; a no-op for
        /// a key with nothing running. Called when the UI tears the candidate down
        /// (the re-identify sheet closing).
        fn cancel_auto_identify(candidate_key: String) {
            this.services.cancel_identify(&candidate_key);
        }

        /// Toggle a signal in a candidate's toolbar — include or exclude it from
        /// triangulation. The identify driver flips the signal and re-combines
        /// over the surviving signals, emitting the resulting state through the
        /// same event channel. Idempotent: a no-op when the candidate isn't
        /// running.
        fn toggle_signal_for_candidate(
            candidate_key: String,
            signal: crate::types::BridgeSignalToggle,
        ) {
            this.services
                .identify_toggle_signal(&candidate_key, signal.into_core());
        }

        /// Re-run a candidate's lookups from the toolbar. A live driver resets to
        /// triangulating and re-dispatches from its retained signals, preserving
        /// exclusions; a candidate showing a resumed stored verdict has no driver,
        /// and a fresh interactive run replaces the stored answer.
        fn rerun_identify_for_candidate(candidate_key: String) {
            this.services.rerun_identify(candidate_key);
        }

        /// Re-ask only the lookups that failed, keeping what every other provider
        /// found. A live driver retries in place; a candidate showing a resumed
        /// verdict has no live answers to keep, so a fresh run replaces it.
        fn retry_failed_identify_for_candidate(candidate_key: String) {
            this.services.retry_failed_identify(candidate_key);
        }

        /// Submit a candidate's typed search. Fire-and-forget like
        /// `rerun_identify_for_candidate`: every configured provider is asked at
        /// once, and each answer lands on the candidate's runtime as it arrives.
        /// A search already running for this candidate is superseded.
        fn start_candidate_search(
            candidate_key: String,
            query: crate::types::BridgeSearchQuery,
        ) {
            this.services
                .import_start_candidate_search(candidate_key, query.into_core());
        }

        /// Re-ask only the providers whose part of the search failed, keeping what
        /// the others found. A no-op when the candidate has no search running.
        fn retry_candidate_search(candidate_key: String) {
            this.services.import_retry_candidate_search(candidate_key);
        }
    }
}

forward! {
    #[cfg(feature = "desktop")]
    async this => {
        /// Replace candidate metadata from a source. An external release's
        /// documents land before provenance does, so the next value draws whole.
        /// Identification writes the same record itself when a verdict settles on
        /// exactly one match; this is the path for the choices only a person can
        /// make.
        fn select_candidate_metadata_provenance(
            candidate_key: String,
            provenance: crate::types::BridgeMetadataProvenance,
        ) -> u64 {
            Ok(this
                .services
                .import_select_candidate_metadata_provenance(candidate_key, provenance.into_core())
                .await?)
        }

        /// Clear the candidate's draft metadata and source without changing any
        /// physical file or track decisions.
        fn clear_candidate_metadata(candidate_key: String) -> u64 {
            Ok(this
                .services
                .import_clear_candidate_metadata(candidate_key)
                .await?)
        }

        /// Re-identify commit. Translates the user's `ReleaseReseed` into a fully
        /// cross-linked identity vec plus the new metadata provenance, then writes via
        /// `set_identity` — the outcome is indistinguishable from re-importing the
        /// release with the same choice.
        ///
        /// Returns the album id the release lives on after the commit, which may have
        /// changed if the new identity vec didn't fit the source album
        /// (`set_identity` move semantics). Reseeding metadata is the caller's call:
        /// `reset_release_edit_to_source` + `update_release_metadata_user_edit`.
        fn re_identify_release(
            release_id: String,
            reseed: crate::types::BridgeReleaseReseed,
        ) -> String {
            let core_reseed = reseed.into_core();
            this.services
                .re_identify_release(&release_id, core_reseed)
                .await
                .map_err(BridgeError::import)?;
            this.services
                .get_album_id_for_release(&release_id)
                .await
                .map_err(BridgeError::database)
        }

        fn add_watched_folder(path: String) -> () {
            this.services
                .import_add_watched_folder(path)
                .await
                .map_err(BridgeError::import)
        }

        fn remove_watched_folder(path: String) -> () {
            this.services
                .import_remove_watched_folder(path)
                .await
                .map_err(BridgeError::import)
        }

        fn refresh_watched_folder(path: String) -> () {
            this.services
                .import_refresh_watched_folder(path)
                .await
                .map_err(BridgeError::import)
        }

        fn set_folder_release_decision(
            key: crate::types::BridgeFolderReleaseDecisionKey,
            decision: crate::types::BridgeFolderReleaseDecision,
        ) -> () {
            this.services
                .import_set_folder_release_decision(key.into_core(), decision.into_core())
                .await
                .map_err(BridgeError::import)
        }

        /// Mark the candidate at `path` skipped or unskipped. Persists the change;
        /// the candidate subscription carries the new row to the import view.
        fn set_candidate_skipped(path: String, skipped: bool) -> () {
            this.services
                .import_set_candidate_skipped(path, skipped)
                .await
                .map_err(BridgeError::import)
        }

        /// What the track sheet `sheet_file_id` on candidate `candidate_key` can be
        /// bound to: the folder's audio, each file offered or refused with the
        /// reason. Empty when the sheet names one file per track rather than one
        /// for the disc — a single choice cannot express that layout — and when the
        /// folder holds no audio.
        ///
        /// Core probes each file to decide, so ask for this when a picker opens
        /// rather than holding it alongside the candidate.
        fn sheet_binding_options(
            candidate_key: String,
            sheet_file_id: String,
        ) -> Vec<crate::types::BridgeSheetBindingOption> {
            Ok(this
                .services
                .import_sheet_binding_options(candidate_key, sheet_file_id)
                .await
                .map_err(BridgeError::import)?
                .into_iter()
                .map(crate::types::BridgeSheetBindingOption::from_core)
                .collect())
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
        fn set_sheet_binding(
            candidate_key: String,
            sheet_file_id: String,
            audio_file_id: Option<String>,
        ) -> () {
            Ok(this
                .services
                .import_set_sheet_binding(candidate_key, sheet_file_id, audio_file_id)
                .await?)
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
        fn set_sheet_disc(
            candidate_key: String,
            sheet_file_id: String,
            disc: crate::types::BridgeSheetDisc,
        ) -> () {
            Ok(this
                .services
                .import_set_sheet_disc(candidate_key, sheet_file_id, disc.into_core())
                .await?)
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
        fn set_file_role(
            candidate_key: String,
            file_id: String,
            choice: crate::types::BridgeFileRoleChoice,
        ) -> () {
            Ok(this
                .services
                .import_set_file_role(candidate_key, file_id, choice.into_core())
                .await?)
        }

        /// Commit a candidate. Nothing about the release rides in: the metadata
        /// source provenance, the metadata typed over it, the corrected rows and the chosen cover
        /// are all stored under the candidate, so the commit consumes the very
        /// values the pane drew.
        fn start_import(candidate_key: String, storage_mode: BridgeStorageMode, pin: bool) -> () {
            this.services
                .import_start_import(&candidate_key, storage_mode.into_core(), pin)
                .await
                .map(|_| ())
                .map_err(BridgeError::import)
        }

        /// Project the embedded tags of a folder's audio files into the
        /// editor's user-edit shape. Used by the File Tags source
        /// affordance: the UI calls this to populate the editor before
        /// the user verifies/edits and commits with
        /// `BridgeReleaseReseed::FileTags`.
        fn preview_file_tags_for_folder(
            candidate_key: String,
        ) -> crate::types::BridgeReleaseUserEdit {
            let edit = this
                .services
                .import_preview_file_tags_for_folder(candidate_key)
                .await
                .map_err(BridgeError::import)?;
            Ok(crate::types::BridgeReleaseUserEdit::from_core(edit))
        }

        /// Record the cover the user chose for a candidate. Nothing comes back:
        /// the per-candidate subscription delivers the pane's next value.
        fn set_candidate_cover(candidate_key: String, cover: BridgeCoverSelection) -> () {
            Ok(this
                .services
                .import_set_candidate_cover(&candidate_key, cover.into_core())
                .await?)
        }

        /// Record which surface the pane's metadata slot shows for a candidate,
        /// so clicking away and back lands on the same one.
        fn set_candidate_presentation(
            candidate_key: String,
            presentation: crate::types::BridgeMetadataPresentation,
        ) -> () {
            Ok(this
                .services
                .import_set_candidate_presentation(&candidate_key, presentation.into_core())
                .await?)
        }

        /// Record the typed-search form as the person left it.
        fn set_candidate_search_form(
            candidate_key: String,
            search: crate::types::BridgeSearchForm,
        ) -> () {
            Ok(this
                .services
                .import_set_candidate_search_form(&candidate_key, search.into_core())
                .await?)
        }

        /// Record the last command the pane ran for a candidate when it failed,
        /// or clear it for the next command.
        fn set_candidate_pane_error(candidate_key: String, error: Option<String>) -> () {
            Ok(this
                .services
                .import_set_candidate_pane_error(&candidate_key, error)
                .await?)
        }

        /// Record one album-level field of the candidate's metadata form as the
        /// user left it.
        fn set_candidate_edit_field(
            candidate_key: String,
            field: crate::types::BridgeCandidateEditField,
            value: String,
        ) -> () {
            Ok(this
                .services
                .import_set_candidate_edit_field(&candidate_key, field.into_core(), value)
                .await?)
        }

        /// Record one mapping-table row as the user left it.
        fn set_candidate_track_edit(
            candidate_key: String,
            track: crate::types::BridgeRawTrackEdit,
        ) -> () {
            Ok(this
                .services
                .import_set_candidate_track_edit(&candidate_key, track.into_core())
                .await?)
        }

        /// Set the same artist assignments on every named mapping-table row.
        fn set_candidate_track_artists(
            candidate_key: String,
            track_ids: Vec<String>,
            assignments: crate::types::BridgeTrackArtistAssignments,
        ) -> () {
            Ok(this
                .services
                .import_set_candidate_track_artists(
                    &candidate_key,
                    track_ids,
                    assignments.into_core(),
                )
                .await?)
        }

        /// Replace the candidate's ordered album-artist choices. Existing artists
        /// are carried by library ID; new artists carry their explicit metadata.
        fn set_candidate_album_artists(
            candidate_key: String,
            assignments: Vec<crate::types::BridgeArtistAssignment>,
        ) -> () {
            Ok(this
                .services
                .import_set_candidate_album_artists(
                    &candidate_key,
                    assignments
                        .into_iter()
                        .map(crate::types::BridgeArtistAssignment::into_core)
                        .collect(),
                )
                .await?)
        }

        /// Take one mapping-table row out of the import.
        fn drop_candidate_track(candidate_key: String, track_id: String) -> () {
            Ok(this
                .services
                .import_drop_candidate_track(&candidate_key, track_id)
                .await?)
        }

        /// Apply a user-supplied metadata edit (from the edit-metadata sheet) to a
        /// release. Writes the user's edited values directly without touching
        /// identity, metadata provenance, or cached source payloads.
        fn update_release_metadata_user_edit(
            release_id: String,
            edit: crate::types::BridgeReleaseUserEdit,
        ) -> () {
            let core_edit = crate::types::BridgeReleaseUserEdit::into_core(edit);
            this.services
                .apply_release_metadata_user_edit(&release_id, &core_edit)
                .await
                .map_err(BridgeError::import)
        }

        /// Seed the edit-metadata form from a library release's current metadata,
        /// including core's answer about whether its source can be projected again.
        /// bae-core does the projection; this is pure type translation.
        fn seed_release_edit(release_id: String) -> crate::types::BridgeReleaseEditSeed {
            let seed = this
                .services
                .release_edit_seed(&release_id)
                .await
                .map_err(BridgeError::import)?;
            Ok(crate::types::BridgeReleaseEditSeed::from_core(seed))
        }

        /// Re-project a release's metadata from its stored provenance. Returns the
        /// projected raw edit without writing — the editor populates its
        /// form with the result; the user re-edits or saves via
        /// `update_release_metadata_user_edit`. Identity and provenance are not
        /// touched.
        fn reset_release_edit_to_source(
            release_id: String,
        ) -> crate::types::BridgeRawReleaseEdit {
            let edit = this
                .services
                .reset_release_edit_to_source(&release_id)
                .await
                .map_err(BridgeError::import)?;
            Ok(crate::types::BridgeRawReleaseEdit::from_core(edit))
        }

        fn fetch_remote_covers(
            target: crate::types::BridgeCoverTarget,
        ) -> crate::types::BridgeRemoteCoverGallery {
            let covers = this
                .services
                .import_fetch_remote_covers(target.into_core())
                .await
                .map_err(BridgeError::import)?;
            Ok(crate::types::BridgeRemoteCoverGallery::from_core(covers))
        }

        /// Bytes of provider art at `url` — art from Cover Art Archive or Discogs
        /// that isn't in the library yet, so there is no image ref to read it by.
        /// Core owns the network: this is the only image fetch that leaves the
        /// device, and its byte cache is core's.
        ///
        /// `None` when the source serves no image at that address: cover addresses
        /// are derived from a release's ids, so an offered one can turn out to hold
        /// nothing. That is the slot having no image, not a failed load.
        fn fetch_remote_image_bytes(url: String) -> Option<Vec<u8>> {
            this.services
                .import_fetch_remote_image_bytes(url)
                .await
                .map(|image| image.map(|image| image.bytes))
                .map_err(BridgeError::import)
        }
    }
}

// =========================================================================
// Export (desktop-only)
// =========================================================================

// Gated on the target, not on `desktop`: bae-core gates `mod export` itself on the
// target, so the export queue and the track exporter exist on every non-mobile
// build whether or not this crate's `desktop` feature is on.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[uniffi::export]
impl AppHandle {
    pub fn subscribe_outputs(
        &self,
        callback: Box<dyn crate::types::OutputCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_watch(
            |services| services.subscribe_output_values(),
            move |value| {
                callback.on_value(crate::types::BridgeOutputSnapshot::from_core(value.clone()))
            },
        )
    }
}

forward! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    sync this => {
        /// The current export-queue snapshot.
        fn get_output_snapshot() -> crate::types::BridgeOutputSnapshot {
            crate::types::BridgeOutputSnapshot::from_core(this.services.output_snapshot())
        }

        /// Pause or resume the export queue. The in-flight export finishes; the queue
        /// stops starting new ones until resumed.
        fn set_outputs_paused(paused: bool) {
            this.services.set_outputs_paused(paused);
        }

        /// Cancel a release's export — drops a queued/failed entry or aborts the
        /// in-flight one (a partial copy never lands its destination file).
        fn cancel_output(release_id: String) {
            this.services.cancel_output(&release_id);
        }

        /// Retry every failed export now (flips them back to queued and wakes the
        /// worker).
        fn retry_outputs() {
            this.services.retry_outputs();
        }
    }
}

forward! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async this => {
        /// Enqueue a verbatim release export to `target_dir`. It joins the in-memory
        /// serial output queue; the worker drains it one release at a time. The
        /// storage-summary lookup (resolving the pane row's title/size) happens here;
        /// the deep cloud read + copy runs on the queue worker.
        fn enqueue_export(release_id: String, target_dir: String) -> () {
            this.services
                .enqueue_export(&release_id, std::path::PathBuf::from(target_dir))
                .await
                .map_err(BridgeError::export)
        }

        /// Enqueue a release-level save to `target_dir` under the preset named by
        /// `preset_id`. The preset is resolved and captured whole at enqueue time,
        /// so a later config edit can't change or break this queued save.
        fn enqueue_release_save(
            release_id: String,
            target_dir: String,
            preset_id: String,
        ) -> () {
            this.services
                .enqueue_release_save(
                    &release_id,
                    std::path::PathBuf::from(target_dir),
                    &preset_id,
                )
                .await
                .map_err(BridgeError::save)
        }

        /// Save one track to `output_path` under the preset named by `preset_id`
        /// (must apply to track saves). Always a constructed file — decoded, encoded
        /// to the preset codec, tagged, cover embedded — never a verbatim copy.
        fn save_track(track_id: String, output_path: String, preset_id: String) -> () {
            this.services
                .save_track(&track_id, std::path::Path::new(&output_path), &preset_id)
                .await
                .map_err(|e| BridgeError::save(format!("{e}")))
        }

        /// The default filename stem (no extension) a single-track "Save As…"
        /// suggests for `track_id` under the preset named by `preset_id`, rendered
        /// from that preset's token pattern. Reads only the database — no audio or
        /// cover — while seeding a save panel.
        fn save_track_suggested_name(track_id: String, preset_id: String) -> String {
            this.services
                .save_track_suggested_name(&track_id, &preset_id)
                .await
                .map_err(|e| BridgeError::save(format!("{e}")))
        }
    }
}
