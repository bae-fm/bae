use super::*;

impl Automation {
    pub fn new(services: AppServices) -> Self {
        let state = Arc::new(AutomationState::new(services.subscribe_import_candidates()));
        Self { services, state }
    }

    pub fn status(&self) -> AutomationStatus {
        AutomationStatus {
            config: self.config_get(),
            candidate_count: self.state.candidate_count(),
        }
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

    pub fn watched_folders(&self) -> Vec<AutomationWatchedFolder> {
        self.current_watched_folders()
            .into_iter()
            .map(|folder| AutomationWatchedFolder {
                path: folder.path,
                name: folder.name,
            })
            .collect()
    }

    pub async fn add_watched_folder(
        &self,
        path: String,
    ) -> Result<Vec<AutomationWatchedFolder>, AutomationError> {
        self.services.import_add_watched_folder(path).await?;
        Ok(self.watched_folders())
    }

    pub async fn remove_watched_folder(
        &self,
        path: String,
    ) -> Result<Vec<AutomationWatchedFolder>, AutomationError> {
        self.services.import_remove_watched_folder(path).await?;
        Ok(self.watched_folders())
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
            watched_folders: self.watched_folders(),
            candidates: self.list_candidates(),
        })
    }

    fn current_watched_folders(&self) -> Vec<bae_core::import::WatchedFolder> {
        self.state.watched_folders()
    }

    pub fn list_candidates(&self) -> Vec<AutomationCandidate> {
        self.state.list_candidates()
    }

    pub fn get_candidate(
        &self,
        candidate_key: String,
    ) -> Result<AutomationCandidate, AutomationError> {
        self.state.get_candidate(&candidate_key)
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

    pub async fn prefetch_release(
        &self,
        candidate_key: String,
        source: AutomationMetadataSource,
        release_id: String,
    ) -> Result<AutomationReleasePrefetch, AutomationError> {
        // Resolve the candidate before fetching anything, and hand core the key
        // the snapshot resolved rather than the caller's string, so the lookup
        // is load-bearing: deleting it on its own stops compiling.
        //
        // Core reads a key it has recorded nothing against as "the pipeline
        // hasn't run": the right answer for a scanned candidate awaiting
        // identification, and indistinguishable from a typo. Answered rather
        // than refused, a typo comes back reading "found by searching", which
        // is what a key with no evidence behind it honestly is.
        let candidate = self.state.get_candidate(&candidate_key)?;

        let prefetch = self
            .services
            .import_prefetch_release(
                candidate.key(),
                &release_id,
                source.into(),
                // The claim a pick records. A caller committing only the album
                // passes its own `identity_choice` to `import_start` and shapes
                // the seed for it with `import_release_edit_shape`.
                bae_core::import::ClaimLevel::Exact,
            )
            .await?;
        Ok(AutomationReleasePrefetch {
            detail: automation_release_detail(prefetch.detail),
            unmasked_seed: automation_release_user_edit(prefetch.seed),
            claim: automation_claim_line(prefetch.claim),
        })
    }

    pub async fn preview_file_tags(
        &self,
        candidate_key: String,
    ) -> Result<AutomationReleaseUserEdit, AutomationError> {
        self.state.get_candidate(&candidate_key)?;
        let edit = self
            .services
            .import_preview_file_tags_for_folder(candidate_key)
            .await?;
        Ok(automation_release_user_edit(edit))
    }

    pub async fn shape_release_edit(
        &self,
        seed: AutomationReleaseUserEdit,
        choice: AutomationIdentityChoice,
    ) -> Result<AutomationReleaseUserEdit, AutomationError> {
        let seed = release_user_edit(seed);
        let choice = identity_choice(choice);
        Ok(automation_release_user_edit(shape_user_edit_for_choice(
            &seed, &choice,
        )))
    }

    pub async fn start_import(
        &self,
        request: AutomationStartImport,
    ) -> Result<AutomationImportStarted, AutomationError> {
        let import_id = self
            .services
            .import_start_import(
                &request.candidate_key,
                request.selected_cover.map(cover_selection),
                storage_mode(request.storage_mode),
                request.pin,
                identity_choice(request.identity_choice),
                request.user_edit.map(release_user_edit),
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
                to_list_value("watched_folders", self.watched_folders())
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
                to_list_value("candidates", self.list_candidates())
            }
            AutomationTool::ImportCandidateGet => {
                let input: CandidateKeyInput = from_value(args)?;
                to_value(self.get_candidate(input.candidate_key)?)
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
            AutomationTool::ImportReleasePrefetch => {
                let input: ReleasePrefetchInput = from_value(args)?;
                to_value(
                    self.prefetch_release(input.candidate_key, input.source, input.release_id)
                        .await?,
                )
            }
            AutomationTool::ImportFileTagsPreview => {
                let input: CandidateKeyInput = from_value(args)?;
                to_value(self.preview_file_tags(input.candidate_key).await?)
            }
            AutomationTool::ImportReleaseEditShape => {
                let input: ShapeReleaseEditInput = from_value(args)?;
                to_value(self.shape_release_edit(input.seed, input.choice).await?)
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
