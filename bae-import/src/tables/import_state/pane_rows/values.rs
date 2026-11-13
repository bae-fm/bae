//! The draft's values besides its editable fields: the roles the source
//! credits on each track and on the release, and the works the tracks perform
//! with the works they nest among.
//!
//! They hang off the draft row, and each track's off its track row.
//! Its tables are `values.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("values.sql");

use super::*;
use bae_catalogs::credits::CreditedRole;
use bae_catalogs::works::{GraphWork, WorkCredit, WorkGraph, WorkPart};
use bae_types::credit_role::CreditedAs;

/// Write the values `draft` holds beside its rows besides its tracks': the
/// works and the roles on the release. The draft row is already written.
pub(super) fn insert_draft_values(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    draft: &CandidateDraft,
) -> Result<(), DbError> {
    for (position, work) in draft.works.works.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_work \
             (content_hash, musicbrainz_work_id, position, title, disambiguation, work_type) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                work.musicbrainz_work_id,
                position as i64,
                work.title,
                work.disambiguation,
                work.work_type,
            ],
        )?;
        for (position, credit) in work.credits.iter().enumerate() {
            let role = credit.role.columns();
            sql.execute(
                "INSERT INTO import_candidate_work_credit \
                 (content_hash, musicbrainz_work_id, position, name, sort_name, \
                  musicbrainz_artist_id, discogs_artist_id, credited_as, role, role_words, \
                  qualifiers) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    content_hash,
                    work.musicbrainz_work_id,
                    position as i64,
                    credit.artist.name,
                    credit.artist.sort_name,
                    credit.artist.musicbrainz_artist_id,
                    credit.artist.discogs_artist_id,
                    credit.artist.credited_as,
                    role.role,
                    role.role_words,
                    role.qualifiers,
                ],
            )?;
        }
    }
    for (position, part) in draft.works.parts.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_work_part \
             (content_hash, position, parent_work_id, child_work_id) VALUES (?, ?, ?, ?)",
            params![content_hash, position as i64, part.parent, part.child],
        )?;
    }
    for (position, role) in draft.release_roles.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_release_role \
             (content_hash, position, name, sort_name, musicbrainz_artist_id, \
              discogs_artist_id, credited_as, source, role) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                position as i64,
                role.artist.name,
                role.artist.sort_name,
                role.artist.musicbrainz_artist_id,
                role.artist.discogs_artist_id,
                role.artist.credited_as,
                role.source.as_str(),
                role.role,
            ],
        )?;
    }
    Ok(())
}

/// Write the values hanging off one draft track: the roles the source
/// credits on it and the works it performs. The track row is already
/// written, and the works are.
pub(super) fn insert_track_values(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    track: &CandidateTrack,
) -> Result<(), DbError> {
    for (position, role) in track.roles.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_track_role \
             (content_hash, track_id, position, name, sort_name, musicbrainz_artist_id, \
              discogs_artist_id, credited_as, source, role) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                track.edit.id,
                position as i64,
                role.artist.name,
                role.artist.sort_name,
                role.artist.musicbrainz_artist_id,
                role.artist.discogs_artist_id,
                role.artist.credited_as,
                role.source.as_str(),
                role.role,
            ],
        )?;
    }
    for (position, work) in track.works.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_track_work \
             (content_hash, track_id, position, musicbrainz_work_id) VALUES (?, ?, ?, ?)",
            params![content_hash, track.edit.id, position as i64, work],
        )?;
    }
    Ok(())
}

/// The works the draft `edit` holds, each with its credits, as a JSON array
/// in the draft's order; decoded by [`works_of`].
pub(super) const WORKS_JSON: &str = "(SELECT json_group_array(json_object( \
         'musicbrainz_work_id', work.musicbrainz_work_id, 'title', work.title, \
         'disambiguation', work.disambiguation, 'work_type', work.work_type, \
         'credits', json((SELECT json_group_array(json_object( \
                 'name', credit.name, 'sort_name', credit.sort_name, \
                 'musicbrainz_artist_id', credit.musicbrainz_artist_id, \
                 'discogs_artist_id', credit.discogs_artist_id, \
                 'credited_as', credit.credited_as, 'role', credit.role, \
                 'role_words', credit.role_words, 'qualifiers', credit.qualifiers) \
                 ORDER BY credit.position) \
             FROM import_candidate_work_credit AS credit \
             WHERE credit.content_hash = work.content_hash \
               AND credit.musicbrainz_work_id = work.musicbrainz_work_id))) \
         ORDER BY work.position) \
     FROM import_candidate_work AS work WHERE work.content_hash = edit.content_hash)";

/// Which of the draft `edit`'s works is part of which, as a JSON array in
/// the order the source relates them.
pub(super) const WORK_PARTS_JSON: &str = "(SELECT json_group_array(json_object( \
         'parent', part.parent_work_id, 'child', part.child_work_id) ORDER BY part.position) \
     FROM import_candidate_work_part AS part WHERE part.content_hash = edit.content_hash)";

/// The roles the source credits on the draft `edit`'s release, as a JSON
/// array in its order; decoded by [`roles_of`].
pub(super) const RELEASE_ROLES_JSON: &str = "(SELECT json_group_array(json_object( \
         'name', role.name, 'sort_name', role.sort_name, \
         'musicbrainz_artist_id', role.musicbrainz_artist_id, \
         'discogs_artist_id', role.discogs_artist_id, \
         'credited_as', role.credited_as, 'source', role.source, \
         'role', role.role) ORDER BY role.position) \
     FROM import_candidate_release_role AS role WHERE role.content_hash = edit.content_hash)";

/// The roles the source credits on the draft track `track`, as a JSON array
/// in its order; decoded by [`roles_of`].
pub(super) const TRACK_ROLES_JSON: &str = "(SELECT json_group_array(json_object( \
         'name', role.name, 'sort_name', role.sort_name, \
         'musicbrainz_artist_id', role.musicbrainz_artist_id, \
         'discogs_artist_id', role.discogs_artist_id, \
         'credited_as', role.credited_as, 'source', role.source, \
         'role', role.role) ORDER BY role.position) \
     FROM import_candidate_track_role AS role \
     WHERE role.content_hash = track.content_hash AND role.track_id = track.track_id)";

/// The works the draft track `track` performs, as a JSON array of their ids
/// in its order.
pub(super) const TRACK_WORKS_JSON: &str = "(SELECT json_group_array( \
         performed.musicbrainz_work_id ORDER BY performed.position) \
     FROM import_candidate_track_work AS performed \
     WHERE performed.content_hash = track.content_hash AND performed.track_id = track.track_id)";

/// One stored work as [`WORKS_JSON`] holds it.
#[derive(serde::Deserialize)]
pub(super) struct StoredWork {
    musicbrainz_work_id: String,
    title: String,
    disambiguation: Option<String>,
    work_type: Option<String>,
    credits: Vec<StoredWorkCredit>,
}

/// One stored work credit as [`WORKS_JSON`] holds it.
#[derive(serde::Deserialize)]
pub(super) struct StoredWorkCredit {
    name: String,
    sort_name: Option<String>,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
    credited_as: Option<String>,
    role: String,
    role_words: Option<String>,
    qualifiers: String,
}

/// One stored part as [`WORK_PARTS_JSON`] holds it.
#[derive(serde::Deserialize)]
pub(super) struct StoredPart {
    parent: String,
    child: String,
}

/// The work graph [`WORKS_JSON`] and [`WORK_PARTS_JSON`] hold.
pub(super) fn works_of(
    works: Vec<StoredWork>,
    parts: Vec<StoredPart>,
) -> Result<WorkGraph, DbError> {
    Ok(WorkGraph {
        works: works
            .into_iter()
            .map(|work| {
                Ok(GraphWork {
                    musicbrainz_work_id: work.musicbrainz_work_id,
                    title: work.title,
                    disambiguation: work.disambiguation,
                    work_type: work.work_type,
                    credits: work
                        .credits
                        .into_iter()
                        .map(|credit| {
                            Ok(WorkCredit {
                                artist: ArtistCredit {
                                    name: credit.name,
                                    sort_name: credit.sort_name,
                                    musicbrainz_artist_id: credit.musicbrainz_artist_id,
                                    discogs_artist_id: credit.discogs_artist_id,
                                    credited_as: credit.credited_as,
                                },
                                role: CreditedAs::from_columns(
                                    &credit.role,
                                    credit.role_words,
                                    &credit.qualifiers,
                                )
                                .map_err(|error| {
                                    DbError::unreadable("a draft work credit's role", error)
                                })?,
                            })
                        })
                        .collect::<Result<_, DbError>>()?,
                })
            })
            .collect::<Result<_, DbError>>()?,
        parts: parts
            .into_iter()
            .map(|part| WorkPart { parent: part.parent, child: part.child })
            .collect(),
    })
}

/// One stored role as [`RELEASE_ROLES_JSON`] and [`TRACK_ROLES_JSON`] hold
/// it, its source not yet read.
#[derive(serde::Deserialize)]
pub(super) struct StoredRole {
    name: String,
    sort_name: Option<String>,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
    credited_as: Option<String>,
    source: String,
    role: Option<String>,
}

/// The credited roles a roles column holds.
pub(super) fn roles_of(roles: Vec<StoredRole>) -> Result<Vec<CreditedRole>, DbError> {
    roles
        .into_iter()
        .map(|stored| {
            Ok(CreditedRole {
                artist: ArtistCredit {
                    name: stored.name,
                    sort_name: stored.sort_name,
                    musicbrainz_artist_id: stored.musicbrainz_artist_id,
                    discogs_artist_id: stored.discogs_artist_id,
                    credited_as: stored.credited_as,
                },
                source: Catalog::from_str(&stored.source).map_err(|_unknown| {
                    DbError::unreadable("a draft role's stored source", &stored.source)
                })?,
                role: stored.role,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "values_tests.rs"]
mod tests;
