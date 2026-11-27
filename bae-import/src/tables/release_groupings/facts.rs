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

// The grouping `key`, and beside it, for an anchored one, the watched folder
// covering its anchor: the one whose path the anchor is, or starts with up to
// a path separator. Watched folders never overlap, so at most one covers it.
sqlive_compose::read!(
    GroupingWithWatched(key: crate::CandidateKey, separator: Vec<u8>),
    r#"
    SELECT stored.*, watched.*
    FROM @crate::tables::release_groupings::fragments::Groupings(
        keys: (SELECT :key AS grouping_key)) AS stored
    LEFT JOIN @crate::tables::watched_folders::fragments::WatchedFolderRows AS watched
        ON substr(stored.anchor_folder, 1, length(watched.watched_path)) = watched.watched_path
        AND (length(stored.anchor_folder) = length(watched.watched_path)
            OR substr(stored.anchor_folder, length(watched.watched_path) + 1, 1) = :separator
            OR substr(watched.watched_path, -1) = :separator)
"#
);

/// How the grouping a row of [`GroupingWithWatched`] is of reads.
fn grouping_facts(row: GroupingWithWatchedRow) -> Result<GroupingFacts, DbError> {
    let GroupingWithWatchedRow { stored, watched } = row;
    let Some(anchor) = stored.anchor_folder else {
        return Ok(GroupingFacts::Picked {
            members: stored
                .members
                .into_iter()
                .map(|member| CandidateKey::from(member.key))
                .collect(),
        });
    };
    let anchor = crate::tables::path_column::decode(anchor)?;
    // A grouping anchored at a folder goes with the watched folder covering
    // it.
    let root = watched
        .ok_or_else(|| {
            DbError::unreadable("release_grouping.anchor_folder under no watched folder", &anchor)
        })
        .and_then(|watched| crate::tables::path_column::decode(watched.watched_path))?;
    Ok(GroupingFacts::Anchored {
        folder: FolderReleaseDecisionKey {
            relative_folder: crate::tables::watched_folders::relative_below(&root, &anchor)?,
            watched_folder_path: root,
        },
        decision: decision_of(stored.combined),
    })
}

impl ImportTables {
    /// How the grouping `key` reads, or `None` when no grouping has that key.
    pub(crate) async fn load_grouping(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<GroupingFacts>, DbError> {
        let request = GroupingWithWatchedParams {
            key: key.clone(),
            separator: vec![std::path::MAIN_SEPARATOR as u8],
        };
        self.store
            .read(move |sql| bae_store::statement_optional(&sql, &request))
            .await?
            .map(grouping_facts)
            .transpose()
    }
}
