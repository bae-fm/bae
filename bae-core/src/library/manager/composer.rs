//! Composer and work domain operations for [`LibraryManager`].

use super::*;

/// Desktop-only, with the import reconcile that is its one caller.
#[cfg(desktop)]
#[derive(Debug, Clone)]
pub(crate) struct ResolvedImportWorks {
    /// One id per input work, in input order.
    pub ids: Vec<String>,
    /// The works this library has not seen before, for finalize to insert.
    pub inserts: Vec<crate::db::DbWork>,
}

impl LibraryManager {
    pub(crate) fn subscribe_composer_browse(
        &self,
        sort: &[crate::db::ComposerSortCriterion],
        initial_windows: crate::library::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        crate::library::LibraryPageWindows,
        crate::db::ComposerBrowseProjection,
    > {
        self.database
            .subscribe_composer_browse(sort, initial_windows)
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
                .map(|window| crate::library::LibraryBrowseWindow {
                    window: window.window,
                    rows: window
                        .rows
                        .into_iter()
                        .map(ComposerSummary::from_raw)
                        .collect(),
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

    /// Resolve each parsed work to the `works` row already holding its
    /// MusicBrainz id, or to a row for finalize to insert under the id that
    /// MusicBrainz id names. `ids` comes back in
    /// input order, so a caller can zip it with `works` to remap the parsed ids
    /// its work links carry. Desktop-only, with the import that calls it.
    #[cfg(desktop)]
    pub(crate) async fn resolve_works_for_import(
        &self,
        works: &[crate::db::DbWork],
    ) -> Result<ResolvedImportWorks, LibraryError> {
        let mut ids = Vec::with_capacity(works.len());
        let mut inserts = Vec::new();

        for work in works {
            match self
                .database
                .find_work_id_by_musicbrainz_id(&work.musicbrainz_work_id)
                .await?
            {
                Some(existing_id) => ids.push(existing_id),
                None => {
                    // The work's id is its MusicBrainz id's, so every device
                    // that meets this work writes one row.
                    let work = crate::db::DbWork {
                        id: crate::db::identity::work_id(&work.musicbrainz_work_id),
                        ..work.clone()
                    };
                    ids.push(work.id.clone());
                    inserts.push(work);
                }
            }
        }

        Ok(ResolvedImportWorks { ids, inserts })
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
        child_works: raw
            .child_works
            .into_iter()
            .map(WorkSummary::from_raw)
            .collect(),
        releases: raw
            .releases
            .into_iter()
            .map(WorkReleaseSummary::from_raw)
            .collect(),
        tracks: raw.tracks,
    }
}
