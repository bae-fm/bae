use super::*;
use bae_catalogs::rate_limiter::CallPriority;

impl ImportServiceHandle {
    /// Tell surfaces a folder candidate's files were bound anew.
    pub(super) fn announce_source_candidate(
        &self,
        candidate: crate::import::folder_scanner::FolderCandidate,
    ) {
        self.event_tx
            .send(ImportEvent::Scan(ScanEvent::CandidateBindingChanged {
                candidate,
            }));
    }

    /// Mark the candidate at `candidate_key` skipped or unskipped and
    /// announce the change; a request that changes nothing writes nothing and
    /// sends no event.
    ///
    /// Skipping ends the candidate's identification, since the person has set
    /// it aside.
    pub async fn set_candidate_skipped(
        &self,
        candidate_key: crate::import::CandidateKey,
        skipped: bool,
    ) -> Result<(), crate::import::ImportError> {
        let this = self.clone();
        self.committed(async move {
            this.set_candidate_skipped_write(candidate_key, skipped)
                .await
        })
        .await
    }

    async fn set_candidate_skipped_write(
        &self,
        candidate_key: crate::import::CandidateKey,
        skipped: bool,
    ) -> Result<(), crate::import::ImportError> {
        let _commit = self.folder_state_commit.lock("skip a candidate").await;
        let Some(candidate) = self.get_release_candidate(&candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        if candidate.grouping.is_some() {
            if self
                .library_manager
                .set_grouping_skipped(&candidate_key, skipped)
                .await?
            {
                if skipped {
                    self.identification_ended(&candidate_key).await;
                }
                self.event_tx
                    .send(ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                        candidate_key,
                        skipped,
                    }));
            }
            return Ok(());
        }
        let changed = self
            .library_manager
            .set_import_candidate_skipped(&candidate.path, skipped)
            .await?;
        if changed {
            if skipped {
                self.identification_ended(&candidate_key).await;
            }
            self.event_tx
                .send(ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                    candidate_key,
                    skipped,
                }));
        }
        Ok(())
    }

    /// For each FILE reference of the track sheet at `sheet_file_id`, the
    /// candidate's audio files, each offered or refused with the reason.
    ///
    /// Refusals are decided here from stored scan facts, so the picker never
    /// offers a file that [`Self::set_sheet_binding`] would reject.
    pub async fn sheet_binding_options(
        &self,
        candidate_key: crate::import::CandidateKey,
        sheet_file_id: String,
    ) -> Result<Vec<crate::import::folder_scanner::SheetReferenceOptions>, crate::import::ImportError>
    {
        let (files, _) = self.folder_files_for_binding(&candidate_key).await?;
        tokio::task::spawn_blocking(move || files.sheet_binding_options(&sheet_file_id))
            .await
            .map_err(|e| crate::import::ImportError::Internal {
                detail: format!("sheet binding option task failed: {e}"),
            })
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
        candidate_key: crate::import::CandidateKey,
        sheet_file_id: String,
        file_reference: String,
        audio_file_id: Option<String>,
    ) -> Result<(), crate::import::ImportError> {
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
        candidate_key: crate::import::CandidateKey,
        sheet_file_id: String,
        file_reference: String,
        audio_file_id: Option<String>,
    ) -> Result<(), crate::import::ImportError> {
        use crate::import::folder_scanner::{SheetBindingOffer, UserSheetBinding};

        let (files, offered_revision) = self.folder_files_for_binding(&candidate_key).await?;
        let reference = files
            .sheet_binding_options(&sheet_file_id)
            .into_iter()
            .find(|reference| reference.file_reference == file_reference)
            .ok_or_else(|| crate::import::ImportError::SheetBinding {
                detail: format!("{sheet_file_id} has no FILE reference {file_reference}"),
            })?;
        if audio_file_id.is_some() && reference.file_id == audio_file_id {
            let _commit = self.folder_state_commit.lock("check a sheet binding").await;
            self.editable_candidate_for_commit(&candidate_key).await?;
            return Ok(());
        }
        let decision = match audio_file_id {
            None => UserSheetBinding::Cleared,
            Some(file_id) => {
                let offer = reference
                    .options
                    .iter()
                    .find(|option| option.file_id == file_id);
                if !matches!(
                    offer.map(|option| &option.offer),
                    Some(SheetBindingOffer::Offered)
                ) {
                    return Err(crate::import::ImportError::SheetBinding {
                        detail: format!(
                            "{file_id} cannot supply {file_reference} in {sheet_file_id}"
                        ),
                    });
                }
                UserSheetBinding::Describes { file_id }
            }
        };
        self.write_file_edits(&candidate_key, files, offered_revision, |edits| {
            edits
                .sheet_bindings
                .set_reference(sheet_file_id, file_reference, decision);
        })
        .await
    }

    /// Set which disc of the release one of a candidate's track sheets holds,
    /// or take the sheet out of the tracklist with
    /// [`SheetDisc::Ignored`](crate::import::folder_scanner::SheetDisc::Ignored).
    ///
    /// Cue filenames are arbitrary (`CD1.cue` may hold disc two), so this is
    /// the person's decision. Like a binding, it clears the stored identify
    /// verdict. Giving a sheet a disc ignores every other sheet that shares its
    /// audio. Disc zero and numbers above `i32::MAX` are refused.
    pub async fn set_sheet_disc(
        &self,
        candidate_key: crate::import::CandidateKey,
        sheet_file_id: String,
        disc: crate::import::folder_scanner::SheetDisc,
    ) -> Result<(), crate::import::ImportError> {
        let this = self.clone();
        self.committed(async move {
            this.set_sheet_disc_write(candidate_key, sheet_file_id, disc)
                .await
        })
        .await
    }

    async fn set_sheet_disc_write(
        &self,
        candidate_key: crate::import::CandidateKey,
        sheet_file_id: String,
        disc: crate::import::folder_scanner::SheetDisc,
    ) -> Result<(), crate::import::ImportError> {
        use crate::import::folder_scanner::SheetDisc;

        if matches!(disc, SheetDisc::Disc { number } if number == 0 || number > i32::MAX as u32) {
            return Err(crate::import::ImportError::SheetBinding {
                detail: format!("{sheet_file_id} has an invalid disc number"),
            });
        }
        let (files, offered_revision) = self.folder_files_for_binding(&candidate_key).await?;
        let Some(selected) = files
            .track_sheets()
            .find(|sheet| sheet.file.relative_path == sheet_file_id)
        else {
            return Err(crate::import::ImportError::SheetBinding {
                detail: format!("{candidate_key} has no track sheet {sheet_file_id}"),
            });
        };
        // The menu fires even when the current disc is picked again; writing
        // would clear the verdict of a folder whose shape did not change.
        if selected.disc == disc {
            let _commit = self.folder_state_commit.lock("check a sheet disc").await;
            self.editable_candidate_for_commit(&candidate_key).await?;
            debug!("{sheet_file_id} is already disc {disc:?}; nothing to write");
            return Ok(());
        }

        let competing = if matches!(disc, SheetDisc::Disc { .. }) {
            if !selected.binding.is_resolved() {
                return Err(crate::import::ImportError::SheetBinding {
                    detail: format!("{sheet_file_id} cannot be selected until every FILE reference has usable audio"),
                });
            }
            let selected_audio = selected
                .binding
                .audio_files()
                .expect("resolved sheet has audio");
            files
                .bound_sheets()
                .into_iter()
                .filter(|sheet| sheet.file.relative_path != sheet_file_id)
                .filter(|sheet| {
                    sheet.audio_files.iter().any(|(_, audio)| {
                        selected_audio
                            .iter()
                            .any(|file| file.file_id == audio.relative_path)
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

    /// The candidate draft a release's reading writes, keeping each current
    /// track on its file. A reading of tracks lays every value it states over
    /// the folder's audio; a release that lists none leaves the draft's tracks
    /// as they are. Where the release states no year, the pressing's is
    /// `pressing_year`, the folder's — see
    /// `identify::row_facts::folder_pressing_year`.
    pub(crate) fn external_candidate_draft(
        values: crate::import::release_values::ReleaseValues,
        current: &crate::import::CandidateDraft,
        pressing_year: Option<i32>,
    ) -> Result<crate::import::CandidateDraft, crate::import::ImportError> {
        let mut draft = if values.tracks.is_empty() {
            crate::import::pane::album_over_draft(values, current)
        } else {
            crate::import::pane::values_over_audio(values, current)?
        };
        if draft.pressing.year.trim().is_empty() {
            if let Some(year) = pressing_year {
                draft.pressing.year = year.to_string();
            }
        }
        Ok(draft)
    }

    /// Build the metadata a release's reading writes and fetch the cover it
    /// is read with, before anything is written. A fetch that fails fails the
    /// whole with its error.
    pub(crate) async fn external_candidate_metadata(
        &self,
        release: &crate::import::source_release::SourceRelease,
        values: crate::import::release_values::ReleaseValues,
        partners: Vec<crate::import::source_release::SourceRelease>,
        current: &crate::import::CandidateDraft,
        pressing_year: Option<i32>,
    ) -> Result<crate::import::CandidateMetadataDraft, crate::import::ImportError> {
        let draft = Self::external_candidate_draft(values, current, pressing_year)?;
        let cover = self
            .fetch_release_cover(release, &partners)
            .await
            .map_err(|failure| failure.error)?;
        Ok(external_metadata_with_cover(
            draft, release, &partners, cover,
        ))
    }

    /// The cover a draft read from an external release is written with: the
    /// first one the release or the partners picked with it offer, with its
    /// bytes, fetched from the catalog that offers it.
    ///
    /// A catalog with nothing at that address leaves the cover `None`, and
    /// the write keeps the candidate's current cover; a fetch that fails
    /// fails, naming the catalog that offered the cover.
    pub(crate) async fn fetch_release_cover(
        &self,
        release: &crate::import::source_release::SourceRelease,
        partners: &[crate::import::source_release::SourceRelease],
    ) -> Result<Option<FetchedCover>, CoverFetchFailure> {
        let Some(remote) = crate::import::source_release::pick_covers(release, partners)
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        let image = self
            .library_manager
            .fetch_remote_image(&remote.image.url)
            .await
            .map_err(|error| CoverFetchFailure {
                source: remote.source,
                error,
            })?;
        Ok(image.map(|image| (remote, image)))
    }

    /// The candidate's stored preparation, for a write about to replace its
    /// draft.
    pub(super) async fn metadata_write_base(
        &self,
        candidate_key: &crate::import::CandidateKey,
    ) -> Result<
        (
            crate::import::folder_scanner::FolderCandidate,
            crate::db::DbCandidateImportPreparation,
        ),
        crate::import::ImportError,
    > {
        let Some(candidate) = self.get_release_candidate(candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        let current = self
            .library_manager
            .load_import_candidate_preparation(&candidate.files.content_hash())
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no stored import preparation"),
            })?;
        Ok((candidate, current))
    }

    /// Read the candidate's draft from its files' own tags. The release link
    /// stays as it is.
    pub(super) async fn read_candidate_file_tags_write(
        &self,
        candidate_key: crate::import::CandidateKey,
    ) -> Result<u64, crate::import::ImportError> {
        let (candidate, current) = self.metadata_write_base(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        loop {
            // Read tags before taking the commit lock: a network share can be
            // slow, and every pane control waits on this lock.
            let read = self
                .read_file_tag_snapshot(&candidate_key, self.file_tags.clone())
                .await?;
            let seed = crate::import::file_metadata_seed::FileMetadataSeed::project(
                &read.candidate,
                read.snapshot,
                Some(&current.draft.tracks),
            )?;
            let commit = self
                .commit_lock_for_revision(
                    "pick file tags",
                    &candidate_key,
                    &content_hash,
                    current.file_edit_revision,
                )
                .await?;
            // A scan that stored the candidate while its tags were read gave it
            // a newer generation, and the write only accepts tags read at the
            // current one: read again.
            let stored_generation = self
                .library_manager
                .load_candidate_file_tag_snapshot(
                    &read.candidate.watched_folder_path,
                    &candidate_key,
                )
                .await?
                .map(|stored| stored.scan_generation);
            if stored_generation != Some(seed.snapshot.scan_generation) {
                drop(commit);
                continue;
            }
            return Ok(self
                .preparations
                .apply_file_metadata(
                    &read.candidate.watched_folder_path,
                    &candidate_key,
                    &crate::import::CandidateAsRead {
                        content_hash: content_hash.clone(),
                        file_edit_revision: current.file_edit_revision,
                        metadata_revision: current.metadata_revision,
                    },
                    &seed.snapshot,
                    &seed.draft,
                    seed.cover.as_ref(),
                )
                .await?);
        }
    }

    /// Link the candidate to the release `link` names and read its draft
    /// from that release.
    pub(super) async fn pick_candidate_release_write(
        &self,
        candidate_key: crate::import::CandidateKey,
        link: crate::import::PressingLink,
    ) -> Result<u64, crate::import::ImportError> {
        let (candidate, current) = self.metadata_write_base(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        let release = self.release_for_pick(&candidate_key, &link.record).await?;
        // Picking a result of a typed search reads its documents: what they
        // state its album is joins the search's cards.
        self.land_picked(&candidate_key, release.release(), release.album_links())
            .await;
        let audio = crate::import::catalog_reading::ReadAudio::of_files(&candidate.files)?;
        let folder_tracks = u32::try_from(audio.lengths.len()).expect("a folder's tracks fit u32");
        // A release that cannot be read onto the folder's audio — it lists
        // another number of tracks than the folder holds, several sets of its
        // discs fit alike, or a track it lays on the audio has no title — is
        // not applied, however a person found it: the pick is refused before
        // anything else is fetched or written. One that lists no tracks says
        // nothing either way and is applied.
        let reading = release.read(&audio);
        let listed = crate::import::catalog_reading::listing(&reading, &audio);
        let values = reading?;
        // A partner that fails to load fails the pick, leaving the previous
        // one in place.
        let prepared_partners = crate::import::service::prepare_partners(
            &self.library_manager,
            &link.record,
            &link.partners,
            CallPriority::Interactive,
        )
        .await?;
        for partner in &prepared_partners {
            self.land_picked(&candidate_key, partner.release(), partner.album_links())
                .await;
        }
        // The choice is stored as the candidate's result unless a run's result
        // already stands.
        let detail = release.detail(&prepared_partners);
        let picked = crate::import::search::MetadataResult::of_pick(&detail, listed);
        // The records of the pressing picked, as the folder's text reads them
        // now: what the year falls back on.
        let records: Vec<crate::import::search::MetadataResult> = std::iter::once(picked.clone())
            .chain(
                prepared_partners
                    .iter()
                    .map(crate::import::search::MetadataResult::of_release),
            )
            .collect();
        let state = self
            .library_manager
            .load_import_candidate_state(&content_hash)
            .await?;
        let text = crate::identify::CandidateText::of_stored(
            state.as_ref().and_then(|state| state.signals.as_ref()),
            &state
                .as_ref()
                .map(|state| state.lookup_choices.clone())
                .unwrap_or_default(),
        );
        let metadata = self
            .external_candidate_metadata(
                &release,
                values,
                prepared_partners,
                &current.draft,
                crate::identify::row_facts::folder_pressing_year(&text, &records),
            )
            .await?;
        let settled_by_choice = crate::identify::TerminalVerdict::of_pick(picked, folder_tracks);
        let _commit = self
            .commit_lock_for_revision(
                "pick a release",
                &candidate_key,
                &content_hash,
                current.file_edit_revision,
            )
            .await?;
        Ok(self
            .preparations
            .apply_pick(
                &candidate.watched_folder_path,
                &crate::import::CandidateAsRead {
                    content_hash: content_hash.clone(),
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: current.metadata_revision,
                },
                &candidate_key,
                &link,
                &metadata,
                settled_by_choice,
            )
            .await?)
    }

    /// Clear the candidate's draft, keeping its file decisions and its release
    /// link. Like a pick, it ends the candidate's identification and announces
    /// the change.
    pub(crate) async fn clear_candidate_metadata(
        &self,
        candidate_key: crate::import::CandidateKey,
    ) -> Result<u64, crate::import::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision = this
                .clear_candidate_metadata_write(candidate_key.clone())
                .await?;
            this.identification_ended(&candidate_key).await;
            this.announce_metadata_changed(candidate_key);
            Ok(revision)
        })
        .await
    }

    async fn clear_candidate_metadata_write(
        &self,
        candidate_key: crate::import::CandidateKey,
    ) -> Result<u64, crate::import::ImportError> {
        let Some(candidate) = self.get_release_candidate(&candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        let content_hash = candidate.files.content_hash();
        let current = self
            .library_manager
            .load_import_candidate_preparation(&content_hash)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no stored import preparation"),
            })?;
        let mut draft = candidate.blank_draft();
        crate::import::pane::keep_row_identities(&mut draft.tracks, &current.draft.tracks)?;
        let _commit = self
            .commit_lock_for_revision(
                "clear metadata",
                &candidate_key,
                &content_hash,
                current.file_edit_revision,
            )
            .await?;
        Ok(self
            .preparations
            .apply_source(
                &candidate.watched_folder_path,
                &crate::import::CandidateAsRead {
                    content_hash: content_hash.clone(),
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: current.metadata_revision,
                },
                &candidate_key,
                &crate::import::CandidateMetadataDraft {
                    draft,
                    provenance: None,
                    cover: None,
                    cover_options: Vec::new(),
                    assets: crate::import::CandidatePreparedAssets::default(),
                },
            )
            .await?)
    }

    /// Keep the candidate's own draft over what its stored lookup offered:
    /// none of the releases it found is the folder's, or it found none. The
    /// candidate is linked to no release and the draft stays as it is; the
    /// decision goes with the verdict, so identifying the candidate again
    /// starts without it.
    pub(crate) async fn keep_candidate_draft(
        &self,
        candidate_key: crate::import::CandidateKey,
    ) -> Result<u64, crate::import::ImportError> {
        let this = self.clone();
        self.committed(async move { this.keep_candidate_draft_write(candidate_key).await })
            .await
    }

    async fn keep_candidate_draft_write(
        &self,
        candidate_key: crate::import::CandidateKey,
    ) -> Result<u64, crate::import::ImportError> {
        let Some(candidate) = self.get_release_candidate(&candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        let content_hash = candidate.files.content_hash();
        let current = self
            .library_manager
            .load_import_candidate_preparation(&content_hash)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no stored import preparation"),
            })?;
        let _commit = self
            .commit_lock_for_revision(
                "keep the folder's own draft",
                &candidate_key,
                &content_hash,
                current.file_edit_revision,
            )
            .await?;
        Ok(self
            .preparations
            .keep_own_draft(&crate::import::CandidateAsRead {
                content_hash: content_hash.clone(),
                file_edit_revision: candidate.file_edit_revision,
                metadata_revision: current.metadata_revision,
            })
            .await?)
    }

    /// Unlink the candidate from its release. The draft stays as it is.
    pub(crate) async fn unlink_candidate_release(
        &self,
        candidate_key: crate::import::CandidateKey,
    ) -> Result<u64, crate::import::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision = this
                .unlink_candidate_release_write(candidate_key.clone())
                .await?;
            this.announce_metadata_changed(candidate_key);
            Ok(revision)
        })
        .await
    }

    async fn unlink_candidate_release_write(
        &self,
        candidate_key: crate::import::CandidateKey,
    ) -> Result<u64, crate::import::ImportError> {
        let (candidate, current) = self.metadata_write_base(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        let _commit = self
            .commit_lock_for_revision(
                "unlink the release",
                &candidate_key,
                &content_hash,
                current.file_edit_revision,
            )
            .await?;
        Ok(self
            .preparations
            .unlink(&crate::import::CandidateAsRead {
                content_hash,
                file_edit_revision: candidate.file_edit_revision,
                metadata_revision: current.metadata_revision,
            })
            .await?)
    }

    pub(crate) fn announce_metadata_changed(&self, candidate_key: crate::import::CandidateKey) {
        self.event_tx
            .send(ImportEvent::Scan(ScanEvent::CandidateMetadataChanged {
                candidate_key,
            }));
    }

    /// Add one file decision to the candidate's stored ones, apply it to every
    /// scanned candidate with these files, store the result, and announce it.
    ///
    /// The decision is applied to copies first, so one the folder cannot take
    /// (unreadable audio) fails with nothing written.
    async fn write_file_edits(
        &self,
        candidate_key: &crate::import::CandidateKey,
        files: crate::import::folder_scanner::CategorizedFiles,
        offered_revision: u64,
        decide: impl FnOnce(&mut crate::import::folder_scanner::CandidateFileEdits),
    ) -> Result<(), crate::import::ImportError> {
        // Settled files and the draft are prepared before the commit lock,
        // because tag reads can be slow and every pane control waits on this
        // lock.
        let content_hash = files.content_hash();
        let current_candidate = self.editable_candidate_for_commit(candidate_key).await?;
        let current_files = &current_candidate.files;
        let expected_revision = current_candidate.file_edit_revision;
        if current_files.content_hash() != content_hash || expected_revision != offered_revision {
            return Err(crate::import::CandidateAsRead::files_moved(
                offered_revision,
                crate::import::preparation::CandidateWrite::FileDecisions,
            )
            .into());
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

        loop {
            let matching_files = self
                .library_manager
                .load_releases_reading(&content_hash, expected_revision)
                .await?;
            let matching_keys = matching_files
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<std::collections::BTreeSet<_>>();
            let decided = edits.clone();
            let settled = tokio::task::spawn_blocking(move || {
                let mut settled = Vec::with_capacity(matching_files.len());
                for (key, mut files) in matching_files {
                    files.apply_candidate_file_edits(&decided)?;
                    settled.push((key, files));
                }
                Ok::<_, crate::import::folder_scanner::UnreadableAudio>(settled)
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
                    detail: format!(
                        "file decision produced no settled candidate for {candidate_key}"
                    ),
                })?;
            let stored = self
                .library_manager
                .load_candidate_file_tag_snapshot(
                    &current_candidate.watched_folder_path,
                    candidate_key,
                )
                .await?
                .ok_or_else(|| crate::import::ImportError::Internal {
                    detail: format!("{candidate_key} has no scanned tag snapshot identity"),
                })?;
            let mut replacement = current_candidate.clone();
            replacement.files = settled_files.clone();
            let reads = crate::import::folder_scanner::FileReads::new(self.file_tags.clone());
            let initialized = tokio::task::spawn_blocking(move || {
                crate::import::file_metadata_seed::FileMetadataSeed::read(
                    &replacement,
                    stored.scan_generation,
                    &reads,
                    None,
                )
                .map(|seed| seed.draft)
            })
            .await
            .map_err(|error| crate::import::ImportError::Internal {
                detail: format!("replacement track metadata task failed: {error}"),
            })??;
            let draft = crate::import::pane::redraw_draft_for_files(
                current_files,
                settled_files,
                initialized,
                &preparation.draft,
                self.ids.as_ref(),
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
                .library_manager
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
                .preparations
                .store_file_decisions(
                    &crate::import::CandidateAsRead {
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
    pub(super) async fn actionable_candidate_files(
        &self,
        candidate_key: &crate::import::CandidateKey,
    ) -> Result<
        Option<(crate::import::folder_scanner::CategorizedFiles, u64)>,
        crate::import::ImportError,
    > {
        Ok(self
            .get_release_candidate(candidate_key)
            .await?
            .map(|candidate| (candidate.files, candidate.file_edit_revision)))
    }

    /// [`Self::actionable_candidate_files`] for a sheet operation, refusing a key
    /// that names no candidate.
    async fn folder_files_for_binding(
        &self,
        candidate_key: &crate::import::CandidateKey,
    ) -> Result<(crate::import::folder_scanner::CategorizedFiles, u64), crate::import::ImportError>
    {
        self.actionable_candidate_files(candidate_key)
            .await?
            .ok_or_else(|| crate::import::ImportError::SheetBinding {
                detail: format!("{candidate_key} is not a folder candidate"),
            })
    }
}

/// The cover [`ImportServiceHandle::fetch_release_cover`] fetched, with the
/// bytes its catalog served.
pub(crate) type FetchedCover = (
    bae_catalogs::remote_image::RemoteCover,
    bae_catalogs::remote_image::RemoteImage,
);

/// A fetch of an external release's cover that failed, naming the catalog
/// that offered it.
#[derive(Debug)]
pub(crate) struct CoverFetchFailure {
    pub(crate) source: bae_catalogs::catalog::Catalog,
    pub(crate) error: bae_catalogs::remote_image::RemoteImageError,
}

/// The metadata `draft`, read from an external release, makes with the
/// cover fetched for it, and the covers `release` and the `partners` picked
/// with it offer the picker.
pub(crate) fn external_metadata_with_cover(
    draft: crate::import::CandidateDraft,
    release: &crate::import::source_release::SourceRelease,
    partners: &[crate::import::source_release::SourceRelease],
    cover: Option<FetchedCover>,
) -> crate::import::CandidateMetadataDraft {
    let (cover, remote_cover) = match cover {
        Some((remote, image)) => (
            Some(crate::import::CoverSelection::Remote(
                remote.image,
                remote.source,
            )),
            Some(image),
        ),
        None => (None, None),
    };
    crate::import::CandidateMetadataDraft {
        draft,
        provenance: Some(crate::import::MetadataProvenance::ExternalRelease {
            record: release.release().clone(),
        }),
        cover,
        cover_options: crate::import::source_release::pick_covers(release, partners),
        assets: crate::import::CandidatePreparedAssets { remote_cover },
    }
}
