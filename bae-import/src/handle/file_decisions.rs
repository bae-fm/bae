//! A candidate's file decisions: binding a track sheet's references to the
//! folder's audio and choosing what a sheet's disc is read as.

use super::*;

impl ImportServiceHandle {
    /// Tell surfaces a folder candidate's files were bound anew.
    pub(super) fn announce_source_candidate(
        &self,
        candidate: crate::folder_scanner::FolderCandidate,
    ) {
        self.services
            .event_tx
            .send(ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }));
    }

    /// For each FILE reference of the track sheet at `sheet_file_id`, the
    /// candidate's audio files, each offered or refused with the reason.
    ///
    /// Refusals are decided here from stored scan facts, so the picker never
    /// offers a file that [`Self::set_sheet_binding`] would reject.
    pub async fn sheet_binding_options(
        &self,
        candidate_key: crate::CandidateKey,
        sheet_file_id: String,
    ) -> Result<Vec<crate::folder_scanner::SheetReferenceOptions>, crate::ImportError> {
        let (files, _) = self.folder_files_for_binding(&candidate_key).await?;
        bae_util::task_failure::blocking("offer a sheet's audio bindings", move || {
            files.sheet_binding_options(&sheet_file_id)
        })
        .await
        .map_err(|failed| self.services.diagnostics.bug(failed).into())
    }

    /// Bind a FILE reference of one of a candidate's track sheets to an audio
    /// file, or clear the binding with `None`.
    ///
    /// Clearing does not restore the scan's guess: clearing says the guess was
    /// wrong. Only audio that [`Self::sheet_binding_options`] offers is
    /// accepted. The write also clears the stored identify verdict, because a
    /// binding changes the folder's tracks.
    pub async fn set_sheet_binding(
        &self,
        candidate_key: crate::CandidateKey,
        sheet_file_id: String,
        file_reference: String,
        audio_file_id: Option<String>,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        self.committed(async move {
            this.set_sheet_binding_write(
                candidate_key,
                sheet_file_id,
                file_reference,
                audio_file_id,
            )
            .await
        })
        .await
    }

    async fn set_sheet_binding_write(
        &self,
        candidate_key: crate::CandidateKey,
        sheet_file_id: String,
        file_reference: String,
        audio_file_id: Option<String>,
    ) -> Result<(), crate::ImportError> {
        use crate::folder_scanner::{SheetBindingOffer, UserSheetBinding};

        let (files, offered_revision) = self.folder_files_for_binding(&candidate_key).await?;
        let reference = files
            .sheet_binding_options(&sheet_file_id)
            .into_iter()
            .find(|reference| reference.file_reference == file_reference)
            .ok_or_else(|| crate::ImportError::SheetBinding {
                detail: format!("{sheet_file_id} has no FILE reference {file_reference}"),
            })?;
        if audio_file_id.is_some() && reference.file_id == audio_file_id {
            let _commit = self.services.folder_state_commit.lock("check a sheet binding").await;
            self.editable_candidate_for_commit(&candidate_key).await?;
            return Ok(());
        }
        let decision = match audio_file_id {
            None => UserSheetBinding::Cleared,
            Some(file_id) => {
                let offer = reference.options.iter().find(|option| option.file_id == file_id);
                if !matches!(offer.map(|option| &option.offer), Some(SheetBindingOffer::Offered)) {
                    return Err(crate::ImportError::SheetBinding {
                        detail: format!(
                            "{file_id} cannot supply {file_reference} in {sheet_file_id}"
                        ),
                    });
                }
                UserSheetBinding::Describes { file_id }
            }
        };
        self.write_file_edits(&candidate_key, files, offered_revision, |edits| {
            edits.sheet_bindings.set_reference(sheet_file_id, file_reference, decision);
        })
        .await
    }

    /// Set which disc of the release one of a candidate's track sheets holds,
    /// or take the sheet out of the tracklist with
    /// [`SheetDisc::Ignored`](crate::folder_scanner::SheetDisc::Ignored).
    ///
    /// Cue filenames are arbitrary (`CD1.cue` may hold disc two), so this is
    /// the person's decision. Like a binding, it clears the stored identify
    /// verdict. Giving a sheet a disc ignores every other sheet that shares its
    /// audio. Disc zero and numbers above `i32::MAX` are refused.
    pub async fn set_sheet_disc(
        &self,
        candidate_key: crate::CandidateKey,
        sheet_file_id: String,
        disc: crate::folder_scanner::SheetDisc,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        self.committed(async move {
            this.set_sheet_disc_write(candidate_key, sheet_file_id, disc).await
        })
        .await
    }

    async fn set_sheet_disc_write(
        &self,
        candidate_key: crate::CandidateKey,
        sheet_file_id: String,
        disc: crate::folder_scanner::SheetDisc,
    ) -> Result<(), crate::ImportError> {
        use crate::folder_scanner::SheetDisc;

        if matches!(disc, SheetDisc::Disc { number } if number == 0 || number > i32::MAX as u32) {
            return Err(crate::ImportError::SheetBinding {
                detail: format!("{sheet_file_id} has an invalid disc number"),
            });
        }
        let (files, offered_revision) = self.folder_files_for_binding(&candidate_key).await?;
        let Some(selected) =
            files.track_sheets().find(|sheet| sheet.file.relative_path == sheet_file_id)
        else {
            return Err(crate::ImportError::SheetBinding {
                detail: format!("{candidate_key} has no track sheet {sheet_file_id}"),
            });
        };
        // The menu fires even when the current disc is picked again; writing
        // would clear the verdict of a folder whose shape did not change.
        if selected.disc == disc {
            let _commit = self.services.folder_state_commit.lock("check a sheet disc").await;
            self.editable_candidate_for_commit(&candidate_key).await?;
            debug!("{sheet_file_id} is already disc {disc:?}; nothing to write");
            return Ok(());
        }

        let competing = if matches!(disc, SheetDisc::Disc { .. }) {
            if !selected.binding.is_resolved() {
                return Err(crate::ImportError::SheetBinding {
                    detail: format!("{sheet_file_id} cannot be selected until every FILE reference has usable audio"),
                });
            }
            let selected_audio = selected.binding.audio_files().expect("resolved sheet has audio");
            files
                .bound_sheets()
                .into_iter()
                .filter(|sheet| sheet.file.relative_path != sheet_file_id)
                .filter(|sheet| {
                    sheet.audio_files.iter().any(|(_, audio)| {
                        selected_audio.iter().any(|file| file.file_id == audio.relative_path)
                    })
                })
                .map(|sheet| sheet.file.relative_path.clone())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        self.write_file_edits(&candidate_key, files, offered_revision, |edits| {
            for other in competing {
                edits.sheet_discs.set(other, SheetDisc::Ignored);
            }
            edits.sheet_discs.set(sheet_file_id, disc);
        })
        .await
    }

    /// Add one file decision to the candidate's stored ones, apply it to every
    /// scanned candidate with these files, store the result, and announce it.
    ///
    /// The decision is applied to copies first, so one the folder cannot take
    /// (unreadable audio) fails with nothing written.
    async fn write_file_edits(
        &self,
        candidate_key: &crate::CandidateKey,
        files: crate::folder_scanner::CategorizedFiles,
        offered_revision: u64,
        decide: impl FnOnce(&mut crate::folder_scanner::CandidateFileEdits),
    ) -> Result<(), crate::ImportError> {
        // Settled files and the draft are prepared before the commit lock,
        // because tag reads can be slow and every pane control waits on this
        // lock.
        let content_hash = files.content_hash();
        let current_candidate = self.editable_candidate_for_commit(candidate_key).await?;
        let current_files = &current_candidate.files;
        let expected_revision = current_candidate.file_edit_revision;
        if current_files.content_hash() != content_hash || expected_revision != offered_revision {
            return Err(crate::CandidateAsRead::files_moved(
                offered_revision,
                crate::preparations::CandidateWrite::FileDecisions,
            ));
        }
        let preparation =
            crate::preparations::stored_preparation(&self.services.tables, &content_hash)
                .await?
                .filter(|preparation| preparation.file_edits.revision == expected_revision)
                .ok_or_else(|| crate::ImportError::Internal {
                    detail: format!(
                "{candidate_key} has no complete preparation for file revision {expected_revision}"
            ),
                })?;
        let mut edits = self.services.tables.load_candidate_file_edits(&content_hash).await?;
        if edits.revision != expected_revision {
            return Err(crate::ImportError::Internal {
                detail: format!(
                    "{candidate_key} file decisions changed from revision {expected_revision}"
                ),
            });
        }
        decide(&mut edits);

        loop {
            let matching_files = self
                .services
                .tables
                .load_releases_reading(&content_hash, expected_revision)
                .await?;
            let matching_keys = matching_files
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<std::collections::BTreeSet<_>>();
            let decided = edits.clone();
            let settled = bae_util::task_failure::blocking("apply a file decision", move || {
                let mut settled = Vec::with_capacity(matching_files.len());
                for (key, mut files) in matching_files {
                    files.apply_candidate_file_edits(&decided)?;
                    settled.push((key, files));
                }
                Ok::<_, crate::folder_scanner::UnreadableAudio>(settled)
            })
            .await
            .map_err(|failed| self.services.diagnostics.bug(failed))??;

            let settled_files = settled
                .iter()
                .find(|(key, _)| key == candidate_key)
                .map(|(_, files)| files)
                .ok_or_else(|| crate::ImportError::Internal {
                    detail: format!(
                        "file decision produced no settled candidate for {candidate_key}"
                    ),
                })?;
            let stored = self
                .services
                .tables
                .load_candidate_file_tag_snapshot(
                    &current_candidate.watched_folder_path,
                    candidate_key,
                )
                .await?
                .ok_or_else(|| crate::ImportError::Internal {
                    detail: format!("{candidate_key} has no scanned tag snapshot identity"),
                })?;
            let mut replacement = current_candidate.clone();
            replacement.files = settled_files.clone();
            let reads = crate::folder_scanner::FileReads::new(self.services.file_tags.clone());
            let initialized =
                bae_util::task_failure::blocking("read the files' track metadata", move || {
                    crate::file_metadata_seed::FileMetadataSeed::read(
                        &replacement,
                        stored.scan_generation,
                        &reads,
                        None,
                    )
                    .map(|seed| seed.draft)
                })
                .await
                .map_err(|failed| self.services.diagnostics.bug(failed))??;
            let draft = crate::pane::redraw_draft_for_files(
                current_files,
                settled_files,
                initialized,
                &preparation.metadata.draft,
                self.services.ids.as_ref(),
            );

            let commit = self
                .commit_lock_for_revision(
                    "store a file decision",
                    candidate_key,
                    &content_hash,
                    expected_revision,
                )
                .await?;
            // A scan may have added or dropped a folder with these files while
            // this was prepared; if so, prepare again.
            let matching_now = self
                .services
                .tables
                .load_releases_reading(&content_hash, expected_revision)
                .await?
                .into_iter()
                .map(|(key, _)| key)
                .collect::<std::collections::BTreeSet<_>>();
            if matching_now != matching_keys {
                drop(commit);
                continue;
            }
            // One write stores the decision and clears the verdict it
            // invalidates, and refuses a metadata revision that moved since it
            // was read.
            let (_next_revision, candidates) = self
                .services
                .preparations
                .store_file_decisions(
                    &crate::CandidateAsRead {
                        content_hash: content_hash.clone(),
                        file_edit_revision: expected_revision,
                        metadata_revision: preparation.metadata_revision,
                    },
                    candidate_key,
                    &edits,
                    &settled,
                    draft,
                )
                .await?;
            for candidate in candidates {
                self.announce_source_candidate(candidate);
            }
            return Ok(());
        }
    }

    /// The candidate's files and file-edit revision, or `None` when the key
    /// names no candidate that can be edited; callers word their own refusal.
    async fn actionable_candidate_files(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<Option<(crate::folder_scanner::CategorizedFiles, u64)>, crate::ImportError> {
        Ok(self
            .get_release_candidate(candidate_key)
            .await?
            .map(|candidate| (candidate.files, candidate.file_edit_revision)))
    }

    /// [`Self::actionable_candidate_files`] for a sheet operation, refusing a key
    /// that names no candidate.
    async fn folder_files_for_binding(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<(crate::folder_scanner::CategorizedFiles, u64), crate::ImportError> {
        self.actionable_candidate_files(candidate_key).await?.ok_or_else(|| {
            crate::ImportError::SheetBinding {
                detail: format!("{candidate_key} is not a folder candidate"),
            }
        })
    }
}

#[cfg(test)]
#[path = "file_decisions_tests.rs"]
mod tests;
