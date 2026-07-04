//! Resolved composer/work types and their pure projections from DB aggregates.

use super::*;
use crate::db::{
    DbComposerSummary, DbReleaseRoleSummary, DbTrackRoleSummary, DbWorkReleaseSummary,
    DbWorkSummary, DbWorkTrackSummary,
};

#[derive(Debug, Clone)]
pub struct ComposerSummary {
    pub raw: DbComposerSummary,
    pub image: Option<ImageRef>,
}

impl ComposerSummary {
    pub(crate) fn from_raw(raw: DbComposerSummary, image: Option<ImageRef>) -> Self {
        Self { raw, image }
    }
}

#[derive(Debug, Clone)]
pub struct WorkSummary {
    pub raw: DbWorkSummary,
    pub representative_cover: Option<ImageRef>,
}

impl WorkSummary {
    pub(crate) fn from_raw(raw: DbWorkSummary, representative_cover: Option<ImageRef>) -> Self {
        Self {
            raw,
            representative_cover,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComposerDetail {
    pub composer: ComposerSummary,
    pub work_groups: Vec<ComposerWorkGroup>,
    pub unlinked_release_roles: Vec<ReleaseRoleSummary>,
    pub unlinked_track_roles: Vec<TrackRoleSummary>,
    pub default_work_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ComposerWorkGroup {
    pub id: String,
    pub parent: Option<WorkSummary>,
    pub works: Vec<WorkSummary>,
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
    pub display_name: String,
    pub format: Option<String>,
    pub cover: Option<ImageRef>,
}

impl WorkReleaseSummary {
    pub(crate) fn from_raw(raw: DbWorkReleaseSummary, cover: Option<ImageRef>) -> Self {
        let display_name = release_display_name(
            raw.release_name.as_deref(),
            raw.year,
            raw.format.as_deref(),
            raw.release_index,
        );
        Self {
            release_id: raw.release_id,
            album_id: raw.album_id,
            album_title: raw.album_title,
            display_name,
            format: raw.format,
            cover,
        }
    }
}

pub type ReleaseRoleSummary = DbReleaseRoleSummary;
pub type TrackRoleSummary = DbTrackRoleSummary;
pub type WorkTrackSummary = DbWorkTrackSummary;
