//! Reading releases together as one, and apart again. Combining every release
//! under one folder reads that folder as one release; combining any other set
//! makes a grouping of exactly those.

use super::*;
use crate::import::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey, ScanItem};
use crate::import::ImportError;

impl ImportServiceHandle {
    /// The folders on disk the release at `key` is read from.
    pub async fn candidate_source_folders(&self, key: &str) -> Result<Vec<String>, ImportError> {
        let detail = self
            .library_manager
            .load_import_candidate(key)
            .await?
            .ok_or_else(|| ImportError::Internal {
                detail: format!("{key} is no longer a candidate"),
            })?;
        Ok(detail
            .candidate
            .source_folders()
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect())
    }

    /// Read the releases at `keys` as one, in key order and named after the
    /// first, and return the new release's key. When they are every release
    /// below one folder, that folder is read as one release, so a release
    /// appearing there later joins it.
    pub async fn combine_candidates(&self, keys: Vec<String>) -> Result<String, ImportError> {
        let this = self.clone();
        self.committed(async move { this.combine_candidates_write(keys).await })
            .await
    }

    async fn combine_candidates_write(&self, mut keys: Vec<String>) -> Result<String, ImportError> {
        keys.sort();
        keys.dedup();
        if keys.len() < 2 {
            return Err(ImportError::Internal {
                detail: "combining a release requires at least two folders".into(),
            });
        }
        let mut members = Vec::with_capacity(keys.len());
        {
            let _commit = self.folder_state_commit.lock("check folders to combine").await;
            for key in &keys {
                self.ensure_combination_idle(key)?;
                members.push(self.editable_candidate_for_commit(key).await?);
            }
        }
        // A picked release is rebuilt from its members, not from disk, so it
        // cannot itself be picked.
        for member in &members {
            if let Some(grouping) = &member.grouping {
                if let Some(crate::db::GroupingFacts::Picked { .. }) =
                    self.library_manager.load_grouping(grouping).await?
                {
                    return Err(ImportError::Internal {
                        detail: "separate an existing combination before combining its folders again"
                            .into(),
                    });
                }
            }
        }
        if let Some(folder) = self.folder_holding_exactly(&members).await? {
            return self.combine_folder(folder).await;
        }
        let key = format!("grouping:{}", self.library_manager.new_id());
        let _commit = self.folder_state_commit.lock("combine releases").await;
        let regrouped = self
            .library_manager
            .combine_releases(key.clone(), members)
            .await?
            .map_err(|reason| ImportError::GroupingBlocked { reason })?;
        for candidate_key in keys {
            self.cancel_identification(&candidate_key);
            self.event_tx
                .send(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }));
        }
        self.announce_regrouped(&regrouped).await?;
        Ok(key)
    }

    /// The deepest folder under their shared watched folder holding exactly
    /// `members`, if there is one.
    async fn folder_holding_exactly(
        &self,
        members: &[crate::import::FolderCandidate],
    ) -> Result<Option<FolderReleaseDecisionKey>, ImportError> {
        let root = &members[0].watched_folder_path;
        if members
            .iter()
            .any(|member| &member.watched_folder_path != root)
        {
            return Ok(None);
        }
        let mut folder = members[0].path.clone();
        for member in &members[1..] {
            while !member.path.starts_with(&folder) {
                if !folder.pop() {
                    return Ok(None);
                }
            }
        }
        let root_path = std::path::Path::new(root);
        if folder == root_path || !folder.starts_with(root_path) {
            return Ok(None);
        }
        let selected: std::collections::BTreeSet<String> =
            members.iter().map(crate::import::FolderCandidate::key).collect();
        let below: std::collections::BTreeSet<String> = self
            .library_manager
            .load_folder_scan_items(root)
            .await?
            .into_iter()
            .filter_map(|item| match item {
                ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
                    candidate.path.starts_with(&folder).then(|| candidate.key())
                }
                ScanItem::Invalid(candidate) => {
                    candidate.path.starts_with(&folder).then(|| candidate.key())
                }
                ScanItem::Decided { .. } | ScanItem::Sidecar(_) => None,
            })
            .collect();
        if below != selected {
            return Ok(None);
        }
        Ok(Some(FolderReleaseDecisionKey {
            watched_folder_path: root.clone(),
            relative_folder_path: crate::import::watched_folder::candidate_relative_path(
                root, &folder,
            )?,
        }))
    }

    /// Read every release below the folder at `folder` as one, and return the
    /// key of the release it becomes.
    pub async fn combine_folder(
        &self,
        folder: FolderReleaseDecisionKey,
    ) -> Result<String, ImportError> {
        self.set_folder_release_decision(folder.clone(), FolderReleaseDecision::CombineAsOneRelease)
            .await?;
        self.library_manager
            .load_folder_release_decisions(&folder.watched_folder_path)
            .await?
            .get(&folder.relative_folder_path)
            .map(|reading| reading.grouping.clone())
            .ok_or_else(|| ImportError::Internal {
                detail: format!("{} was combined but no grouping is stored for it", folder.relative_folder_path),
            })
    }

    /// Read the release at `key` as the releases it is made of.
    pub async fn separate_candidate(&self, key: &str) -> Result<(), ImportError> {
        let this = self.clone();
        let key = key.to_string();
        self.committed(async move { this.separate_candidate_write(&key).await })
            .await
    }

    async fn separate_candidate_write(&self, key: &str) -> Result<(), ImportError> {
        {
            let _commit = self.folder_state_commit.lock("check a release to separate").await;
            self.ensure_combination_idle(key)?;
            if let Some(detail) = self.library_manager.load_import_candidate(key).await? {
                if detail.is_added {
                    return Err(ImportError::CandidateAlreadyImported);
                }
            }
        }
        match self.library_manager.load_grouping(key).await? {
            Some(crate::db::GroupingFacts::Anchored { folder, .. }) => {
                self.set_folder_release_decision(
                    folder,
                    FolderReleaseDecision::KeepAsSeparateReleases,
                )
                .await
            }
            Some(crate::db::GroupingFacts::Picked { .. }) => {
                let _commit = self.folder_state_commit.lock("separate a picked release").await;
                let (returned, regrouped) =
                    self.library_manager.separate_picked_grouping(key).await?;
                self.cancel_identification(key);
                self.event_tx.send(ImportEvent::Scan(ScanEvent::CandidateRemoved {
                    candidate_key: key.into(),
                }));
                for item in returned {
                    let event = match item {
                        // Each release a separation returns is found, as
                        // combining finds the release it makes.
                        ScanItem::Valid(candidate) => {
                            let standing = self
                                .candidate_standing(&candidate.key(), &candidate)
                                .await?;
                            ScanEvent::FolderCandidate {
                                candidate,
                                skipped: standing.skipped,
                                is_added: standing.imported,
                                found_while_automatic: self
                                    .library_manager
                                    .identifies_automatically(),
                            }
                        }
                        ScanItem::Discovered(candidate) => {
                            let standing = self
                                .candidate_standing(&candidate.key(), &candidate)
                                .await?;
                            ScanEvent::CandidateDiscovered {
                                candidate,
                                skipped: standing.skipped,
                                is_added: standing.imported,
                            }
                        }
                        ScanItem::Invalid(candidate) => ScanEvent::InvalidCandidate(candidate),
                        ScanItem::Decided { .. } | ScanItem::Sidecar(_) => continue,
                    };
                    self.event_tx.send(ImportEvent::Scan(event));
                }
                self.announce_regrouped(&regrouped).await?;
                Ok(())
            }
            None => Err(ImportError::Internal {
                detail: format!("{key} is not a release read from several folders"),
            }),
        }
    }

    /// Announce the releases a grouping rebuilt, saying which are new while
    /// identification runs on its own.
    async fn announce_regrouped(
        &self,
        regrouped: &crate::db::GroupingChanges,
    ) -> Result<(), ImportError> {
        for candidate_key in &regrouped.removed {
            self.event_tx.send(ImportEvent::Scan(ScanEvent::CandidateRemoved {
                candidate_key: candidate_key.clone(),
            }));
        }
        for item in &regrouped.written {
            let event = match item.clone() {
                ScanItem::Valid(candidate) => {
                    let standing = self.candidate_standing(&candidate.key(), &candidate).await?;
                    ScanEvent::FolderCandidate {
                        found_while_automatic: regrouped.found.contains(&candidate.key())
                            && self.library_manager.identifies_automatically(),
                        candidate,
                        skipped: standing.skipped,
                        is_added: standing.imported,
                    }
                }
                ScanItem::Invalid(candidate) => ScanEvent::InvalidCandidate(candidate),
                ScanItem::Discovered(_) | ScanItem::Decided { .. } | ScanItem::Sidecar(_) => {
                    continue
                }
            };
            self.event_tx.send(ImportEvent::Scan(event));
        }
        Ok(())
    }

    fn ensure_combination_idle(&self, key: &str) -> Result<(), ImportError> {
        if let Some(runtime) = self.runtime.get(key) {
            if runtime.import.is_some() {
                return Err(ImportError::CandidateImportInProgress);
            }
            if runtime.queued.is_some() || runtime.running.is_some() {
                return Err(ImportError::Internal {
                    detail: format!("identification is still running for {key}"),
                });
            }
        }
        Ok(())
    }
}
