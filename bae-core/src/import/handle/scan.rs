use super::*;
use crate::util::rate_limiter::CallPriority;

impl ImportServiceHandle {
    /// Mark the candidate at `path` skipped or unskipped and announce the
    /// change; a request that changes nothing writes nothing and sends no event.
    ///
    /// Skipping ends the candidate's identification, since the person has set
    /// it aside.
    pub async fn set_candidate_skipped(
        &self,
        path: String,
        skipped: bool,
    ) -> Result<(), crate::import::ImportError> {
        let this = self.clone();
        self.committed(async move { this.set_candidate_skipped_write(path, skipped).await })
            .await
    }

    async fn set_candidate_skipped_write(
        &self,
        path: String,
        skipped: bool,
    ) -> Result<(), crate::import::ImportError> {
        let _commit = self.folder_state_commit.lock("skip a candidate").await;
        let Some(candidate) = self.get_release_candidate(&path).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{path} is not an actionable folder candidate"),
            });
        };
        if candidate.grouping.is_some() {
            if self
                .library_manager
                .set_grouping_skipped(&path, skipped)
                .await?
            {
                if skipped {
                    self.cancel_identification(&path);
                }
                self.event_tx
                    .send(ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                        candidate_key: path,
                        skipped,
                    }));
            }
            return Ok(());
        }
        let changed = self
            .library_manager
            .set_import_candidate_skipped(&candidate.path.to_string_lossy(), skipped)
            .await?;
        if changed {
            if skipped {
                self.cancel_identification(&path);
            }
            self.event_tx
                .send(ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                    candidate_key: path,
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
        candidate_key: String,
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
        candidate_key: String,
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
        candidate_key: String,
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
                let duplicate = files
                    .track_sheets()
                    .find(|sheet| sheet.file.relative_path == sheet_file_id)
                    .and_then(|sheet| sheet.binding.audio_files())
                    .is_some_and(|associated| {
                        associated.iter().any(|file| {
                            file.file_id == file_id && file.file_reference != file_reference
                        })
                    });
                if duplicate {
                    return Err(crate::import::ImportError::SheetBinding {
                        detail: format!(
                            "{file_id} already supplies another FILE reference in {sheet_file_id}"
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
        candidate_key: String,
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
        candidate_key: String,
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

    /// Build a candidate draft from an external release, keeping each current
    /// track on its file. Refuses a release whose track count differs from the
    /// candidate's.
    pub(crate) fn external_candidate_draft(
        &self,
        release: &crate::import::source_release::SourceRelease,
        durations: &crate::import::probe::SourceDurations,
        current: &crate::import::CandidateDraft,
    ) -> Result<crate::import::pane::CandidateSourceDraft, crate::import::ImportError> {
        let audio_durations = current.audio_durations(durations)?;
        let parsed = release.parsed(&audio_durations, self.clock.as_ref(), self.ids.as_ref())?;
        let edit = crate::import::RawReleaseEdit::from_user_edit(
            crate::import::parsed_album_to_user_edit(&parsed),
            crate::import::pane::CANDIDATE_TRACK_ID_PREFIX,
        );
        let mut source = crate::import::pane::metadata_over_audio(edit, current)?;
        source.source_discogs_artist_ids = crate::import::pane::source_discogs_artist_ids(&parsed);
        for (index, track) in source.draft.tracks.iter_mut().enumerate() {
            track.source_index = Some(u32::try_from(index).expect("source track index fits u32"));
        }
        Ok(source)
    }

    /// Build the metadata for an external release and fetch the artist images
    /// and cover it needs, before anything is written.
    pub(crate) async fn external_candidate_metadata(
        &self,
        release: &crate::import::source_release::SourceRelease,
        partners: Vec<crate::import::source_release::SourceRelease>,
        durations: &crate::import::probe::SourceDurations,
        provenance: crate::import::MetadataProvenance,
        current: &crate::import::CandidateDraft,
    ) -> Result<crate::import::CandidateMetadataDraft, crate::import::ImportError> {
        let source_draft = self.external_candidate_draft(release, durations, current)?;
        self.external_candidate_assets(source_draft, release, partners, durations, provenance, current)
            .await
    }

    /// The metadata `source_draft`, read from an external release, makes,
    /// with the artist images and cover it needs fetched.
    ///
    /// The cover is the first one the release or its partners offer; when there
    /// is none or it cannot be fetched, `cover` is `None` and the write keeps
    /// the candidate's current cover.
    pub(crate) async fn external_candidate_assets(
        &self,
        source_draft: crate::import::pane::CandidateSourceDraft,
        release: &crate::import::source_release::SourceRelease,
        partners: Vec<crate::import::source_release::SourceRelease>,
        durations: &crate::import::probe::SourceDurations,
        provenance: crate::import::MetadataProvenance,
        current: &crate::import::CandidateDraft,
    ) -> Result<crate::import::CandidateMetadataDraft, crate::import::ImportError> {
        let draft = source_draft.draft;
        let source_discogs_artist_ids = source_draft.source_discogs_artist_ids;
        let required_artist_ids = source_discogs_artist_ids
            .union(&source_draft.mapped_credit_discogs_artist_ids)
            .cloned()
            .collect();
        let artist_images = self
            .library_manager
            .prepare_discogs_artist_images(required_artist_ids)
            .await?;
        let default_cover = crate::import::source_release::pick_covers(release, &partners)
            .into_iter()
            .next();
        let (cover, remote_cover) = match default_cover {
            Some(remote) => match self.library_manager.fetch_remote_image(&remote.image.url).await? {
                Some(image) => (
                    Some(crate::import::CoverSelection::Remote(
                        remote.image,
                        remote.source,
                    )),
                    Some(image),
                ),
                None => (None, None),
            },
            None => (None, None),
        };
        Ok(crate::import::CandidateMetadataDraft {
            draft,
            source_discogs_artist_ids,
            provenance: Some(provenance),
            cover,
            assets: crate::import::CandidatePreparedAssets {
                applied_source: Some(crate::import::source_release::AppliedSource {
                    primary: release.clone(),
                    partners,
                    audio_durations_ms: current.audio_durations(durations)?,
                }),
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
        let content_hash = candidate.files.content_hash();
        let current = self
            .library_manager
            .load_import_candidate_preparation(&content_hash)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no stored import preparation"),
            })?;
        let expected_metadata_revision = current.metadata_revision;
        let durations = crate::import::probe::source_durations(&candidate.files)?;
        match &provenance {
            crate::import::MetadataProvenance::FileMetadata => loop {
                // Read tags before taking the commit lock: a network share can
                // be slow, and every pane control waits on this lock.
                let read = self
                    .read_file_tag_snapshot(&candidate_key, self.file_tags.clone())
                    .await?;
                let seed = crate::import::file_metadata_seed::FileMetadataSeed::project(
                    &read.candidate,
                    read.snapshot,
                    Some(&current.draft.tracks),
                    self.clock.as_ref(),
                    self.ids.as_ref(),
                )?;
                let commit = self
                    .commit_lock_for_revision(
"pick file tags",
                        &candidate_key,
                        &content_hash,
                        current.file_edit_revision,
                    )
                    .await?;
                // A scan that stored the candidate while its tags were read gave
                // it a newer generation, and the write only accepts tags read at
                // the current one: read again.
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
                            metadata_revision: expected_metadata_revision,
                        },
                        &seed.snapshot,
                        &seed.draft,
                        seed.cover.as_ref(),
                    )
                    .await?);
            },
            crate::import::MetadataProvenance::ExternalRelease { record, partners } => {
                let primary = record.clone();
                let release = self
                    .release_for_provenance(&candidate_key, &primary)
                    .await?;
                // A partner that fails to load fails the pick, leaving the
                // previous one in place.
                let prepared_partners = crate::import::service::prepare_partners(
                    &self.library_manager,
                    &primary,
                    partners,
                    CallPriority::Interactive,
                )
                .await?;
                // The choice is stored as the candidate's result unless a run's
                // result already stands.
                let audio_durations =
                    crate::import::audio_layout::audio_durations(&candidate.files, &durations)?;
                let detail = release.detail_for_audio(&audio_durations, &prepared_partners)?;
                let metadata = self
                    .external_candidate_metadata(
                        &release,
                        prepared_partners,
                        &durations,
                        provenance.clone(),
                        &current.draft,
                    )
                    .await?;
                let settled_by_choice = crate::identify::TerminalVerdict::of_pick(
                    crate::import::search::MetadataResult::of_pick(&detail),
                    audio_durations.len() as u32,
                );
                let _commit = self
                    .commit_lock_for_revision(
"pick a release",
                        &candidate_key,
                        &content_hash,
                        current.file_edit_revision,
                    )
                    .await?;
                return Ok(self
                    .preparations
                    .apply_source_as_result(
                        &candidate.watched_folder_path,
                        &crate::import::CandidateAsRead {
                            content_hash: content_hash.clone(),
                            file_edit_revision: candidate.file_edit_revision,
                            metadata_revision: expected_metadata_revision,
                        },
                        &candidate_key,
                        &metadata,
                        settled_by_choice,
                    )
                    .await?);
            }
        }
    }

    /// Clear the candidate's source metadata, keeping its file decisions. Like
    /// a pick, it ends the candidate's identification and announces the change.
    pub(crate) async fn clear_candidate_metadata(
        &self,
        candidate_key: String,
    ) -> Result<u64, crate::import::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision = this
                .clear_candidate_metadata_write(candidate_key.clone())
                .await?;
            this.cancel_identification(&candidate_key);
            this.announce_metadata_provenance(candidate_key);
            Ok(revision)
        })
        .await
    }

    async fn clear_candidate_metadata_write(
        &self,
        candidate_key: String,
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
        let mut draft = candidate.blank_source().draft;
        crate::import::pane::keep_row_identities(&mut draft.tracks, &current.draft.tracks)?;
        let _commit = self
            .commit_lock_for_revision(
"clear metadata",&candidate_key, &content_hash, current.file_edit_revision)
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
                    source_discogs_artist_ids: Default::default(),
                    provenance: None,
                    cover: None,
                    assets: crate::import::CandidatePreparedAssets::default(),
                },
            )
            .await?)
    }

    pub(crate) fn announce_metadata_provenance(&self, candidate_key: String) {
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
        candidate_key: &str,
        files: crate::import::folder_scanner::CategorizedFiles,
        offered_revision: u64,
        decide: impl FnOnce(&mut crate::import::folder_scanner::CandidateFileEdits),
    ) -> Result<(), crate::import::ImportError> {
        // Settled files, draft, and artist images are prepared before the
        // commit lock, because tag reads and image fetches can be slow and
        // every pane control waits on this lock.
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
            let matching_files = crate::import::candidates::files_for_identity(
                &self.library_manager.load_all_folder_scan_items().await?,
                &content_hash,
                expected_revision,
            );
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
                Ok::<_, crate::import::folder_scanner::InvalidReason>(settled)
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
            let initialized = if self
                .library_manager
                .get_config()
                .prefs
                .prefill_with_file_metadata
            {
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
                let reader = self.file_tags.clone();
                let clock = self.clock.clone();
                let ids = self.ids.clone();
                tokio::task::spawn_blocking(move || {
                    crate::import::file_metadata_seed::FileMetadataSeed::read(
                        &replacement,
                        stored.scan_generation,
                        reader.as_ref(),
                        clock.as_ref(),
                        ids.as_ref(),
                    )
                    .map(|seed| seed.draft)
                })
                .await
                .map_err(|error| crate::import::ImportError::Internal {
                    detail: format!("replacement track metadata task failed: {error}"),
                })??
            } else {
                crate::import::pane::blank_candidate_source(settled_files).draft
            };
            let draft = crate::import::pane::redraw_draft_for_files(
                current_files,
                initialized,
                &preparation.draft,
                self.ids.as_ref(),
            );
            let active = draft.release_edit();
            let (source_discogs_artist_ids, artist_images) = self
                .prepared_artist_images_for_active(
                    preparation.assets.applied_source.as_ref(),
                    &active,
                    &draft.tracks,
                    preparation.assets.artist_images.clone(),
                )
                .await?;
            let mapping_preparation = crate::import::CandidateMappingPreparation {
                draft,
                source_discogs_artist_ids,
                artist_images,
            };

            let commit = self
                .commit_lock_for_revision(
"store a file decision",candidate_key, &content_hash, expected_revision)
                .await?;
            // A scan may have added or dropped a folder with these files while
            // this was prepared; if so, prepare again.
            let matching_now = crate::import::candidates::files_for_identity(
                &self.library_manager.load_all_folder_scan_items().await?,
                &content_hash,
                expected_revision,
            )
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
                    &mapping_preparation,
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
        candidate_key: &str,
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
        candidate_key: &str,
    ) -> Result<(crate::import::folder_scanner::CategorizedFiles, u64), crate::import::ImportError>
    {
        self.actionable_candidate_files(candidate_key)
            .await?
            .ok_or_else(|| crate::import::ImportError::SheetBinding {
                detail: format!("{candidate_key} is not a folder candidate"),
            })
    }
}
