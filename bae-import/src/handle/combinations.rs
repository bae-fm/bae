//! Reading releases together as one, and apart again. Combining every release
//! under one folder reads that folder as one release; combining any other set
//! makes a grouping of exactly those.

use super::*;
use crate::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey, ScanItem};
use crate::tables::require_combination_idle;
use crate::{CandidateKey, ImportError};

impl ImportServiceHandle {
    /// The folders on disk the release at `key` is read from.
    pub async fn candidate_source_folders(
        &self,
        key: &CandidateKey,
    ) -> Result<Vec<std::path::PathBuf>, ImportError> {
        let entry = self
            .services
            .tables
            .load_candidate_entry(key)
            .await?
            .filter(|entry| entry.listed())
            .ok_or_else(|| ImportError::Internal {
                detail: format!("{key} is no longer a candidate"),
            })?;
        Ok(entry.candidate.source_folders())
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
        self.committed(async move { this.combine_candidates_write(keys).await }).await
    }

    async fn combine_candidates_write(
        &self,
        mut keys: Vec<CandidateKey>,
    ) -> Result<CandidateKey, ImportError> {
        keys.sort();
        keys.dedup();
        let mut members = Vec::with_capacity(keys.len());
        for key in &keys {
            let entry = self.editable_candidate(key).await?;
            require_combination_idle(key, &entry.runtime)?;
            members.push(entry.candidate);
        }
        // A picked release is rebuilt from its members, not from disk, so it
        // cannot itself be picked.
        for member in &members {
            if let Some(grouping) = &member.grouping {
                if let Some(crate::tables::GroupingFacts::Picked { .. }) =
                    self.services.tables.load_grouping(grouping).await?
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
        let key = CandidateKey::grouping(&self.services.ids.new_id().to_string());
        let _commit = self.services.folder_state_commit.lock("combine releases").await;
        let regrouped = self.services.tables.combine_releases(key.clone(), members).await?;
        for candidate_key in keys {
            self.identification_ended(&candidate_key).await;
            self.services
                .event_tx
                .send(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }));
        }
        self.announce_regrouped(&regrouped).await?;
        Ok(key)
    }

    /// The deepest folder under their shared watched folder holding exactly
    /// `members`, if there is one. Fewer than two members are no combination
    /// for a folder to hold: `combine_releases` refuses them.
    async fn folder_holding_exactly(
        &self,
        members: &[crate::FolderCandidate],
    ) -> Result<Option<FolderReleaseDecisionKey>, ImportError> {
        if members.len() < 2 {
            return Ok(None);
        }
        let root = &members[0].watched_folder_path;
        if members.iter().any(|member| &member.watched_folder_path != root) {
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
        let selected: std::collections::BTreeSet<CandidateKey> =
            members.iter().map(crate::FolderCandidate::key).collect();
        let below = self.services.tables.scanned_release_keys_at_or_below(root, &folder).await?;
        if below != selected {
            return Ok(None);
        }
        Ok(Some(FolderReleaseDecisionKey {
            watched_folder_path: root.clone(),
            relative_folder: crate::tables::watched_path::candidate_relative_path(root, &folder)?,
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
        self.services.tables.folder_reading_grouping(&folder).await?.ok_or_else(|| {
            ImportError::Internal {
                detail: format!(
                    "{} was combined but no grouping is stored for it",
                    folder.relative_folder.text()
                ),
            }
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
        self.committed(async move { this.separate_candidate_write(&key).await }).await
    }

    async fn separate_candidate_write(
        &self,
        key: &CandidateKey,
    ) -> Result<Vec<CandidateKey>, ImportError> {
        if let Some(entry) = self.services.tables.load_candidate_entry(key).await? {
            require_combination_idle(key, &entry.runtime)?;
        }
        match self.services.tables.load_grouping(key).await? {
            Some(crate::tables::GroupingFacts::Anchored { folder, .. }) => {
                self.set_folder_release_decision(
                    folder,
                    FolderReleaseDecision::KeepAsSeparateReleases,
                )
                .await
            }
            Some(crate::tables::GroupingFacts::Picked { .. }) => {
                let _commit =
                    self.services.folder_state_commit.lock("separate a picked release").await;
                let (returned, regrouped) =
                    self.services.tables.separate_picked_grouping(key).await?;
                let keys = crate::list::release_keys_in_path_order(&returned);
                self.identification_ended(key).await;
                let automatic = bae_preferences::preferences(&self.services.store)
                    .await?
                    .identification
                    .automatic;
                self.services.event_tx.send(ImportEvent::Scan(ScanEvent::CandidateRemoved {
                    candidate_key: key.clone(),
                }));
                for item in returned {
                    let event = match item {
                        // Each release a separation returns is found, as
                        // combining finds the release it makes.
                        ScanItem::Valid(candidate) => ScanEvent::FolderCandidate {
                            candidate,
                            found_while_automatic: automatic,
                        },
                        ScanItem::Discovered(candidate) => {
                            ScanEvent::CandidateDiscovered { candidate }
                        }
                        ScanItem::Invalid(candidate) => ScanEvent::InvalidCandidate(candidate),
                    };
                    self.services.event_tx.send(ImportEvent::Scan(event));
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
        regrouped: &crate::tables::GroupingChanges,
    ) -> Result<(), ImportError> {
        for candidate_key in &regrouped.removed {
            self.services.event_tx.send(ImportEvent::Scan(ScanEvent::CandidateRemoved {
                candidate_key: candidate_key.clone(),
            }));
        }
        let automatic =
            bae_preferences::preferences(&self.services.store).await?.identification.automatic;
        for item in &regrouped.written {
            let event = match item.clone() {
                ScanItem::Valid(candidate) => ScanEvent::FolderCandidate {
                    found_while_automatic: regrouped.found.contains(&candidate.key()) && automatic,
                    candidate,
                },
                ScanItem::Invalid(candidate) => ScanEvent::InvalidCandidate(candidate),
                ScanItem::Discovered(_) => continue,
            };
            self.services.event_tx.send(ImportEvent::Scan(event));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "combinations_tests.rs"]
mod tests;
