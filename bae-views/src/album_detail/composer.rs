//! Resolved composer/work types and their pure projections from DB aggregates.

use super::*;
use crate::{
    DbComposerSummary, DbReleaseRoleSummary, DbTrackRoleSummary, DbWorkReleaseSummary,
    DbWorkSummary, DbWorkTrackSummary,
};

/// A composer's page above its rows: the composer, and the work the page
/// opens first, the first its rows list.
#[derive(Debug, Clone)]
pub struct ComposerDetail {
    pub composer: DbComposerSummary,
    pub default_work_id: Option<String>,
}

/// One row of a composer's page: its id, the heading it opens when it is the
/// first of its part of the page, and what it shows.
#[derive(Debug, Clone)]
pub struct ComposerDetailRow {
    pub id: i64,
    pub heading: Option<ComposerHeading>,
    pub item: ComposerDetailItem,
}

/// The heading over a part of a composer's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerHeading {
    /// Over its works.
    Works,
    /// Over its unlinked release credits.
    Credits,
    /// Over its unlinked track credits.
    Recordings,
}

/// What a row of a composer's page shows.
#[derive(Debug, Clone)]
pub enum ComposerDetailItem {
    /// A work, `part` when it is listed under the work heading its group.
    Work {
        work: DbWorkSummary,
        part: bool,
    },
    ReleaseCredit(DbReleaseRoleSummary),
    TrackCredit(DbTrackRoleSummary),
}

#[derive(Debug, Clone)]
pub struct WorkDetail {
    pub work: DbWorkSummary,
    pub child_works: Vec<DbWorkSummary>,
    pub releases: Vec<WorkReleaseSummary>,
    pub tracks: Vec<DbWorkTrackSummary>,
}

#[derive(Debug, Clone)]
pub struct WorkReleaseSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub name: ReleaseName,
    pub media: Vec<bae_types::pressing::MediaCount>,
    pub cover: Option<ImageRef>,
}

impl WorkReleaseSummary {
    pub(crate) fn from_raw(raw: DbWorkReleaseSummary) -> Self {
        let cover = cover_ref(&raw.release_id, raw.cover_version);
        let name =
            ReleaseName::of(raw.release_name.as_deref(), raw.year, &raw.media, raw.release_index);
        Self {
            release_id: raw.release_id,
            album_id: raw.album_id,
            album_title: raw.album_title,
            name,
            media: raw.media,
            cover,
        }
    }
}
