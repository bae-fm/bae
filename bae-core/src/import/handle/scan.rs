use super::*;

impl ImportServiceHandle {
    /// Mark the candidate at `path` skipped or unskipped, persisting the change
    /// and broadcasting it so the import view re-tabs the row (New ↔ Skipped).
    /// A no-op request (already in the requested state) persists nothing and
    /// emits no event.
    pub fn set_candidate_skipped(
        &self,
        path: String,
        skipped: bool,
    ) -> Result<(), crate::import::ImportError> {
        let library_dir = self.library_manager.library_dir();
        let mut registry = self.folder_registry.lock().unwrap();
        let changed = registry.set_skipped(&library_dir, path.clone(), skipped)?;
        drop(registry);
        if changed {
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
        let files = self.candidate_files(&candidate_key)?;
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

        let files = self.candidate_files(&candidate_key)?;
        if files
            .track_sheets()
            .all(|sheet| sheet.file.relative_path != sheet_file_id)
        {
            return Err(crate::import::ImportError::SheetBinding {
                detail: format!("{candidate_key} has no track sheet {sheet_file_id}"),
            });
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

        // Applied to a copy first: an audio file that has gone unreadable since
        // the offer leaves the candidate exactly as it was, with nothing
        // written.
        let content_hash = files.content_hash();
        let mut edits = self
            .library_manager
            .load_stored_sheet_bindings()
            .await?
            .take(&content_hash);
        edits.set(sheet_file_id, decision);
        let (bound, edits) = tokio::task::spawn_blocking(move || {
            let mut bound = files;
            bound.apply_sheet_binding_edits(&edits)?;
            Ok::<_, crate::import::folder_scanner::InvalidReason>((bound, edits))
        })
        .await
        .map_err(|e| crate::import::ImportError::Internal {
            detail: format!("sheet binding task failed: {e}"),
        })??;

        // Durable first, and atomically: the binding and the verdict it
        // invalidates move together, so nothing can observe a folder whose
        // stored answer describes the shape it just stopped having.
        self.library_manager
            .save_import_candidate_sheet_bindings(&content_hash, &candidate_key, &edits)
            .await?;

        let Some(candidate) = self
            .candidate_state
            .lock()
            .unwrap()
            .set_files(&candidate_key, bound)
        else {
            // The candidate dropped out of the scan while the decision was
            // being written. The decision itself stands — it is keyed by the
            // folder's bytes, not by its presence in this session's list — and
            // the next scan that finds the folder applies it.
            tracing::info!(
                "{candidate_key} left the candidate list while its sheet binding was written; \
                 the decision stands and the next scan applies it"
            );
            return Ok(());
        };
        send_event(
            &self.event_tx,
            ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }),
        );
        Ok(())
    }

    /// A scanned folder candidate's files, or an error naming what is not
    /// there. A key that resolves to an invalid candidate has no roles to edit.
    fn candidate_files(
        &self,
        candidate_key: &str,
    ) -> Result<crate::import::folder_scanner::CategorizedFiles, crate::import::ImportError> {
        match self.candidate_state.lock().unwrap().get(candidate_key) {
            Some(ImportCandidateSnapshot::Folder { candidate, .. }) => Ok(candidate.files),
            _ => Err(crate::import::ImportError::SheetBinding {
                detail: format!("{candidate_key} is not a folder candidate"),
            }),
        }
    }

    /// Subscribe to the unified event channel, filtered to only `ScanEvent`s.
    pub fn subscribe_folder_scan_events(&self) -> mpsc::UnboundedReceiver<ScanEvent> {
        let mut rx = self.event_tx.subscribe();
        let (tx, out_rx) = mpsc::unbounded_channel();
        let diagnostics = self.library_manager.diagnostics().clone();
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
                        diagnostics.event(crate::diagnostics::TelemetryEvent::Anomaly {
                            kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                        });
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        out_rx
    }
}
