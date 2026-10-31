//! Reading releases together as one, and apart again. Combining every release
//! under one folder reads that folder as one release; combining any other set
//! makes a grouping of exactly those.

use super::*;
use crate::import::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey, ScanItem};
use crate::import::{CandidateKey, ImportError};

impl ImportServiceHandle {
    /// The folders on disk the release at `key` is read from.
    pub async fn candidate_source_folders(
        &self,
        key: &CandidateKey,
    ) -> Result<Vec<std::path::PathBuf>, ImportError> {
        let detail = self
            .library_manager
            .load_import_candidate(key)
            .await?
            .ok_or_else(|| ImportError::Internal {
                detail: format!("{key} is no longer a candidate"),
            })?;
        Ok(detail.candidate.source_folders())
    }

    /// Read the releases at `keys` as one, in key order and named after the
    /// first, and return the new release's key. When they are every release
    /// below one folder, that folder is read as one release, so a release
    /// appearing there later joins it.
    pub async fn combine_candidates(
        &self,
        keys: Vec<CandidateKey>,
    ) -> Result<CandidateKey, ImportError> {
        let this = self.clone();
        self.committed(async move { this.combine_candidates_write(keys).await })
            .await
    }

    async fn combine_candidates_write(
        &self,
        mut keys: Vec<CandidateKey>,
    ) -> Result<CandidateKey, ImportError> {
        keys.sort();
        keys.dedup();
        if keys.len() < 2 {
            return Err(ImportError::Internal {
                detail: "combining a release requires at least two folders".into(),
            });
        }
        let mut members = Vec::with_capacity(keys.len());
        {
            let _commit = self
                .folder_state_commit
                .lock("check folders to combine")
                .await;
            for key in &keys {
                self.ensure_combination_idle(key).await?;
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
                        detail:
                            "separate an existing combination before combining its folders again"
                                .into(),
                    });
                }
            }
        }
        if let Some(folder) = self.folder_holding_exactly(&members).await? {
            return self.combine_folder(folder).await;
        }
        let key = CandidateKey::grouping(&self.library_manager.new_id());
        let _commit = self.folder_state_commit.lock("combine releases").await;
        let regrouped = self
            .library_manager
            .combine_releases(key.clone(), members)
            .await?
            .map_err(|reason| ImportError::GroupingBlocked { reason })?;
        for candidate_key in keys {
            self.identification_ended(&candidate_key).await;
            self.event_tx
                .send(ImportEvent::Scan(ScanEvent::CandidateRemoved {
                    candidate_key,
                }));
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
        if &folder == root || !folder.starts_with(root) {
            return Ok(None);
        }
        let selected: std::collections::BTreeSet<CandidateKey> = members
            .iter()
            .map(crate::import::FolderCandidate::key)
            .collect();
        let below = self
            .library_manager
            .scanned_release_keys_at_or_below(root, &folder)
            .await?;
        if below != selected {
            return Ok(None);
        }
        Ok(Some(FolderReleaseDecisionKey {
            watched_folder_path: root.clone(),
            relative_folder: crate::import::watched_folder::candidate_relative_path(root, &folder)?,
        }))
    }

    /// Read every release below the folder at `folder` as one, and return the
    /// key of the release it becomes.
    pub async fn combine_folder(
        &self,
        folder: FolderReleaseDecisionKey,
    ) -> Result<CandidateKey, ImportError> {
        self.set_folder_release_decision(
            folder.clone(),
            FolderReleaseDecision::CombineAsOneRelease,
        )
        .await?;
        self.library_manager
            .folder_reading_grouping(&folder)
            .await?
            .ok_or_else(|| ImportError::Internal {
                detail: format!(
                    "{} was combined but no grouping is stored for it",
                    folder.relative_folder.text()
                ),
            })
    }

    /// Read the release at `key` as the releases it is made of, and answer
    /// with their keys in path order.
    pub async fn separate_candidate(
        &self,
        key: &CandidateKey,
    ) -> Result<Vec<CandidateKey>, ImportError> {
        let this = self.clone();
        let key = key.clone();
        self.committed(async move { this.separate_candidate_write(&key).await })
            .await
    }

    async fn separate_candidate_write(
        &self,
        key: &CandidateKey,
    ) -> Result<Vec<CandidateKey>, ImportError> {
        {
            let _commit = self
                .folder_state_commit
                .lock("check a release to separate")
                .await;
            self.ensure_combination_idle(key).await?;
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
                let _commit = self
                    .folder_state_commit
                    .lock("separate a picked release")
                    .await;
                let (returned, regrouped) =
                    self.library_manager.separate_picked_grouping(key).await?;
                let keys = crate::import::list::release_keys_in_path_order(&returned);
                self.identification_ended(key).await;
                self.event_tx
                    .send(ImportEvent::Scan(ScanEvent::CandidateRemoved {
                        candidate_key: key.clone(),
                    }));
                for item in returned {
                    let event = match item {
                        // Each release a separation returns is found, as
                        // combining finds the release it makes.
                        ScanItem::Valid(candidate) => ScanEvent::FolderCandidate {
                            candidate,
                            found_while_automatic: self.library_manager.identifies_automatically(),
                        },
                        ScanItem::Discovered(candidate) => {
                            ScanEvent::CandidateDiscovered { candidate }
                        }
                        ScanItem::Invalid(candidate) => ScanEvent::InvalidCandidate(candidate),
                        ScanItem::Decided { .. } | ScanItem::Sidecar(_) => continue,
                    };
                    self.event_tx.send(ImportEvent::Scan(event));
                }
                self.announce_regrouped(&regrouped).await?;
                Ok(keys)
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
            self.event_tx
                .send(ImportEvent::Scan(ScanEvent::CandidateRemoved {
                    candidate_key: candidate_key.clone(),
                }));
        }
        for item in &regrouped.written {
            let event = match item.clone() {
                ScanItem::Valid(candidate) => ScanEvent::FolderCandidate {
                    found_while_automatic: regrouped.found.contains(&candidate.key())
                        && self.library_manager.identifies_automatically(),
                    candidate,
                },
                ScanItem::Invalid(candidate) => ScanEvent::InvalidCandidate(candidate),
                ScanItem::Discovered(_) | ScanItem::Decided { .. } | ScanItem::Sidecar(_) => {
                    continue
                }
            };
            self.event_tx.send(ImportEvent::Scan(event));
        }
        Ok(())
    }

    /// Refuse to combine or separate a release an import owns or that is
    /// waiting for or running an identification, read from its standing
    /// under the commit lock the caller holds.
    async fn ensure_combination_idle(
        &self,
        key: &crate::import::CandidateKey,
    ) -> Result<(), ImportError> {
        let facts = self
            .runtime
            .standing_facts(key)
            .await
            .map_err(crate::library::LibraryError::from)?;
        if facts.importing() {
            return Err(ImportError::CandidateImportInProgress);
        }
        if matches!(
            facts.identification,
            Some(
                crate::import::IdentificationStatus::Queued
                    | crate::import::IdentificationStatus::Running
            )
        ) {
            return Err(ImportError::Internal {
                detail: format!("identification is still running for {key}"),
            });
        }
        Ok(())
    }
}
