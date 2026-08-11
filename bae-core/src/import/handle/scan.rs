use super::*;

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
        let watched_folder_path = match self.candidate_state.lock().unwrap().get(&path) {
            Some(ImportCandidateSnapshot::Folder {
                candidate,
                actionable: true,
                ..
            }) => candidate.watched_folder_path,
            _ => {
                return Err(crate::import::ImportError::Internal {
                    detail: format!("{path} is not an actionable folder candidate"),
                })
            }
        };
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
            self.candidate_state
                .lock()
                .unwrap()
                .set_skipped(&path, skipped);
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
        let (files, _) = self.folder_files_for_binding(&candidate_key)?;
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

        let (files, offered_revision) = self.folder_files_for_binding(&candidate_key)?;
        let Some(bound) = files
            .track_sheets()
            .find(|sheet| sheet.file.relative_path == sheet_file_id)
            .map(|sheet| sheet.binding.describes().map(str::to_string))
        else {
            return Err(crate::import::ImportError::SheetBinding {
                detail: format!("{candidate_key} has no track sheet {sheet_file_id}"),
            });
        };
        // Same rule as `set_sheet_disc`: re-stating the binding in force
        // decides nothing, and must not clear the verdict.
        if bound == audio_file_id {
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
        let (files, offered_revision) = self.folder_files_for_binding(&candidate_key)?;
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
            debug!("{sheet_file_id} is already disc {disc:?}; nothing to write");
            return Ok(());
        }

        self.write_file_edits(&candidate_key, files, offered_revision, |edits| {
            edits.sheet_discs.set(sheet_file_id, disc);
        })
        .await
    }

    /// Record the identity the user chose for a candidate — the pressing they
    /// picked, or the decision to read the folder's own tags — keyed by the
    /// folder's current content hash, so selection reopens the pane answered
    /// after a restart. Only explicit choices land here: a Ready row's
    /// auto-pick is derived from its stored verdict and re-derives on every
    /// open, so it never writes one.
    pub(crate) async fn set_candidate_identity_pick(
        &self,
        candidate_key: String,
        pick: crate::import::IdentityPick,
    ) -> Result<(), crate::import::ImportError> {
        let Some((files, _)) = self.actionable_candidate_files(&candidate_key) else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        let pick_json =
            serde_json::to_string(&pick).map_err(|e| crate::import::ImportError::Internal {
                detail: format!("encoding identity pick: {e}"),
            })?;
        self.library_manager
            .save_candidate_identity_pick(&files.content_hash(), &candidate_key, &pick_json)
            .await?;
        Ok(())
    }

    /// Tell the surfaces a candidate's identity is decided.
    pub(crate) fn announce_identity_pick(&self, candidate_key: String) {
        send_event(
            &self.event_tx,
            ImportEvent::Scan(ScanEvent::CandidateIdentityPicked { candidate_key }),
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
        let Some((files, offered_revision)) = self.actionable_candidate_files(&candidate_key)
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
        let (current_files, expected_revision) = self
            .actionable_candidate_files(candidate_key)
            .ok_or_else(|| crate::import::ImportError::FileRole {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            })?;
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

        let matching_files = self
            .candidate_state
            .lock()
            .unwrap()
            .files_for_identity(&content_hash, expected_revision);
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

        // Durable first, and atomically: the decision and the verdict it
        // invalidates move together, so nothing can observe a folder whose
        // stored answer describes the shape it just stopped having.
        let next_revision = self
            .library_manager
            .save_import_candidate_file_edits(
                &content_hash,
                candidate_key,
                expected_revision,
                &edits,
                &settled,
            )
            .await?;

        let Some(candidates) = self.candidate_state.lock().unwrap().set_files_for_identity(
            &content_hash,
            expected_revision,
            settled,
            next_revision,
        ) else {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "{candidate_key} identity changed while its file decision was being written"
                ),
            });
        };
        for candidate in candidates {
            send_event(
                &self.event_tx,
                ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }),
            );
        }
        Ok(())
    }

    /// A scanned folder candidate's files, or `None` for a key that names no
    /// folder — an invalid candidate has no roles or bindings to edit. Each
    /// caller names the refusal in its own terms rather than borrowing the
    /// other's.
    fn actionable_candidate_files(
        &self,
        candidate_key: &str,
    ) -> Option<(crate::import::folder_scanner::CategorizedFiles, u64)> {
        match self.candidate_state.lock().unwrap().get(candidate_key) {
            Some(ImportCandidateSnapshot::Folder {
                candidate,
                actionable: true,
                ..
            }) => Some((candidate.files, candidate.file_edit_revision)),
            _ => None,
        }
    }

    /// A folder candidate's files for a binding operation, or the refusal that
    /// names what the key resolved to instead.
    fn folder_files_for_binding(
        &self,
        candidate_key: &str,
    ) -> Result<(crate::import::folder_scanner::CategorizedFiles, u64), crate::import::ImportError>
    {
        self.actionable_candidate_files(candidate_key)
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
