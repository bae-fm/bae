//! The draft's values besides its editable fields, as the statements that
//! read them from a candidate's tables: the roles the source credits on each
//! track and on the release, and the works the tracks perform with the works
//! they nest among. `import_state::draft_writes` writes them.

use bae_catalogs::catalog::Catalog;
use bae_catalogs::credits::ArtistCredit;
use bae_catalogs::credits::CreditedRole;
use bae_catalogs::works::{GraphWork, WorkCredit, WorkGraph, WorkPart};
use bae_store::DbError;
use bae_types::credit_role::CreditedAs;
use std::str::FromStr;

/// The works of the owner `owner` (an SQL expression for its key), each with
/// its credits, as a JSON array in the draft's order, which [`works_of`]
/// decodes.
pub(crate) fn works_json_sql(owner: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
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
         FROM import_candidate_work AS work WHERE work.content_hash = {owner})",
    )
}

/// Which of the owner `owner`'s works is part of which, as a JSON array in
/// the order the source relates them.
pub(crate) fn work_parts_json_sql(owner: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
             'parent', part.parent_work_id, 'child', part.child_work_id) ORDER BY part.position) \
         FROM import_candidate_work_part AS part WHERE part.content_hash = {owner})",
    )
}

/// The columns of one role as a JSON object, which [`StoredRole`] reads.
const ROLE_JSON: &str = "json_object( \
     'name', role.name, 'sort_name', role.sort_name, \
     'musicbrainz_artist_id', role.musicbrainz_artist_id, \
     'discogs_artist_id', role.discogs_artist_id, \
     'credited_as', role.credited_as, 'source', role.source, \
     'role', role.role, 'role_words', role.role_words, \
     'qualifiers', role.qualifiers)";

/// The roles the source credits on the owner `owner`'s release, as a JSON
/// array in its order, which [`roles_of`] decodes.
pub(crate) fn release_roles_json_sql(owner: &str) -> String {
    format!(
        "(SELECT json_group_array({ROLE_JSON} ORDER BY role.position) \
         FROM import_candidate_release_role AS role WHERE role.content_hash = {owner})",
    )
}

/// The roles the source credits on the owner `owner`'s track `track` (SQL
/// expressions for their keys), as a JSON array in its order, which
/// [`roles_of`] decodes.
pub(crate) fn track_roles_json_sql(owner: &str, track: &str) -> String {
    format!(
        "(SELECT json_group_array({ROLE_JSON} ORDER BY role.position) \
         FROM import_candidate_track_role AS role WHERE role.content_hash = {owner} AND role.track_id = {track})",
    )
}

/// The works the owner `owner`'s track `track` performs, as a JSON array of
/// their ids in its order.
pub(crate) fn track_works_json_sql(owner: &str, track: &str) -> String {
    format!(
        "(SELECT json_group_array(performed.musicbrainz_work_id ORDER BY performed.position) \
         FROM import_candidate_track_work AS performed \
         WHERE performed.content_hash = {owner} AND performed.track_id = {track})",
    )
}

/// One stored work as [`works_json_sql`] holds it.
#[derive(serde::Deserialize)]
pub(crate) struct StoredWork {
    musicbrainz_work_id: String,
    title: String,
    disambiguation: Option<String>,
    work_type: Option<String>,
    credits: Vec<StoredWorkCredit>,
}

/// One stored work credit as [`works_json_sql`] holds it.
#[derive(serde::Deserialize)]
pub(crate) struct StoredWorkCredit {
    name: String,
    sort_name: Option<String>,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
    credited_as: Option<String>,
    role: String,
    role_words: Option<String>,
    qualifiers: String,
}

/// One stored part as [`work_parts_json_sql`] holds it.
#[derive(serde::Deserialize)]
pub(crate) struct StoredPart {
    parent: String,
    child: String,
}

/// The work graph [`works_json_sql`] and [`work_parts_json_sql`] hold.
pub(crate) fn works_of(
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

/// One stored role as [`release_roles_json_sql`] and [`track_roles_json_sql`] hold
/// it, its source still the text [`roles_of`] reads.
#[derive(serde::Deserialize)]
pub(crate) struct StoredRole {
    name: String,
    sort_name: Option<String>,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
    credited_as: Option<String>,
    source: String,
    role: String,
    role_words: Option<String>,
    qualifiers: String,
}

/// The credited roles a roles column holds.
pub(crate) fn roles_of(roles: Vec<StoredRole>) -> Result<Vec<CreditedRole>, DbError> {
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
                role: CreditedAs::from_columns(&stored.role, stored.role_words, &stored.qualifiers)
                    .map_err(|error| DbError::unreadable("a draft role's stored role", error))?,
            })
        })
        .collect()
}
