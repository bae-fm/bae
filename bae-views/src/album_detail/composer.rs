//! Resolved composer/work types and their pure projections from DB aggregates.

use super::*;
use crate::{
    DbComposerSummary, DbReleaseRoleSummary, DbTrackRoleSummary, DbWorkReleaseSummary,
    DbWorkSummary, DbWorkTrackSummary,
};

#[derive(Debug, Clone)]
pub struct ComposerSummary {
    pub raw: DbComposerSummary,
}

impl ComposerSummary {
    /// The composer as its row reads it, image and all.
    pub(crate) fn from_raw(raw: DbComposerSummary) -> Self {
        Self { raw }
    }

    /// The line under the composer's name in its list, its detail and
    /// search: how many works it is credited on.
    pub fn count_line(&self) -> bae_types::count_label::CountLabel {
        bae_types::count_label::CountLabel {
            key: "core.library.work_count".to_string(),
            count: u32::try_from(self.raw.work_count).expect("a work count is never negative"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkSummary {
    pub raw: DbWorkSummary,
}

impl WorkSummary {
    /// The work as its row reads it, its representative release's cover and
    /// all.
    pub(crate) fn from_raw(raw: DbWorkSummary) -> Self {
        Self { raw }
    }
}

/// A composer's page above its rows: the composer, and the work the page
/// opens first, the first its rows list.
#[derive(Debug, Clone)]
pub struct ComposerDetail {
    pub composer: ComposerSummary,
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
        work: WorkSummary,
        part: bool,
    },
    ReleaseCredit(ReleaseRoleSummary),
    TrackCredit(TrackRoleSummary),
}

#[derive(Debug, Clone)]
pub struct WorkDetail {
    pub work: WorkSummary,
    pub child_works: Vec<WorkSummary>,
    pub releases: Vec<WorkReleaseSummary>,
    pub tracks: Vec<WorkTrackSummary>,
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

pub type ReleaseRoleSummary = DbReleaseRoleSummary;
pub type TrackRoleSummary = DbTrackRoleSummary;
pub type WorkTrackSummary = DbWorkTrackSummary;
