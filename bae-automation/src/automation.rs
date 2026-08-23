use super::*;

impl Automation {
    pub fn new(services: AppServices, runtime_handle: &tokio::runtime::Handle) -> Self {
        let _ = runtime_handle;
        Self { services }
    }

    pub fn config_get(&self) -> AutomationConfig {
        let config = self.services.get_config();
        AutomationConfig {
            library_id: config.store_id.clone(),
            library_name: config.store_name.clone(),
            library_path: config.library_path().to_string_lossy().to_string(),
            mcp: config.mcp.into(),
        }
    }

    pub fn watched_folders(&self) -> Result<Vec<AutomationWatchedFolder>, AutomationError> {
        Ok(self
            .current_watched_folders()
            .into_iter()
            .map(|folder| AutomationWatchedFolder {
                path: folder.path,
                name: folder.name,
            })
            .collect())
    }

    pub async fn add_watched_folder(
        &self,
        path: String,
    ) -> Result<Vec<AutomationWatchedFolder>, AutomationError> {
        self.services.import_add_watched_folder(path).await?;
        self.watched_folders()
    }

    pub async fn remove_watched_folder(
        &self,
        path: String,
    ) -> Result<Vec<AutomationWatchedFolder>, AutomationError> {
        self.services.import_remove_watched_folder(path).await?;
        self.watched_folders()
    }

    pub async fn scan_watched_folders(
        &self,
        wait: ScanWait,
    ) -> Result<AutomationScanResult, AutomationError> {
        match wait {
            ScanWait::NoWait => {
                self.services.import_scan_watched_folders()?;
            }
            ScanWait::UntilFinished { timeout_ms } => {
                let mut rx = self.services.import_subscribe_folder_scan_events();
                let mut pending: std::collections::HashSet<_> = self
                    .current_watched_folders()
                    .into_iter()
                    .map(|folder| folder.path)
                    .collect();
                self.services.import_scan_watched_folders()?;
                let wait_for_finish = async {
                    while !pending.is_empty() {
                        let Some(event) = rx.recv().await else {
                            return Err(AutomationError::Unavailable(
                                "scan event channel closed before finish".to_string(),
                            ));
                        };
                        if let ScanEvent::FolderScanStatusChanged { status } = event {
                            match status.status {
                                bae_core::import::FolderScanStatus::Complete => {
                                    pending.remove(&status.watched_folder_path);
                                }
                                bae_core::import::FolderScanStatus::Failed { error } => {
                                    return Err(AutomationError::import(format!(
                                        "{}: {error}",
                                        status.watched_folder_path
                                    )));
                                }
                                bae_core::import::FolderScanStatus::Scanning => {}
                            }
                        }
                    }
                    Ok::<(), AutomationError>(())
                };
                tokio::time::timeout(Duration::from_millis(timeout_ms), wait_for_finish)
                    .await
                    .map_err(|_| {
                        AutomationError::Timeout(
                            "timed out waiting for watched-folder scan".to_string(),
                        )
                    })??;
            }
        }
        Ok(AutomationScanResult {
            watched_folders: self.watched_folders()?,
            candidates: self.list_candidates().await?,
        })
    }

    fn current_watched_folders(&self) -> Vec<bae_core::import::WatchedFolder> {
        self.services.import_watched_folders()
    }

    /// Every candidate the import tab holds, in path order.
    ///
    /// The tab's own order is per watched folder and per tab; automation
    /// presents one flat list across both, so it walks all three tabs and
    /// sorts by path — the key callers name candidates by.
    pub async fn list_candidates(&self) -> Result<Vec<AutomationCandidate>, AutomationError> {
        let runtime = self.services.candidate_runtimes();
        let mut candidates = Vec::new();
        for tab in [TriageTab::Pending, TriageTab::Done, TriageTab::Skipped] {
            let view = ImportListView {
                tab,
                ..ImportListView::default()
            };
            let projection = self
                .services
                .load_import_list(view, whole_list())
                .await
                .map_err(AutomationError::from)?;
            for window in projection.windows {
                for item in window.items {
                    match item {
                        ImportListItem::Candidate(row) => {
                            let Some(detail) = self
                                .services
                                .load_import_candidate(&row.candidate_key)
                                .await
                                .map_err(AutomationError::from)?
                            else {
                                continue;
                            };
                            candidates.push(automation_candidate_from_folder(&detail, &runtime));
                        }
                        ImportListItem::Invalid(candidate) => {
                            candidates.push(automation_candidate_from_invalid(&candidate));
                        }
                        ImportListItem::GroupHeader { .. } | ImportListItem::Boundary(_) => {}
                    }
                }
            }
        }
        candidates.sort_by(|left, right| left.path().cmp(right.path()));
        Ok(candidates)
    }

    /// One candidate by key, or `NotFound`. A request naming a key the import
    /// tables hold nothing for is refused rather than answered: core reads a
    /// key it has recorded nothing against as "the pipeline hasn't run", which
    /// is right for a scanned candidate awaiting identification and
    /// indistinguishable from a typo.
    pub async fn get_candidate(
        &self,
        candidate_key: String,
    ) -> Result<AutomationCandidate, AutomationError> {
        if let Some(detail) = self
            .services
            .load_import_candidate(&candidate_key)
            .await
            .map_err(AutomationError::from)?
        {
            let runtime = self.services.candidate_runtimes();
            return Ok(automation_candidate_from_folder(&detail, &runtime));
        }
        if let Some(bae_core::import::ImportCandidateSnapshot::Invalid(candidate)) = self
            .services
            .import_get_candidate(&candidate_key)
            .await
            .map_err(AutomationError::from)?
        {
            return Ok(automation_candidate_from_invalid(&candidate));
        }
        Err(AutomationError::not_found(format!(
            "candidate '{candidate_key}' not found"
        )))
    }

    pub async fn set_candidate_skipped(
        &self,
        candidate_key: String,
        skipped: bool,
    ) -> Result<(), AutomationError> {
        self.services
            .import_set_candidate_skipped(candidate_key, skipped)
            .await?;
        Ok(())
    }

    pub async fn search_imports(
        &self,
        query: AutomationSearchQuery,
    ) -> Result<AutomationSearchResults, AutomationError> {
        let results = self
            .services
            .import_search_with_status(search_query(query))
            .await?;
        Ok(automation_search_results(results))
    }

    /// Pick an identity for a candidate: a release, or the folder's own tags.
    /// The documents land before the pick does, so a pane opened afterwards
    /// draws whole.
    pub async fn pick_candidate_identity(
        &self,
        candidate_key: String,
        pick: AutomationIdentityPick,
    ) -> Result<EmptyResponse, AutomationError> {
        // Resolve the candidate first, and hand core the key the snapshot
        // resolved rather than the caller's string, so a typo is refused here
        // rather than stored.
        let candidate = self.get_candidate(candidate_key).await?;
        self.services
            .import_pick_candidate_identity(candidate.key().to_string(), identity_pick(pick))
            .await?;
        Ok(EmptyResponse {})
    }

    /// Type one album-level metadata field over what the pick seeds.
    pub async fn set_candidate_edit_field(
        &self,
        candidate_key: String,
        field: AutomationCandidateEditField,
        value: String,
    ) -> Result<EmptyResponse, AutomationError> {
        let candidate = self.get_candidate(candidate_key).await?;
        self.services
            .import_set_candidate_edit_field(candidate.key(), candidate_edit_field(field), value)
            .await?;
        Ok(EmptyResponse {})
    }

    /// Choose the cover this candidate commits with.
    pub async fn set_candidate_cover(
        &self,
        candidate_key: String,
        cover: AutomationCoverSelection,
    ) -> Result<EmptyResponse, AutomationError> {
        let candidate = self.get_candidate(candidate_key).await?;
        self.services
            .import_set_candidate_cover(candidate.key(), cover_selection(cover))
            .await?;
        Ok(EmptyResponse {})
    }

    pub async fn preview_file_tags(
        &self,
        candidate_key: String,
    ) -> Result<AutomationReleaseUserEdit, AutomationError> {
        self.get_candidate(candidate_key.clone()).await?;
        let edit = self
            .services
            .import_preview_file_tags_for_folder(candidate_key)
            .await?;
        Ok(automation_release_user_edit(edit))
    }

    pub async fn start_import(
        &self,
        request: AutomationStartImport,
    ) -> Result<AutomationImportStarted, AutomationError> {
        let import_id = self
            .services
            .import_start_import(
                &request.candidate_key,
                storage_mode(request.storage_mode),
                request.pin,
            )
            .await?;
        Ok(AutomationImportStarted { import_id })
    }

    pub async fn release_detail(
        &self,
        release_id: String,
    ) -> Result<AutomationRelease, AutomationError> {
        self.services
            .find_release_detail(&release_id)
            .await?
            .map(automation_release)
            .ok_or_else(|| AutomationError::not_found(format!("release '{release_id}' not found")))
    }

    /// Enqueue a release to export its files verbatim to `target_dir`. Returns
    /// immediately once queued; the copy runs on the background export queue.
    pub async fn export_release(
        &self,
        release_id: String,
        target_dir: String,
    ) -> Result<AutomationReleaseExport, AutomationError> {
        self.services
            .enqueue_export(&release_id, PathBuf::from(target_dir))
            .await?;
        Ok(AutomationReleaseExport { release_id })
    }

    /// Run a storage transition against one release: the same calls the desktop
    /// Storage Manager's row menu makes.
    ///
    /// The release is resolved first, so a key that names nothing is refused
    /// rather than handed to a transfer. Which transitions are available is
    /// core's answer, read off the release's own `storage_actions` — the list the
    /// desktop renders its menu from — so this refuses exactly what the UI hides
    /// (no cloud home, already local, already pinned) without deciding anything
    /// itself. Cancel is not gated: core dispatches it to whichever transition is
    /// running and does nothing when none is.
    pub async fn release_storage_action(
        &self,
        release_id: String,
        action: AutomationStorageAction,
    ) -> Result<AutomationStorageActionOutcome, AutomationError> {
        let summary = self.release_storage_summary(&release_id).await?;
        match action {
            AutomationStorageAction::MoveToCloud { pin } => {
                require_action(
                    &summary,
                    AutomationReleaseStorageAction::MakeRemote,
                    "move to cloud",
                )?;
                let outbox_revision = self.services.make_release_remote(&release_id, pin).await?;
                Ok(AutomationStorageActionOutcome::CloudUploadQueued {
                    release_id,
                    outbox_revision,
                })
            }
            AutomationStorageAction::Pin => {
                require_action(&summary, AutomationReleaseStorageAction::Pin, "pin")?;
                // The pin joins the download queue rather than running inline —
                // the same path the Storage Manager's Pin uses, so a batch
                // serializes and reports through the Downloads pane.
                self.services.enqueue_pins(vec![release_id.clone()]).await;
                Ok(AutomationStorageActionOutcome::PinQueued { release_id })
            }
            AutomationStorageAction::Unpin => {
                require_action(&summary, AutomationReleaseStorageAction::Unpin, "unpin")?;
                self.services.unpin_release(&release_id).await?;
                Ok(AutomationStorageActionOutcome::Unpinned { release_id })
            }
            AutomationStorageAction::MakeLocal { destination_dir } => {
                require_action(
                    &summary,
                    AutomationReleaseStorageAction::MakeLocal,
                    "make local",
                )?;
                self.services
                    .make_release_local(&release_id, &destination_dir)
                    .await?;
                Ok(AutomationStorageActionOutcome::MadeLocal { release_id })
            }
            AutomationStorageAction::Cancel => {
                self.services.cancel_release_transition(&release_id).await?;
                Ok(AutomationStorageActionOutcome::Cancelled { release_id })
            }
        }
    }

    /// The release's storage facts, or `NotFound` when no release wears that id.
    async fn release_storage_summary(
        &self,
        release_id: &str,
    ) -> Result<AutomationReleaseSummary, AutomationError> {
        self.services
            .find_release_detail(release_id)
            .await?
            .map(|detail| automation_release_summary(detail.summary))
            .ok_or_else(|| AutomationError::not_found(format!("release '{release_id}' not found")))
    }

    /// The current export-queue snapshot: per-release state and rolled-up counts.
    pub fn output_status(&self) -> AutomationOutputSnapshot {
        automation_output_snapshot(self.services.output_snapshot())
    }

    pub async fn reidentify_release(
        &self,
        release_id: String,
        choice: AutomationIdentityChoice,
    ) -> Result<(), AutomationError> {
        self.services
            .re_identify_release(&release_id, identity_choice(choice))
            .await?;
        Ok(())
    }

    pub async fn reset_release_metadata(
        &self,
        release_id: String,
    ) -> Result<AutomationReleaseUserEdit, AutomationError> {
        self.services
            .reset_metadata_to_source(&release_id)
            .await
            .map(automation_release_user_edit)
            .map_err(AutomationError::from)
    }

    pub async fn update_release_metadata(
        &self,
        release_id: String,
        edit: AutomationReleaseUserEdit,
    ) -> Result<(), AutomationError> {
        self.services
            .apply_release_metadata_user_edit(&release_id, &release_user_edit(edit))
            .await?;
        Ok(())
    }

    pub async fn search_library(
        &self,
        query: String,
    ) -> Result<AutomationLibrarySearchResults, AutomationError> {
        // Trimming and the blank check are core policy: a blank query is not a
        // search and returns nothing, rather than matching every row.
        let results = match bae_core::library::LibrarySearchQuery::parse(&query) {
            Some(query) => self.services.search_library(&query).await?,
            None => SearchResults::default(),
        };
        Ok(automation_library_search_results(results))
    }

    pub async fn call_tool(
        &self,
        tool: AutomationTool,
        args: Value,
    ) -> Result<Value, AutomationError> {
        match tool {
            AutomationTool::ConfigGet => {
                expect_no_args(args, tool.name())?;
                to_value(self.config_get())
            }
            AutomationTool::WatchedFoldersList => {
                expect_no_args(args, tool.name())?;
                to_list_value("watched_folders", self.watched_folders()?)
            }
            AutomationTool::WatchedFolderAdd => {
                let input: PathInput = from_value(args)?;
                to_list_value(
                    "watched_folders",
                    self.add_watched_folder(input.path).await?,
                )
            }
            AutomationTool::WatchedFolderRemove => {
                let input: PathInput = from_value(args)?;
                to_list_value(
                    "watched_folders",
                    self.remove_watched_folder(input.path).await?,
                )
            }
            AutomationTool::WatchedFoldersScan => {
                let wait: ScanWait = from_value(args)?;
                to_value(self.scan_watched_folders(wait).await?)
            }
            AutomationTool::ImportCandidatesList => {
                expect_no_args(args, tool.name())?;
                to_list_value("candidates", self.list_candidates().await?)
            }
            AutomationTool::ImportCandidateGet => {
                let input: CandidateKeyInput = from_value(args)?;
                to_value(self.get_candidate(input.candidate_key).await?)
            }
            AutomationTool::ImportCandidateSkipSet => {
                let input: CandidateSkipSetInput = from_value(args)?;
                self.set_candidate_skipped(input.candidate_key, input.skipped)
                    .await?;
                to_value(EmptyResponse {})
            }
            AutomationTool::ImportSearch => {
                let query: AutomationSearchQuery = from_value(args)?;
                to_value(self.search_imports(query).await?)
            }
            AutomationTool::ImportCandidateIdentityPick => {
                let input: CandidateIdentityPickInput = from_value(args)?;
                to_value(
                    self.pick_candidate_identity(input.candidate_key, input.pick)
                        .await?,
                )
            }
            AutomationTool::ImportCandidateEditFieldSet => {
                let input: CandidateEditFieldInput = from_value(args)?;
                to_value(
                    self.set_candidate_edit_field(input.candidate_key, input.field, input.value)
                        .await?,
                )
            }
            AutomationTool::ImportCandidateCoverSet => {
                let input: CandidateCoverInput = from_value(args)?;
                to_value(
                    self.set_candidate_cover(input.candidate_key, input.cover)
                        .await?,
                )
            }
            AutomationTool::ImportFileTagsPreview => {
                let input: CandidateKeyInput = from_value(args)?;
                to_value(self.preview_file_tags(input.candidate_key).await?)
            }
            AutomationTool::ImportStart => {
                let input: AutomationStartImport = from_value(args)?;
                to_value(self.start_import(input).await?)
            }
            AutomationTool::ReleaseDetailGet => {
                let input: ReleaseIdInput = from_value(args)?;
                to_value(self.release_detail(input.release_id).await?)
            }
            AutomationTool::ReleaseExport => {
                let input: ReleaseExportInput = from_value(args)?;
                to_value(
                    self.export_release(input.release_id, input.target_dir)
                        .await?,
                )
            }
            AutomationTool::ReleaseStorageAction => {
                let input: ReleaseStorageActionInput = from_value(args)?;
                to_value(
                    self.release_storage_action(input.release_id, input.action)
                        .await?,
                )
            }
            AutomationTool::OutputStatus => {
                expect_no_args(args, tool.name())?;
                to_value(self.output_status())
            }
            AutomationTool::ReleaseReidentify => {
                let input: ReleaseReidentifyInput = from_value(args)?;
                self.reidentify_release(input.release_id, input.choice)
                    .await?;
                to_value(EmptyResponse {})
            }
            AutomationTool::ReleaseMetadataReset => {
                let input: ReleaseIdInput = from_value(args)?;
                to_value(self.reset_release_metadata(input.release_id).await?)
            }
            AutomationTool::ReleaseMetadataUpdate => {
                let input: ReleaseMetadataUpdateInput = from_value(args)?;
                self.update_release_metadata(input.release_id, input.edit)
                    .await?;
                to_value(EmptyResponse {})
            }
            AutomationTool::LibrarySearch => {
                let input: LibrarySearchInput = from_value(args)?;
                to_value(self.search_library(input.query).await?)
            }
        }
    }
}

/// A window over every item the list holds. Automation presents the whole
/// queue at once; the paging the tab does is a rendering concern.
fn whole_list() -> bae_core::library::LibraryPageWindows {
    std::iter::once(bae_core::library::LibraryPageWindow {
        offset: 0,
        limit: u64::MAX,
    })
    .collect()
}
