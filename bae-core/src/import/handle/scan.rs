use super::*;
use crate::util::rate_limiter::CallPriority;

impl ImportServiceHandle {
    /// Mark the candidate at `path` skipped or unskipped, persisting the change
    /// and broadcasting it so the import view re-tabs the row (New ↔ Skipped).
    /// A no-op request (already in the requested state) persists nothing and
    /// emits no event.
    pub async fn set_candidate_skipped(
        &self,
        path: String,
        skipped: bool,
    ) -> Result<(), crate::import::ImportError> {
        let _commit = self.folder_state_commit.lock().await;
        let Some(candidate) = self.get_release_candidate(&path).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{path} is not an actionable folder candidate"),
            });
        };
        let crate::import::release_candidate::ReleaseCandidate::Folder(candidate) = candidate
        else {
            if self
                .library_manager
                .set_combined_candidate_skipped(&path, skipped)
                .await?
            {
                send_event(
                    &self.event_tx,
                    ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                        candidate_key: path,
                        skipped,
                    }),
                );
            }
            return Ok(());
        };
        let watched_folder_path = candidate.watched_folder_path;
        let relative_candidate_path = crate::import::folder_registry::candidate_relative_path(
            &watched_folder_path,
            std::path::Path::new(&path),
        )?;
        let changed = self
            .library_manager
            .set_import_candidate_skipped(&watched_folder_path, &relative_candidate_path, skipped)
            .await?;
        if changed {
            self.folder_registry.lock().unwrap().apply_skipped(
                watched_folder_path,
                relative_candidate_path,
                skipped,
            );
            send_event(
                &self.event_tx,
                ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                    candidate_key: path,
                    skipped,
                }),
            );
        }
        Ok(())
    }

    /// What the track sheet at `sheet_file_id` can be bound to: the candidate's
    /// audio, each file either offered or refused with the reason.
    ///
    /// The refusals are decided here rather than by a UI reading codecs,
    /// because deciding them anywhere else means offering a file the commit
    /// would then reject — the failure an editable binding exists to remove.
    /// Deciding them costs one probe per audio file, which is why the set is
    /// asked for when a picker opens and not carried on every candidate.
    pub async fn sheet_binding_options(
        &self,
        candidate_key: String,
        sheet_file_id: String,
    ) -> Result<Vec<crate::import::folder_scanner::SheetBindingOption>, crate::import::ImportError>
    {
        let (files, _) = self.folder_files_for_binding(&candidate_key).await?;
        tokio::task::spawn_blocking(move || files.sheet_binding_options(&sheet_file_id))
            .await
            .map_err(|e| crate::import::ImportError::Internal {
                detail: format!("sheet binding option task failed: {e}"),
            })
    }

    /// Bind one of a candidate's track sheets to an audio file, or clear the
    /// binding by passing `None`.
    ///
    /// Clearing does **not** restore what the scan proposed. Someone who
    /// cleared a binding is saying the guess was wrong, so re-guessing it is
    /// the one answer that is certainly not what they asked for.
    ///
    /// The named audio must be one the sheet can actually use; the same
    /// offerable set the picker was built from is what decides, so a choice
    /// that would fail at commit is refused here instead.
    ///
    /// The decision is written before anything else changes, and writing it
    /// clears the candidate's stored identify verdict in the same statement:
    /// binding a sheet turns a one-track image into a twelve-track disc with a
    /// computable disc ID, so the verdict was an answer about a folder that no
    /// longer exists. The event that follows makes the view read the candidate
    /// again and the queue sweep identify it again.
    pub async fn set_sheet_binding(
        &self,
        candidate_key: String,
        sheet_file_id: String,
        audio_file_id: Option<String>,
    ) -> Result<(), crate::import::ImportError> {
        use crate::import::folder_scanner::{SheetBindingOffer, UserSheetBinding};

        let (files, offered_revision) = self.folder_files_for_binding(&candidate_key).await?;
        let Some(binding) = files
            .track_sheets()
            .find(|sheet| sheet.file.relative_path == sheet_file_id)
            .map(|sheet| sheet.binding)
        else {
            return Err(crate::import::ImportError::SheetBinding {
                detail: format!("{candidate_key} has no track sheet {sheet_file_id}"),
            });
        };
        // Same rule as `set_sheet_disc`: re-stating the binding in force
        // decides nothing, and must not clear the verdict. A sheet already on
        // exactly the one file asked for is in force whether the scan or the
        // user put it there.
        let already_in_force = match (binding.audio_files(), audio_file_id.as_deref()) {
            (Some(named), Some(requested)) => {
                matches!(named, [file] if file.file_id == requested)
            }
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        };
        if already_in_force {
            let _commit = self.folder_state_commit.lock().await;
            self.editable_candidate_for_commit(&candidate_key).await?;
            debug!("{sheet_file_id} already binds {audio_file_id:?}; nothing to write");
            return Ok(());
        }

        let decision = match audio_file_id {
            None => UserSheetBinding::Cleared,
            Some(audio_file_id) => {
                let sheet = sheet_file_id.clone();
                let audio = audio_file_id.clone();
                let offered = files.clone();
                let offer = tokio::task::spawn_blocking(move || {
                    offered
                        .sheet_binding_options(&sheet)
                        .into_iter()
                        .find(|option| option.file_id == audio)
                        .map(|option| option.offer)
                })
                .await
                .map_err(|e| crate::import::ImportError::Internal {
                    detail: format!("sheet binding option task failed: {e}"),
                })?;
                match offer {
                    Some(SheetBindingOffer::Offered) => {}
                    Some(SheetBindingOffer::RefusedCodec { codec }) => {
                        return Err(crate::import::ImportError::SheetBinding {
                            detail: format!("{audio_file_id} is {codec}"),
                        })
                    }
                    Some(SheetBindingOffer::RefusedTiming) => {
                        return Err(crate::import::ImportError::SheetBinding {
                            detail: format!("{sheet_file_id} has timings outside {audio_file_id}"),
                        })
                    }
                    Some(SheetBindingOffer::RefusedUnreadable) => {
                        return Err(crate::import::ImportError::SheetBinding {
                            detail: format!("{audio_file_id} cannot be read"),
                        })
                    }
                    None => {
                        return Err(crate::import::ImportError::SheetBinding {
                            detail: format!("{audio_file_id} is not this sheet's to name"),
                        })
                    }
                }
                UserSheetBinding::Describes {
                    file_id: audio_file_id,
                }
            }
        };

        self.write_file_edits(&candidate_key, files, offered_revision, |edits| {
            edits.sheet_bindings.set(sheet_file_id, decision);
        })
        .await
    }

    /// Say which disc of the release one of a candidate's track sheets holds,
    /// or take the sheet out of the tracklist with
    /// [`SheetDisc::Ignored`](crate::import::folder_scanner::SheetDisc::Ignored).
    ///
    /// Cue filenames are arbitrary — `CD1.cue` may hold disc two — so which
    /// disc a sheet carves is a decision rather than something read off a name.
    /// Like a binding it is stored with the candidate and it clears the stored
    /// identify verdict: re-assigning a sheet re-shapes the tracklist, and
    /// ignoring one hands its container back to the release as loose audio.
    ///
    /// Discs count from one, so disc zero is refused: there is no such disc to
    /// put the sheet's entries on.
    pub async fn set_sheet_disc(
        &self,
        candidate_key: String,
        sheet_file_id: String,
        disc: crate::import::folder_scanner::SheetDisc,
    ) -> Result<(), crate::import::ImportError> {
        use crate::import::folder_scanner::SheetDisc;

        if let SheetDisc::Disc { number: 0 } = disc {
            return Err(crate::import::ImportError::SheetBinding {
                detail: format!("{sheet_file_id} cannot be disc zero; discs count from one"),
            });
        }
        let (files, offered_revision) = self.folder_files_for_binding(&candidate_key).await?;
        let Some(in_force) = files
            .track_sheets()
            .find(|sheet| sheet.file.relative_path == sheet_file_id)
            .map(|sheet| sheet.disc)
        else {
            return Err(crate::import::ImportError::SheetBinding {
                detail: format!("{candidate_key} has no track sheet {sheet_file_id}"),
            });
        };
        // Re-stating the disc the sheet already holds decides nothing — the
        // menu fires on every selection, including of the current item — and
        // a write here would clear the stored verdict and re-identify a
        // folder whose shape did not change.
        if in_force == disc {
            let _commit = self.folder_state_commit.lock().await;
            self.editable_candidate_for_commit(&candidate_key).await?;
            debug!("{sheet_file_id} is already disc {disc:?}; nothing to write");
            return Ok(());
        }

        self.write_file_edits(&candidate_key, files, offered_revision, |edits| {
            edits.sheet_discs.set(sheet_file_id, disc);
        })
        .await
    }

    /// Replace the candidate draft with metadata from an explicitly chosen
    /// source. The projection completes before the database transaction, which
    /// replaces the draft and provenance together while leaving all physical
    /// file and track decisions untouched.
    pub(crate) fn external_candidate_draft(
        &self,
        payloads: &crate::import::payloads::ReleasePayloads,
        files: &crate::import::folder_scanner::CategorizedFiles,
        durations: &crate::import::probe::SourceDurations,
    ) -> Result<crate::import::pane::CandidateSourceDraft, crate::import::ImportError> {
        let pane = crate::import::pane::release_pane(
            payloads,
            files,
            durations,
            &crate::import::CandidateEditOverlay::default(),
            &[],
            self.clock.as_ref(),
            self.ids.as_ref(),
        )?;
        Ok(crate::import::pane::candidate_draft_from_source(pane))
    }

    /// Project one external release and prepare every provider image its
    /// candidate revision owns before the revision is written.
    pub(crate) async fn external_candidate_metadata(
        &self,
        payloads: &crate::import::payloads::ReleasePayloads,
        files: &crate::import::folder_scanner::CategorizedFiles,
        durations: &crate::import::probe::SourceDurations,
        provenance: crate::import::MetadataProvenance,
        fallback_cover: Option<&crate::import::CoverSelection>,
    ) -> Result<crate::import::CandidateMetadataDraft, crate::import::ImportError> {
        let source_draft = self.external_candidate_draft(payloads, files, durations)?;
        let edit = source_draft.edit;
        let track_mappings = source_draft.track_mappings;
        let source_discogs_artist_ids = source_draft.source_discogs_artist_ids;
        let required_artist_ids = source_discogs_artist_ids
            .union(&source_draft.mapped_new_discogs_artist_ids)
            .cloned()
            .collect();
        let artist_images = self
            .library_manager
            .prepare_discogs_artist_images(required_artist_ids)
            .await?;
        let (cover, remote_cover) = match payloads.default_cover()? {
            Some(remote) => match self.library_manager.fetch_remote_image(&remote.url).await? {
                Some(image) => (
                    Some(crate::import::CoverSelection::Remote(
                        remote.url,
                        remote.source,
                    )),
                    Some(image),
                ),
                None => (local_or_embedded_cover(fallback_cover), None),
            },
            None => (local_or_embedded_cover(fallback_cover), None),
        };
        Ok(crate::import::CandidateMetadataDraft {
            edit,
            track_mappings,
            source_discogs_artist_ids,
            provenance: Some(provenance),
            cover,
            assets: crate::import::CandidatePreparedAssets {
                remote_cover,
                artist_images,
            },
        })
    }

    pub(crate) async fn set_candidate_metadata_provenance(
        &self,
        candidate_key: String,
        provenance: crate::import::MetadataProvenance,
    ) -> Result<u64, crate::import::ImportError> {
        let Some(candidate) = self.get_release_candidate(&candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        let content_hash = candidate.files().content_hash();
        let current = self
            .library_manager
            .load_import_candidate_preparation(&content_hash)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no stored import preparation"),
            })?;
        let expected_metadata_revision = current.metadata_revision;
        let durations = crate::import::probe::source_durations(candidate.files())?;
        match &provenance {
            crate::import::MetadataProvenance::FileTags => {
                let (snapshot_candidate, snapshot) = self.file_tag_snapshot(&candidate_key).await?;
                let pane = crate::import::pane::file_tags_pane(
                    &snapshot_candidate,
                    &snapshot,
                    &durations,
                    &crate::import::CandidateEditOverlay::default(),
                    &[],
                    self.clock.as_ref(),
                    self.ids.as_ref(),
                )?;
                let source_draft = crate::import::pane::candidate_draft_from_source(pane);
                let track_mappings = crate::import::edits::preserve_track_mapping_decisions(
                    source_draft.track_mappings,
                    &current.track_mappings,
                );
                let cover = crate::import::file_tag_snapshot::embedded_cover_selection(&snapshot);
                let _commit = self.folder_state_commit.lock().await;
                self.editable_candidate_revision_for_commit(
                    &candidate_key,
                    &content_hash,
                    current.file_edit_revision,
                )
                .await?;
                return Ok(self
                    .library_manager
                    .replace_candidate_file_tags_metadata(
                        snapshot_candidate.watched_folder_path(),
                        &candidate_key,
                        &content_hash,
                        current.file_edit_revision,
                        expected_metadata_revision,
                        &snapshot,
                        &source_draft.edit,
                        &track_mappings,
                        cover.as_ref(),
                    )
                    .await?);
            }
            crate::import::MetadataProvenance::ExternalRelease {
                source,
                release_id,
                partners,
            } => {
                let primary = crate::import::MetadataRef::new(release_id.clone(), *source);
                let payloads = self
                    .payloads_for_provenance(&candidate_key, &primary)
                    .await?;
                // Every source the pick claims has to read offline afterwards:
                // import and re-identify both open the partner's own document
                // for the identity it states. This runs before the provenance
                // is written, so a partner that will not prepare leaves the
                // candidate with the draft and pick it already had.
                crate::import::service::prepare_partners(
                    &self.library_manager,
                    &primary,
                    partners,
                    CallPriority::Interactive,
                )
                .await?;
                let mut metadata = self
                    .external_candidate_metadata(
                        &payloads,
                        candidate.files(),
                        &durations,
                        provenance.clone(),
                        current.cover.as_ref(),
                    )
                    .await?;
                metadata.track_mappings = crate::import::edits::preserve_track_mapping_decisions(
                    metadata.track_mappings,
                    &current.track_mappings,
                );
                let _commit = self.folder_state_commit.lock().await;
                self.editable_candidate_revision_for_commit(
                    &candidate_key,
                    &content_hash,
                    current.file_edit_revision,
                )
                .await?;
                return Ok(self
                    .library_manager
                    .replace_candidate_metadata_prepared(
                        candidate.watched_folder_path(),
                        &content_hash,
                        &candidate_key,
                        candidate.file_edit_revision(),
                        expected_metadata_revision,
                        &metadata,
                    )
                    .await?);
            }
        }
    }

    /// Clear source metadata while retaining the candidate's physical layout
    /// and every explicit mapping decision.
    pub(crate) async fn clear_candidate_metadata(
        &self,
        candidate_key: String,
    ) -> Result<u64, crate::import::ImportError> {
        let Some(candidate) = self.get_release_candidate(&candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        let content_hash = candidate.files().content_hash();
        let current = self
            .library_manager
            .load_import_candidate_preparation(&content_hash)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no stored import preparation"),
            })?;
        let source_draft = candidate.blank_source();
        let track_mappings = crate::import::edits::preserve_track_mapping_decisions(
            source_draft.track_mappings,
            &current.track_mappings,
        );
        let _commit = self.folder_state_commit.lock().await;
        self.editable_candidate_revision_for_commit(
            &candidate_key,
            &content_hash,
            current.file_edit_revision,
        )
        .await?;
        Ok(self
            .library_manager
            .replace_candidate_metadata_prepared(
                candidate.watched_folder_path(),
                &content_hash,
                &candidate_key,
                candidate.file_edit_revision(),
                current.metadata_revision,
                &crate::import::CandidateMetadataDraft {
                    edit: source_draft.edit,
                    track_mappings,
                    source_discogs_artist_ids: Default::default(),
                    provenance: None,
                    cover: None,
                    assets: crate::import::CandidatePreparedAssets::default(),
                },
            )
            .await?)
    }

    /// Tell the surfaces a candidate's metadata provenance changed.
    pub(crate) fn announce_metadata_provenance(&self, candidate_key: String) {
        send_event(
            &self.event_tx,
            ImportEvent::Scan(ScanEvent::CandidateMetadataChanged { candidate_key }),
        );
    }

    /// Put one of a candidate's files in a role, or put it back in the one the
    /// scan proposed.
    ///
    /// Only a file the scan read as playable audio can move, and only between
    /// being one of the release's tracks and not being one. That is the whole
    /// set of role changes with a consequence: an image is an image, and a
    /// track sheet's job is decided by what it is bound to, which
    /// [`Self::set_sheet_binding`] already owns.
    ///
    /// Taking a file out does **not** take it out of the release. The folder is
    /// the release, so the file still imports, uploads, and comes back on
    /// export — it just stops being one of the tracks, which is also why the
    /// content hash this decision is stored under does not move for it.
    ///
    /// Taking out the last audio the folder has is refused: there would be
    /// nothing left to import, and a release with no tracks is not a state the
    /// rest of the import can describe.
    pub async fn set_file_role(
        &self,
        candidate_key: String,
        file_id: String,
        choice: crate::import::folder_scanner::FileRoleChoice,
    ) -> Result<(), crate::import::ImportError> {
        let Some((files, offered_revision)) =
            self.actionable_candidate_files(&candidate_key).await?
        else {
            return Err(crate::import::ImportError::FileRole {
                detail: format!("{candidate_key} is not a folder candidate"),
            });
        };
        let Some(entry) = files
            .files
            .iter()
            .find(|entry| entry.file.relative_path == file_id)
        else {
            return Err(crate::import::ImportError::FileRole {
                detail: format!("{candidate_key} has no file {file_id}"),
            });
        };
        if !entry.role_alternatives().contains(&choice) {
            return Err(crate::import::ImportError::FileRole {
                detail: format!("{file_id} is not the folder's audio"),
            });
        }
        // Same rule as `set_sheet_disc`: re-stating the role in force decides
        // nothing, and must not clear the verdict.
        if entry.role_choice() == Some(choice) {
            let _commit = self.folder_state_commit.lock().await;
            self.editable_candidate_for_commit(&candidate_key).await?;
            debug!("{file_id} is already {choice:?}; nothing to write");
            return Ok(());
        }

        self.write_file_edits(&candidate_key, files, offered_revision, |edits| {
            edits.file_roles.set(file_id, choice);
        })
        .await
    }

    /// Add one decision to what the user has settled about a candidate's files,
    /// apply the result, and publish it.
    ///
    /// Applied to a **copy** first: a decision that turns out not to survive
    /// contact with the folder — audio that has gone unreadable since the offer,
    /// or a change that would leave the release with no tracks — leaves the
    /// candidate exactly as it was, with nothing written.
    async fn write_file_edits(
        &self,
        candidate_key: &str,
        files: crate::import::folder_scanner::CategorizedFiles,
        offered_revision: u64,
        decide: impl FnOnce(&mut crate::import::folder_scanner::CandidateFileEdits),
    ) -> Result<(), crate::import::ImportError> {
        let _commit = self.folder_state_commit.lock().await;
        let content_hash = files.content_hash();
        let crate::import::release_candidate::ReleaseCandidate::Folder(current_candidate) =
            self.editable_candidate_for_commit(candidate_key).await?
        else {
            return Err(crate::import::ImportError::FileRole {
                detail: "separate the folders before changing their file roles or CUE bindings"
                    .into(),
            });
        };
        let current_files = current_candidate.files;
        let expected_revision = current_candidate.file_edit_revision;
        if current_files.content_hash() != content_hash {
            return Err(crate::import::ImportError::FileRole {
                detail: format!("{candidate_key} changed before its file decision was written"),
            });
        }
        if expected_revision != offered_revision {
            return Err(crate::import::ImportError::FileRole {
                detail: format!("{candidate_key} file decisions changed before the write"),
            });
        }
        let preparation = self
            .library_manager
            .load_import_candidate_preparation(&content_hash)
            .await?
            .filter(|preparation| preparation.file_edit_revision == expected_revision)
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!(
                    "{candidate_key} has no complete preparation for file revision {expected_revision}"
                ),
            })?;
        let mut edits = self
            .library_manager
            .load_candidate_file_edits(&content_hash)
            .await?;
        if edits.revision != expected_revision {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "{candidate_key} file decisions changed from revision {expected_revision}"
                ),
            });
        }
        decide(&mut edits);

        let matching_files = crate::import::candidates::files_for_identity(
            &self.library_manager.load_all_folder_scan_items().await?,
            &content_hash,
            expected_revision,
        );
        let (settled, edits) = tokio::task::spawn_blocking(move || {
            let mut settled = Vec::with_capacity(matching_files.len());
            for (key, mut files) in matching_files {
                files.apply_candidate_file_edits(&edits)?;
                settled.push((key, files));
            }
            Ok::<_, crate::import::folder_scanner::InvalidReason>((settled, edits))
        })
        .await
        .map_err(|e| crate::import::ImportError::Internal {
            detail: format!("candidate file edit task failed: {e}"),
        })??;

        let settled_files = settled
            .iter()
            .find(|(key, _)| key == candidate_key)
            .map(|(_, files)| files)
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("file decision produced no settled candidate for {candidate_key}"),
            })?;
        let proposed_mappings = crate::import::pane::automatic_mappings_for_draft(
            settled_files,
            &crate::import::probe::SourceDurations::default(),
            preparation.metadata_draft.clone(),
            &preparation.track_mappings,
            preparation.metadata_provenance.as_ref(),
        )?;
        let available_files = crate::import::track_slots::units_of(
            &crate::import::track_slots::audio_layout(settled_files),
        )
        .into_iter()
        .collect();
        let track_mappings = crate::import::edits::reconcile_track_mapping_decisions(
            proposed_mappings,
            &preparation.track_mappings,
            &available_files,
        );
        let active = crate::import::edits::apply_track_mappings_to_draft(
            preparation.metadata_draft,
            &track_mappings,
        )?;
        let (source_discogs_artist_ids, artist_images) = self
            .prepared_artist_images_for_active(
                candidate_key,
                settled_files,
                preparation.metadata_provenance.as_ref(),
                &active,
                preparation.assets.artist_images,
            )
            .await?;
        let mapping_preparation = crate::import::CandidateMappingPreparation {
            track_mappings,
            source_discogs_artist_ids,
            artist_images,
        };

        // Durable first, and atomically: the decision and the verdict it
        // invalidates move together, so nothing can observe a folder whose
        // stored answer describes the shape it just stopped having.
        let (_next_revision, candidates) = self
            .library_manager
            .save_import_candidate_file_edits(
                &content_hash,
                candidate_key,
                expected_revision,
                preparation.metadata_revision,
                &edits,
                &settled,
                &mapping_preparation,
            )
            .await?;
        for candidate in candidates {
            send_event(
                &self.event_tx,
                ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }),
            );
        }
        Ok(())
    }

    /// A scanned folder candidate's files, read by key, or `None` for a key
    /// that names no folder — an invalid candidate has no roles or bindings to
    /// edit. Each caller names the refusal in its own terms rather than
    /// borrowing the other's.
    pub(super) async fn actionable_candidate_files(
        &self,
        candidate_key: &str,
    ) -> Result<
        Option<(crate::import::folder_scanner::CategorizedFiles, u64)>,
        crate::import::ImportError,
    > {
        Ok(match self.get_release_candidate(candidate_key).await? {
            Some(crate::import::release_candidate::ReleaseCandidate::Folder(candidate)) => {
                Some((candidate.files, candidate.file_edit_revision))
            }
            _ => None,
        })
    }

    /// A folder candidate's files for a binding operation, or the refusal that
    /// names what the key resolved to instead.
    async fn folder_files_for_binding(
        &self,
        candidate_key: &str,
    ) -> Result<(crate::import::folder_scanner::CategorizedFiles, u64), crate::import::ImportError>
    {
        self.actionable_candidate_files(candidate_key)
            .await?
            .ok_or_else(|| crate::import::ImportError::SheetBinding {
                detail: format!("{candidate_key} is not a folder candidate"),
            })
    }

    /// Subscribe to the unified event channel, filtered to only `ScanEvent`s.
    pub fn subscribe_folder_scan_events(&self) -> mpsc::UnboundedReceiver<ScanEvent> {
        let mut rx = self.event_tx.subscribe();
        let (tx, out_rx) = mpsc::unbounded_channel();
        let library_manager = self.library_manager.clone();
        self.runtime_handle.spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if tx.is_closed() {
                            break;
                        }
                        if let ImportEvent::Scan(event) = event {
                            if tx.send(event).is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Scan event subscriber lagged by {n} events");
                        library_manager.record_telemetry(
                            crate::diagnostics::TelemetryEvent::Anomaly {
                                kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                            },
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        out_rx
    }
}

fn local_or_embedded_cover(
    cover: Option<&crate::import::CoverSelection>,
) -> Option<crate::import::CoverSelection> {
    match cover {
        Some(
            cover @ (crate::import::CoverSelection::Local(_)
            | crate::import::CoverSelection::Embedded(_)),
        ) => Some(cover.clone()),
        Some(crate::import::CoverSelection::Remote(_, _)) | None => None,
    }
}

#[cfg(test)]
mod cover_fallback_tests {
    use super::*;

    #[test]
    fn a_source_without_a_remote_cover_keeps_the_local_selection() {
        let selected = crate::import::CoverSelection::Local("cover.jpg".to_string());

        assert_eq!(local_or_embedded_cover(Some(&selected)), Some(selected));
    }

    #[test]
    fn a_source_without_a_remote_cover_does_not_reuse_an_old_remote_selection() {
        let selected = crate::import::CoverSelection::Remote(
            "https://example.invalid/old".to_string(),
            crate::import::MetadataSource::Discogs,
        );

        assert_eq!(local_or_embedded_cover(Some(&selected)), None);
    }
}
