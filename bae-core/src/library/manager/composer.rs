//! Composer and work domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    pub(crate) fn subscribe_composer_browse(
        &self,
        sort: &[crate::db::ComposerSortCriterion],
        initial_windows: crate::db::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        crate::db::LibraryPageWindows,
        crate::db::ComposerBrowseProjection,
    > {
        self.database.subscribe_composer_browse(sort, initial_windows)
    }

    pub(crate) fn resolve_composer_browse(
        &self,
        projection: crate::db::ComposerBrowseProjection,
        request_revision: u64,
        cause: coven::LiveQueryCause,
    ) -> crate::library::LibraryBrowseSnapshot<ComposerSummary> {
        crate::library::LibraryBrowseSnapshot {
            windows: projection
                .windows
                .into_iter()
                .map(|window| crate::db::LibraryBrowseWindow {
                    window: window.window,
                    rows: window.rows.into_iter().map(ComposerSummary::from_raw).collect(),
                })
                .collect(),
            total_count: projection.total_count,
            row_count: projection.total_count,
            sections: Vec::new(),
            request_revision,
            cause,
        }
    }

    pub(crate) fn subscribe_composer_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, crate::db::ComposerDetailProjection> {
        self.database.subscribe_composer_detail(initial)
    }

    pub(crate) fn resolve_composer_detail_projection(
        &self,
        projection: crate::db::ComposerDetailProjection,
    ) -> Option<ComposerDetail> {
        let raw = projection.detail?;
        let work_groups = raw
            .work_groups
            .into_iter()
            .map(|group| ComposerWorkGroup {
                id: group.id,
                parent: group.parent.map(WorkSummary::from_raw),
                works: group.works.into_iter().map(WorkSummary::from_raw).collect(),
            })
            .collect::<Vec<_>>();
        let default_work_id = work_groups.first().and_then(|group| {
            group
                .parent
                .as_ref()
                .map(|work| work.raw.work.id.clone())
                .or_else(|| group.works.first().map(|work| work.raw.work.id.clone()))
        });
        Some(ComposerDetail {
            composer: ComposerSummary::from_raw(raw.composer),
            work_groups,
            unlinked_release_roles: raw.unlinked_release_roles,
            unlinked_track_roles: raw.unlinked_track_roles,
            default_work_id,
        })
    }

    #[cfg(test)]
    pub async fn get_work_detail(&self, work_id: &str) -> Result<Option<WorkDetail>, LibraryError> {
        let Some(raw) = self.database.find_work_detail(work_id).await? else {
            return Ok(None);
        };
        Ok(Some(work_detail_of(raw)))
    }

    pub(crate) fn subscribe_work_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, crate::db::WorkDetailProjection> {
        self.database.subscribe_work_detail(initial)
    }

    pub(crate) fn resolve_work_detail_projection(
        &self,
        projection: crate::db::WorkDetailProjection,
    ) -> Option<WorkDetail> {
        projection.detail.map(work_detail_of)
    }
}

/// A work's page as its rows read it, each with its cover's version.
fn work_detail_of(raw: crate::db::DbWorkDetail) -> WorkDetail {
    WorkDetail {
        work: WorkSummary::from_raw(raw.work),
        child_works: raw.child_works.into_iter().map(WorkSummary::from_raw).collect(),
        releases: raw.releases.into_iter().map(WorkReleaseSummary::from_raw).collect(),
        tracks: raw.tracks,
    }
}
