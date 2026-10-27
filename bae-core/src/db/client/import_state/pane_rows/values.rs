//! The draft's values besides its editable fields: the roles the source
//! credits on each track and on the release, and the works the tracks perform
//! with the works they nest among.
//!
//! They hang off the draft row (and each track's off its track row), so
//! replacing the draft takes them with it.

use super::*;
use crate::import::{CreditedRole, GraphWork, WorkGraph, WorkPart};

/// Write the values `draft` holds beside its rows. The draft row and its
/// track rows are already written.
pub(super) fn insert_draft_values(
    sql: &SqlContext<'_, '_>,
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
        for (position, composer) in work.composers.iter().enumerate() {
            sql.execute(
                "INSERT INTO import_candidate_work_composer \
                 (content_hash, musicbrainz_work_id, position, name, sort_name, \
                  musicbrainz_artist_id, discogs_artist_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    content_hash,
                    work.musicbrainz_work_id,
                    position as i64,
                    composer.name,
                    composer.sort_name,
                    composer.musicbrainz_artist_id,
                    composer.discogs_artist_id,
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
              discogs_artist_id, source, role) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                position as i64,
                role.artist.name,
                role.artist.sort_name,
                role.artist.musicbrainz_artist_id,
                role.artist.discogs_artist_id,
                role.source.as_str(),
                role.role,
            ],
        )?;
    }
    for track in &draft.tracks {
        for (position, role) in track.roles.iter().enumerate() {
            sql.execute(
                "INSERT INTO import_candidate_track_role \
                 (content_hash, track_id, position, name, sort_name, musicbrainz_artist_id, \
                  discogs_artist_id, source, role) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    content_hash,
                    track.edit.id,
                    position as i64,
                    role.artist.name,
                    role.artist.sort_name,
                    role.artist.musicbrainz_artist_id,
                    role.artist.discogs_artist_id,
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
    }
    Ok(())
}

/// Every candidate's stored values, or those of the one `only` names.
#[derive(Default)]
pub(super) struct StoredDraftValues {
    pub works: HashMap<String, WorkGraph>,
    pub release_roles: HashMap<String, Vec<CreditedRole>>,
    pub track_roles: HashMap<(String, String), Vec<CreditedRole>>,
    pub track_works: HashMap<(String, String), Vec<String>>,
}

pub(super) fn load_draft_values_on(
    sql: &SqlReadContext<'_>,
    only: CandidateRows<'_>,
) -> Result<StoredDraftValues, DbError> {
    let filter = only.filter("content_hash");
    let clause = &filter.clause;
    let mut values = StoredDraftValues::default();
    let works = sql.query(
        &format!(
            "SELECT content_hash, musicbrainz_work_id, title, disambiguation, work_type \
             FROM import_candidate_work {clause} \
             ORDER BY content_hash, position"
        ),
        filter.params().as_slice(),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                GraphWork {
                    musicbrainz_work_id: row.get(1)?,
                    title: row.get(2)?,
                    disambiguation: row.get(3)?,
                    work_type: row.get(4)?,
                    composers: Vec::new(),
                },
            ))
        },
    )?;
    for (content_hash, work) in works {
        values
            .works
            .entry(content_hash)
            .or_default()
            .works
            .push(work);
    }
    let composers = sql.query(
        &format!(
            "SELECT content_hash, musicbrainz_work_id, name, sort_name, musicbrainz_artist_id, \
                discogs_artist_id \
             FROM import_candidate_work_composer {clause} \
             ORDER BY content_hash, musicbrainz_work_id, position"
        ),
        filter.params().as_slice(),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                ArtistCredit {
                    name: row.get(2)?,
                    sort_name: row.get(3)?,
                    musicbrainz_artist_id: row.get(4)?,
                    discogs_artist_id: row.get(5)?,
                    credited_as: None,
                },
            ))
        },
    )?;
    for (content_hash, work_id, composer) in composers {
        values
            .works
            .get_mut(&content_hash)
            .and_then(|graph| {
                graph
                    .works
                    .iter_mut()
                    .find(|work| work.musicbrainz_work_id == work_id)
            })
            .ok_or_else(|| {
                DbError::Message(format!(
                    "candidate {content_hash} credits a composer on work {work_id}, which it does not hold"
                ))
            })?
            .composers
            .push(composer);
    }
    let parts = sql.query(
        &format!(
            "SELECT content_hash, parent_work_id, child_work_id FROM import_candidate_work_part \
             {clause} ORDER BY content_hash, position"
        ),
        filter.params().as_slice(),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                WorkPart {
                    parent: row.get(1)?,
                    child: row.get(2)?,
                },
            ))
        },
    )?;
    for (content_hash, part) in parts {
        values
            .works
            .entry(content_hash)
            .or_default()
            .parts
            .push(part);
    }
    for (content_hash, role) in sql.query(
        &format!(
            "SELECT content_hash, name, sort_name, musicbrainz_artist_id, discogs_artist_id, \
                source, role \
             FROM import_candidate_release_role {clause} \
             ORDER BY content_hash, position"
        ),
        filter.params().as_slice(),
        |row| Ok((row.get::<_, String>(0)?, role_columns(row, 1)?)),
    )? {
        values
            .release_roles
            .entry(content_hash)
            .or_default()
            .push(role.read()?);
    }
    for (content_hash, track_id, role) in sql.query(
        &format!(
            "SELECT content_hash, track_id, name, sort_name, musicbrainz_artist_id, \
                discogs_artist_id, source, role \
             FROM import_candidate_track_role {clause} \
             ORDER BY content_hash, track_id, position"
        ),
        filter.params().as_slice(),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                role_columns(row, 2)?,
            ))
        },
    )? {
        values
            .track_roles
            .entry((content_hash, track_id))
            .or_default()
            .push(role.read()?);
    }
    for (content_hash, track_id, work) in sql.query(
        &format!(
            "SELECT content_hash, track_id, musicbrainz_work_id FROM import_candidate_track_work \
             {clause} \
             ORDER BY content_hash, track_id, position"
        ),
        filter.params().as_slice(),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )? {
        values
            .track_works
            .entry((content_hash, track_id))
            .or_default()
            .push(work);
    }
    Ok(values)
}

/// A stored role as SQLite hands it over, its source not yet read.
struct RoleColumns {
    artist: ArtistCredit,
    source: String,
    role: Option<String>,
}

impl RoleColumns {
    fn read(self) -> Result<CreditedRole, DbError> {
        Ok(CreditedRole {
            artist: self.artist,
            source: Catalog::from_str(&self.source).map_err(DbError::Message)?,
            role: self.role,
        })
    }
}

fn role_columns(row: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<RoleColumns> {
    Ok(RoleColumns {
        artist: ArtistCredit {
            name: row.get(first)?,
            sort_name: row.get(first + 1)?,
            musicbrainz_artist_id: row.get(first + 2)?,
            discogs_artist_id: row.get(first + 3)?,
            credited_as: None,
        },
        source: row.get(first + 4)?,
        role: row.get(first + 5)?,
    })
}
