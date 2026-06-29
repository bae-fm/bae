//! Resolved composer/work types and their pure projections from DB aggregates.

use super::*;
use crate::db::{
    DbComposerSummary, DbReleaseRoleSummary, DbTrackRoleSummary, DbWorkSummary, DbWorkTrackSummary,
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
    pub releases: Vec<ReleaseSummary>,
    pub tracks: Vec<WorkTrackSummary>,
}

pub type ReleaseRoleSummary = DbReleaseRoleSummary;
pub type TrackRoleSummary = DbTrackRoleSummary;
pub type WorkTrackSummary = DbWorkTrackSummary;
