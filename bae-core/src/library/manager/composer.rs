//! Composer and work domain operations for [`LibraryManager`].

use super::*;

/// Desktop-only, with the import reconcile that is its one caller.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone)]
pub(crate) struct ResolvedImportWorks {
    /// One id per input work, in input order.
    pub ids: Vec<String>,
    /// The works this library has not seen before, for finalize to insert.
    pub inserts: Vec<crate::db::DbWork>,
}

impl LibraryManager {
    pub async fn get_composer_count(&self) -> Result<u64, LibraryError> {
        Ok(self.database.get_composer_count().await?)
    }

    pub async fn get_composer_page(
        &self,
        sort: &[crate::db::ComposerSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<ComposerSummary>, LibraryError> {
        let raw = self.database.get_composer_page(sort, offset, limit).await?;
        let artist_ids: Vec<String> = raw.iter().map(|c| c.artist.id.clone()).collect();
        let images = self.artist_image_refs(&artist_ids).await?;
        Ok(raw
            .into_iter()
            .map(|composer| {
                let image = images.get(&composer.artist.id).cloned();
                ComposerSummary::from_raw(composer, image)
            })
            .collect())
    }

    pub(crate) fn subscribe_composer_page(
        &self,
        sort: &[crate::db::ComposerSortCriterion],
        offset: u64,
        limit: u64,
    ) -> coven::LiveQuery<crate::db::ComposerPageProjection> {
        self.database.subscribe_composer_page(sort, offset, limit)
    }

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

    pub(crate) fn resolve_composer_page(
        &self,
        projection: crate::db::ComposerPageProjection,
    ) -> (Vec<ComposerSummary>, u64) {
        let images = composer_image_refs(projection.image_versions);
        let rows = resolve_composer_rows(projection.rows, &images);
        (rows, projection.total_count)
    }

    pub(crate) fn resolve_composer_browse(
        &self,
        projection: crate::db::ComposerBrowseProjection,
        request_revision: u64,
        cause: coven::ReconfigurableLiveQueryCause,
    ) -> crate::library::LibraryBrowseSnapshot<ComposerSummary> {
        let images = composer_image_refs(projection.image_versions);
        crate::library::LibraryBrowseSnapshot {
            windows: projection
                .windows
                .into_iter()
                .map(|window| crate::library::LibraryBrowseWindow {
                    window: window.window,
                    rows: resolve_composer_rows(window.rows, &images),
                })
                .collect(),
            total_count: projection.total_count,
            request_revision,
            cause,
        }
    }

    pub async fn get_composer_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<ComposerDetail>, LibraryError> {
        let Some(raw) = self.database.find_composer_detail(artist_id).await? else {
            return Ok(None);
        };
        let composer_images = self
            .artist_image_refs(std::slice::from_ref(&raw.composer.artist.id))
            .await?;
        let release_ids: Vec<String> = raw
            .work_groups
            .iter()
            .flat_map(|group| {
                group
                    .parent
                    .iter()
                    .chain(group.works.iter())
                    .filter_map(|work| work.representative_release_id.clone())
            })
            .collect();
        let covers = self.cover_refs(&release_ids).await?;
        let work_groups: Vec<ComposerWorkGroup> = raw
            .work_groups
            .into_iter()
            .map(|group| ComposerWorkGroup {
                id: group.id,
                parent: group
                    .parent
                    .map(|parent| work_summary_with_cover(parent, &covers)),
                works: group
                    .works
                    .into_iter()
                    .map(|work| work_summary_with_cover(work, &covers))
                    .collect(),
            })
            .collect();
        let default_work_id = work_groups.first().and_then(|group| {
            group
                .parent
                .as_ref()
                .map(|work| work.raw.work.id.clone())
                .or_else(|| group.works.first().map(|work| work.raw.work.id.clone()))
        });
        Ok(Some(ComposerDetail {
            composer: {
                let image = composer_images.get(&raw.composer.artist.id).cloned();
                ComposerSummary::from_raw(raw.composer, image)
            },
            work_groups,
            unlinked_release_roles: raw.unlinked_release_roles,
            unlinked_track_roles: raw.unlinked_track_roles,
            default_work_id,
        }))
    }

    pub(crate) fn subscribe_composer_detail(
        &self,
        artist_id: &str,
    ) -> coven::LiveQuery<crate::db::ComposerDetailProjection> {
        self.database.subscribe_composer_detail(artist_id)
    }

    pub(crate) fn resolve_composer_detail_projection(
        &self,
        projection: crate::db::ComposerDetailProjection,
    ) -> Option<ComposerDetail> {
        let raw = projection.detail?;
        let images = projection
            .image_versions
            .into_iter()
            .map(|(id, version)| {
                (
                    id.clone(),
                    ImageRef {
                        id,
                        version,
                        image_type: crate::db::LibraryImageType::Artist,
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let covers = projection
            .cover_versions
            .into_iter()
            .map(|(id, version)| {
                (
                    id.clone(),
                    ImageRef {
                        id,
                        version,
                        image_type: crate::db::LibraryImageType::Cover,
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let work_groups = raw
            .work_groups
            .into_iter()
            .map(|group| ComposerWorkGroup {
                id: group.id,
                parent: group
                    .parent
                    .map(|parent| work_summary_with_cover(parent, &covers)),
                works: group
                    .works
                    .into_iter()
                    .map(|work| work_summary_with_cover(work, &covers))
                    .collect(),
            })
            .collect::<Vec<_>>();
        let default_work_id = work_groups.first().and_then(|group| {
            group
                .parent
                .as_ref()
                .map(|work| work.raw.work.id.clone())
                .or_else(|| group.works.first().map(|work| work.raw.work.id.clone()))
        });
        let image = images.get(&raw.composer.artist.id).cloned();
        Some(ComposerDetail {
            composer: ComposerSummary::from_raw(raw.composer, image),
            work_groups,
            unlinked_release_roles: raw.unlinked_release_roles,
            unlinked_track_roles: raw.unlinked_track_roles,
            default_work_id,
        })
    }

    /// Resolve each parsed work to the `works` row already holding its
    /// MusicBrainz id, or to a row for finalize to insert. `ids` comes back in
    /// input order, so a caller can zip it with `works` to remap the parsed ids
    /// its work links carry. Desktop-only, with the import that calls it.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
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
                    ids.push(work.id.clone());
                    inserts.push(work.clone());
                }
            }
        }

        Ok(ResolvedImportWorks { ids, inserts })
    }

    pub async fn get_work_detail(&self, work_id: &str) -> Result<Option<WorkDetail>, LibraryError> {
        let Some(raw) = self.database.find_work_detail(work_id).await? else {
            return Ok(None);
        };
        let release_ids: Vec<String> = raw.releases.iter().map(|r| r.release_id.clone()).collect();
        let covers = self.cover_refs(&release_ids).await?;
        let work_cover = raw
            .work
            .representative_release_id
            .as_ref()
            .and_then(|id| covers.get(id).cloned());
        let child_release_ids: Vec<String> = raw
            .child_works
            .iter()
            .filter_map(|work| work.representative_release_id.clone())
            .collect();
        let child_covers = self.cover_refs(&child_release_ids).await?;
        Ok(Some(WorkDetail {
            work: WorkSummary::from_raw(raw.work, work_cover),
            child_works: raw
                .child_works
                .into_iter()
                .map(|work| work_summary_with_cover(work, &child_covers))
                .collect(),
            releases: raw
                .releases
                .into_iter()
                .map(|release| {
                    let cover = covers.get(&release.release_id).cloned();
                    WorkReleaseSummary::from_raw(release, cover)
                })
                .collect(),
            tracks: raw.tracks,
        }))
    }

    pub(crate) fn subscribe_work_detail(
        &self,
        work_id: &str,
    ) -> coven::LiveQuery<crate::db::WorkDetailProjection> {
        self.database.subscribe_work_detail(work_id)
    }

    pub(crate) fn resolve_work_detail_projection(
        &self,
        projection: crate::db::WorkDetailProjection,
    ) -> Option<WorkDetail> {
        let raw = projection.detail?;
        let covers = projection
            .cover_versions
            .into_iter()
            .map(|(id, version)| {
                (
                    id.clone(),
                    ImageRef {
                        id,
                        version,
                        image_type: crate::db::LibraryImageType::Cover,
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let work_cover = raw
            .work
            .representative_release_id
            .as_ref()
            .and_then(|id| covers.get(id).cloned());
        Some(WorkDetail {
            work: WorkSummary::from_raw(raw.work, work_cover),
            child_works: raw
                .child_works
                .into_iter()
                .map(|work| work_summary_with_cover(work, &covers))
                .collect(),
            releases: raw
                .releases
                .into_iter()
                .map(|release| {
                    let cover = covers.get(&release.release_id).cloned();
                    WorkReleaseSummary::from_raw(release, cover)
                })
                .collect(),
            tracks: raw.tracks,
        })
    }
}

fn composer_image_refs(versions: HashMap<String, String>) -> HashMap<String, ImageRef> {
    versions
        .into_iter()
        .map(|(id, version)| {
            let image = ImageRef {
                id: id.clone(),
                version,
                image_type: crate::db::LibraryImageType::Artist,
            };
            (id, image)
        })
        .collect()
}

fn resolve_composer_rows(
    rows: Vec<crate::db::DbComposerSummary>,
    images: &HashMap<String, ImageRef>,
) -> Vec<ComposerSummary> {
    rows.into_iter()
        .map(|row| {
            let image = images.get(&row.artist.id).cloned();
            ComposerSummary::from_raw(row, image)
        })
        .collect()
}

fn work_summary_with_cover(
    work: crate::db::DbWorkSummary,
    covers: &HashMap<String, ImageRef>,
) -> WorkSummary {
    let cover = work
        .representative_release_id
        .as_ref()
        .and_then(|id| covers.get(id).cloned());
    WorkSummary::from_raw(work, cover)
}
