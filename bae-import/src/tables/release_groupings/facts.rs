//! How one grouping reads, as the actions on its release need it.

use super::*;

/// How a grouping reads, as the actions on its release need it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupingFacts {
    /// A folder's reading: the folder, and whether its releases are one.
    Anchored { folder: FolderReleaseDecisionKey, decision: FolderReleaseDecision },
    /// Releases the person read as one, by key, in play order.
    Picked { members: Vec<CandidateKey> },
}

// The grouping `key`, a row beside each watched folder: the folder an
// anchored grouping reads is addressed below the one covering it.
sqlive_compose::read!(
    GroupingWithWatched(key: crate::CandidateKey),
    r#"
    SELECT stored.*, watched.*
    FROM @crate::tables::release_groupings::fragments::Groupings(
        keys: (SELECT :key AS grouping_key)) AS stored
    LEFT JOIN @crate::tables::watched_folders::fragments::WatchedFolderRows AS watched ON 1
"#
);

/// How the grouping `rows` of [`GroupingWithWatched`] are of reads, or `None`
/// when there are none.
fn grouping_facts(rows: Vec<GroupingWithWatchedRow>) -> Result<Option<GroupingFacts>, DbError> {
    let mut watched = Vec::with_capacity(rows.len());
    let mut stored = None;
    for row in rows {
        if let Some(folder) = row.watched {
            watched.push(super::path_column::decode(folder.watched_path)?);
        }
        stored = Some(row.stored);
    }
    let Some(stored) = stored else {
        return Ok(None);
    };
    let Some(anchor) = stored.anchor_folder else {
        return Ok(Some(GroupingFacts::Picked {
            members: stored
                .members
                .into_iter()
                .map(|member| CandidateKey::from(member.key))
                .collect(),
        }));
    };
    let anchor = super::path_column::decode(anchor)?;
    // A grouping anchored at a folder goes with the watched folder covering
    // it, and watched folders never overlap.
    let root = watched.into_iter().find(|root| anchor.starts_with(root)).ok_or_else(|| {
        DbError::unreadable("release_grouping.anchor_folder under no watched folder", &anchor)
    })?;
    Ok(Some(GroupingFacts::Anchored {
        folder: FolderReleaseDecisionKey {
            relative_folder: super::watched_folders::relative_below(&root, &anchor)?,
            watched_folder_path: root,
        },
        decision: decision_of(stored.combined),
    }))
}

impl ImportTables {
    /// How the grouping `key` reads, or `None` when no grouping has that key.
    pub(crate) async fn load_grouping(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<GroupingFacts>, DbError> {
        let request = GroupingWithWatchedParams { key: key.clone() };
        let rows = self.store.read_rows(request).await?;
        grouping_facts(rows)
    }
}
